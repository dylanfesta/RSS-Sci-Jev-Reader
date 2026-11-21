//! Automatic fallback abstract enrichment for RSS entries with missing or 1–2 sentence summaries.
//!
//! Queries free, keyless scholarly APIs in waterfall order:
//!   1. arXiv Atom API (when an arXiv ID is present)
//!   2. Crossref REST API (by DOI, or strict title match which also resolves missing DOIs)
//!   3. OpenAlex REST API (reconstructing `abstract_inverted_index` via DOI or strict title match)
//!   4. Europe PMC REST API (`abstractText` via DOI or strict title match)
//!   5. Semantic Scholar Graph API (`abstract` via DOI or arXiv ID)
//!
//! Stops immediately once a genuine multi-sentence paragraph abstract is found and silently
//! updates `entry["summary"]` in place.

use crate::rss_service::unescape_html_entities;
use quick_xml::events::Event;
use quick_xml::Reader;
use regex::Regex;
use serde_json::Value;
use std::collections::HashSet;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

pub const POLITE_USER_AGENT: &str = concat!(
    "SmartRSSReader/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/dylanfesta/RSS-Sci-Jev-Reader; scholarly-rss-reader)"
);

const MAX_CONCURRENT_ENRICHMENTS: usize = 4;

#[derive(Debug, Clone)]
pub struct EnricherEndpoints {
    pub arxiv_api_url: String,
    pub crossref_api_url: String,
    pub openalex_api_url: String,
    pub europepmc_api_url: String,
    pub semanticscholar_api_url: String,
    pub allow_local_or_example_urls: bool,
}

impl Default for EnricherEndpoints {
    fn default() -> Self {
        Self {
            arxiv_api_url: "https://export.arxiv.org/api/query".to_string(),
            crossref_api_url: "https://api.crossref.org".to_string(),
            openalex_api_url: "https://api.openalex.org".to_string(),
            europepmc_api_url: "https://www.ebi.ac.uk/europepmc/webservices/rest".to_string(),
            semanticscholar_api_url: "https://api.semanticscholar.org/graph/v1".to_string(),
            allow_local_or_example_urls: false,
        }
    }
}

pub fn percent_encode_component(input: &str) -> String {
    let mut out = String::with_capacity(input.len() + 16);
    for &b in input.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

pub fn encode_doi_path(doi: &str) -> String {
    doi.split('/')
        .map(percent_encode_component)
        .collect::<Vec<_>>()
        .join("/")
}

fn decode_basic_url_escapes(input: &str) -> String {
    let mut s = input.to_string();
    for (pat, rep) in [
        ("%2F", "/"),
        ("%2f", "/"),
        ("%3A", ":"),
        ("%3a", ":"),
        ("%28", "("),
        ("%29", ")"),
        ("%3B", ";"),
        ("%3b", ";"),
    ] {
        if s.contains(pat) {
            s = s.replace(pat, rep);
        }
    }
    s
}

fn clean_extracted_doi(raw_doi: &str) -> Option<String> {
    let mut s = raw_doi.trim().to_string();
    // Strip query parameters or fragment identifiers if extracted from a URL
    if let Some((before_q, _)) = s.split_once('?') {
        s = before_q.to_string();
    }
    if let Some((before_hash, _)) = s.split_once('#') {
        s = before_hash.to_string();
    }

    // Trim trailing punctuation while preserving balanced parentheses (e.g. Elsevier/Cell PII DOIs)
    loop {
        let trimmed = s.trim_end_matches(&['.', ',', ';', ':', ']', '}', '>', '\'', '"'][..]);
        if trimmed.len() != s.len() {
            s = trimmed.to_string();
            continue;
        }
        if s.ends_with(')') {
            let open_cnt = s.chars().filter(|&c| c == '(').count();
            let close_cnt = s.chars().filter(|&c| c == ')').count();
            if close_cnt > open_cnt {
                s.pop();
                continue;
            }
        }
        break;
    }

    // Strip common publisher URL suffixes attached to DOIs in article links
    for suffix in [
        ".full.pdf",
        ".pdf",
        ".epdf",
        ".full",
        ".short",
        ".abstract",
        "/full",
        "/abstract",
        "/pdf",
        "/epdf",
        "/meta",
        "/suppinfo",
    ] {
        if s.len() > suffix.len() + 8 && s.to_ascii_lowercase().ends_with(suffix) {
            s.truncate(s.len() - suffix.len());
            break;
        }
    }

    // Strip version suffixes on bioRxiv / medRxiv / arXiv DOIs (e.g. 10.1101/2026.11.18.624123v1)
    let lower = s.to_ascii_lowercase();
    if lower.starts_with("10.1101/")
        || lower.starts_with("10.64898/")
        || lower.starts_with("10.48550/arxiv.")
    {
        static VERSION_RE: OnceLock<Regex> = OnceLock::new();
        let vre = VERSION_RE.get_or_init(|| Regex::new(r"(?i)v\d+$").unwrap());
        s = vre.replace(&s, "").into_owned();
    }

    if s.starts_with("10.") && s.contains('/') && s.len() >= 8 {
        Some(s)
    } else {
        None
    }
}

pub fn extract_doi_from_text(raw: &str) -> Option<String> {
    if raw.trim().is_empty() {
        return None;
    }
    let decoded = unescape_html_entities(&decode_basic_url_escapes(raw));

    static DOI_RE: OnceLock<Regex> = OnceLock::new();
    let doi_re = DOI_RE.get_or_init(|| Regex::new(r"(?i)\b(10\.\d{4,9}/[^\s\x22'<>]+)").unwrap());

    if let Some(caps) = doi_re.captures(&decoded) {
        if let Some(m) = caps.get(1) {
            if let Some(clean) = clean_extracted_doi(m.as_str()) {
                return Some(clean);
            }
        }
    }

    // Fallback: Nature article URLs (https://www.nature.com/articles/s41586-026-08123-4 -> 10.1038/...)
    static NATURE_URL_RE: OnceLock<Regex> = OnceLock::new();
    let nat_re = NATURE_URL_RE.get_or_init(|| {
        Regex::new(r"(?i)https?://(?:www\.)?nature\.com/articles/([a-z0-9][a-z0-9._-]+)").unwrap()
    });
    if let Some(caps) = nat_re.captures(&decoded) {
        if let Some(m) = caps.get(1) {
            return clean_extracted_doi(&format!("10.1038/{}", m.as_str()));
        }
    }

    None
}

pub fn extract_arxiv_id_from_text(raw: &str) -> Option<String> {
    if raw.trim().is_empty() {
        return None;
    }
    let decoded = unescape_html_entities(&decode_basic_url_escapes(raw));

    static ARXIV_DOI_RE: OnceLock<Regex> = OnceLock::new();
    let doi_re = ARXIV_DOI_RE
        .get_or_init(|| Regex::new(r"(?i)\b10\.48550/arxiv\.(\d{4}\.\d{4,5})(?:v\d+)?\b").unwrap());
    if let Some(caps) = doi_re.captures(&decoded) {
        if let Some(m) = caps.get(1) {
            return Some(m.as_str().to_string());
        }
    }

    static ARXIV_ID_RE: OnceLock<Regex> = OnceLock::new();
    let id_re = ARXIV_ID_RE.get_or_init(|| {
        Regex::new(
            r"(?i)(?:arxiv\.org/(?:abs|pdf|html)/|oai:arxiv\.org:|arxiv:)(\d{4}\.\d{4,5}|[a-z-]+/\d{7})(?:v\d+)?(?:\.pdf)?\b",
        )
        .unwrap()
    });
    if let Some(caps) = id_re.captures(&decoded) {
        if let Some(m) = caps.get(1) {
            return Some(m.as_str().to_string());
        }
    }

    None
}

pub fn extract_identifiers_from_parts(parts: &[&str]) -> (Option<String>, Option<String>) {
    let mut doi: Option<String> = None;
    let mut arxiv_id: Option<String> = None;

    for part in parts {
        if doi.is_none() {
            doi = extract_doi_from_text(part);
        }
        if arxiv_id.is_none() {
            arxiv_id = extract_arxiv_id_from_text(part);
        }
        if doi.is_some() && arxiv_id.is_some() {
            break;
        }
    }

    if doi.is_none() {
        if let Some(ref aid) = arxiv_id {
            if aid.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                doi = Some(format!("10.48550/arXiv.{aid}"));
            }
        }
    }

    (doi, arxiv_id)
}

pub fn clean_abstract_markup(raw: &str) -> String {
    if raw.trim().is_empty() {
        return String::new();
    }

    static TITLE_TAG_RE: OnceLock<Regex> = OnceLock::new();
    static BLOCK_TAG_RE: OnceLock<Regex> = OnceLock::new();
    static INLINE_TAG_RE: OnceLock<Regex> = OnceLock::new();
    static ANY_TAG_RE: OnceLock<Regex> = OnceLock::new();
    static WS_RE: OnceLock<Regex> = OnceLock::new();
    static LEADING_LABEL_PUNCT_RE: OnceLock<Regex> = OnceLock::new();
    static LEADING_ABSTRACT_UPPER_RE: OnceLock<Regex> = OnceLock::new();

    let title_re = TITLE_TAG_RE
        .get_or_init(|| Regex::new(r"(?is)<(?:jats:)?title\b[^>]*>.*?</(?:jats:)?title>").unwrap());
    let block_re = BLOCK_TAG_RE.get_or_init(|| {
        Regex::new(r"(?i)</?(?:jats:)?(?:p|sec|list-item|div|br|tr|li|h[1-6])\b[^>]*>").unwrap()
    });
    let inline_re = INLINE_TAG_RE.get_or_init(|| {
        Regex::new(
            r"(?i)</?(?:jats:)?(?:italic|bold|sub|sup|sc|underline|monospace|span|i|b|em|strong|u|small|a)\b[^>]*>",
        )
        .unwrap()
    });
    let any_tag_re = ANY_TAG_RE.get_or_init(|| Regex::new(r"<[^>]+>").unwrap());
    let ws_re = WS_RE.get_or_init(|| Regex::new(r"\s+").unwrap());
    let lead_punct_re = LEADING_LABEL_PUNCT_RE
        .get_or_init(|| Regex::new(r"(?i)^(?:abstract|summary)\s*[:.\-—]+\s*").unwrap());
    let lead_upper_re = LEADING_ABSTRACT_UPPER_RE
        .get_or_init(|| Regex::new(r"^(?:ABSTRACT|Abstract|SUMMARY|Summary)\s+([A-Z])").unwrap());

    let s = title_re.replace_all(raw, "\n");
    let s = block_re.replace_all(&s, "\n");
    let s = inline_re.replace_all(&s, "");
    let s = any_tag_re.replace_all(&s, " ");
    let s = unescape_html_entities(&s);

    let mut lines: Vec<String> = s
        .lines()
        .map(|line| ws_re.replace_all(line, " ").trim().to_string())
        .filter(|line| !line.is_empty())
        .collect();

    if let Some(first) = lines.first().cloned() {
        if matches!(
            first.to_ascii_lowercase().as_str(),
            "abstract" | "abstract:" | "summary" | "summary:"
        ) {
            lines.remove(0);
        }
    }

    let joined = lines.join("\n").trim().to_string();
    let stripped = lead_punct_re.replace(&joined, "").into_owned();
    lead_upper_re
        .replace(&stripped, "$1")
        .trim()
        .to_string()
}

pub fn reconstruct_openalex_abstract(inverted_index: &Value) -> Option<String> {
    let obj = inverted_index.as_object()?;
    if obj.is_empty() {
        return None;
    }

    let mut max_pos: usize = 0;
    let mut has_any = false;
    for positions in obj.values() {
        if let Some(arr) = positions.as_array() {
            for pos_val in arr {
                if let Some(p) = pos_val.as_u64() {
                    let idx = p as usize;
                    if idx > 25_000 {
                        continue;
                    }
                    has_any = true;
                    if idx > max_pos {
                        max_pos = idx;
                    }
                }
            }
        }
    }

    if !has_any {
        return None;
    }

    let mut slots: Vec<Option<&str>> = vec![None; max_pos + 1];
    for (word, positions) in obj {
        if let Some(arr) = positions.as_array() {
            for pos_val in arr {
                if let Some(p) = pos_val.as_u64() {
                    let idx = p as usize;
                    if idx <= max_pos {
                        slots[idx] = Some(word.as_str());
                    }
                }
            }
        }
    }

    let raw_joined = slots
        .into_iter()
        .flatten()
        .filter(|w| !w.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    let cleaned = clean_abstract_markup(&raw_joined);
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

pub fn is_non_abstract_article_title(title: &str) -> bool {
    let lower = title.trim().to_ascii_lowercase();
    const SKIP_PREFIXES: &[&str] = &[
        "author correction:",
        "publisher correction:",
        "correction:",
        "correction to:",
        "erratum:",
        "erratum to:",
        "corrigendum:",
        "corrigendum to:",
        "retraction:",
        "retraction note:",
        "expression of concern:",
    ];
    SKIP_PREFIXES.iter().any(|p| lower.starts_with(p))
}

pub fn strip_rss_boilerplate_for_check(raw: &str) -> String {
    if raw.trim().is_empty() {
        return String::new();
    }

    static NATURE_META_LINE_RE: OnceLock<Regex> = OnceLock::new();
    static CITATION_LINE_RE: OnceLock<Regex> = OnceLock::new();
    static ARXIV_LINE_RE: OnceLock<Regex> = OnceLock::new();
    static KV_META_LINE_RE: OnceLock<Regex> = OnceLock::new();
    static INLINE_NATURE_PREFIX_RE: OnceLock<Regex> = OnceLock::new();
    static INLINE_ARXIV_PREFIX_RE: OnceLock<Regex> = OnceLock::new();
    static LEADING_LABEL_RE: OnceLock<Regex> = OnceLock::new();

    let nat_re = NATURE_META_LINE_RE.get_or_init(|| {
        Regex::new(r"(?i)^[^.!?]{1,110},\s*published online:\s*.*doi:\s*10\.\d{4,9}/\S+\s*$")
            .unwrap()
    });
    let cit_re = CITATION_LINE_RE.get_or_init(|| {
        Regex::new(
            r"(?i)^[^.!?]{1,110},\s*(?:volume|vol\.?|issue|no\.?|ahead of print)\b.*(?:issue|page|pp\.?|\b20\d{2}\b).*$",
        )
        .unwrap()
    });
    let arxiv_re =
        ARXIV_LINE_RE.get_or_init(|| Regex::new(r"(?i)^arxiv:\S+\s+announce type:\s*\S+$").unwrap());
    let kv_re = KV_META_LINE_RE.get_or_init(|| {
        Regex::new(
            r"(?i)^(?:doi|pmid|pmcid|url|link|published(?: online)?|publication date|authors?|source)\s*:\s*.+$",
        )
        .unwrap()
    });
    let inline_nat_re = INLINE_NATURE_PREFIX_RE.get_or_init(|| {
        Regex::new(r"(?i)^[^.!?]{1,110},\s*published online:[^.!?]*?doi:\s*10\.\d{4,9}/\S+\s+")
            .unwrap()
    });
    let inline_arxiv_re = INLINE_ARXIV_PREFIX_RE
        .get_or_init(|| Regex::new(r"(?i)^arxiv:\S+\s+announce type:\s*\S+\s+").unwrap());
    let lead_label_re = LEADING_LABEL_RE.get_or_init(|| {
        Regex::new(r"(?i)^(?:abstract|summary|graphical abstract)\s*[:.\-—]\s*").unwrap()
    });

    let cleaned = clean_abstract_markup(raw);
    let kept_lines: Vec<&str> = cleaned
        .lines()
        .map(|l| l.trim())
        .filter(|l| {
            if l.is_empty() {
                return false;
            }
            if nat_re.is_match(l) || cit_re.is_match(l) || arxiv_re.is_match(l) {
                return false;
            }
            if kv_re.is_match(l) && count_words(l) <= 18 {
                return false;
            }
            true
        })
        .collect();

    let joined = kept_lines.join(" ");
    let s = inline_nat_re.replace(&joined, "");
    let s = inline_arxiv_re.replace(&s, "");
    let s = lead_label_re.replace(&s, "");
    let trimmed = s.trim();

    let lower = trimmed.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        ""
            | "no abstract"
            | "no abstract."
            | "no abstract available"
            | "no abstract available."
            | "abstract not available"
            | "abstract not available."
            | "[no abstract]"
            | "n/a"
            | "none"
            | "graphical abstract"
            | "graphical abstract."
            | "graphical abstract available"
            | "graphical abstract available."
    ) || lower.starts_with("no abstract available")
        || lower.starts_with("abstract not available")
    {
        return String::new();
    }

    trimmed.to_string()
}

pub fn count_words(text: &str) -> usize {
    text.split_whitespace()
        .filter(|tok| tok.chars().any(|c| c.is_alphanumeric()))
        .count()
}

pub fn count_sentences(text: &str) -> usize {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return 0;
    }

    static ABBR_RE: OnceLock<Regex> = OnceLock::new();
    static INITIAL_RE: OnceLock<Regex> = OnceLock::new();
    static DECIMAL_RE: OnceLock<Regex> = OnceLock::new();
    static SENT_SPLIT_RE: OnceLock<Regex> = OnceLock::new();

    let abbr_re = ABBR_RE.get_or_init(|| {
        Regex::new(
            r"(?i)\b(?:et\s+al|e\.g|i\.e|vs|v|cf|ca|approx|fig|figs|eq|eqs|ref|refs|dr|prof|mr|mrs|ms|jr|sr|vol|no|pp|ch|sec|sp|spp|subsp|var|inc|ltd|co|u\.s|u\.k)\.",
        )
        .unwrap()
    });
    let init_re = INITIAL_RE.get_or_init(|| Regex::new(r"\b([A-Z])\.").unwrap());
    let dec_re = DECIMAL_RE.get_or_init(|| Regex::new(r"(\d)\.(\d)").unwrap());
    let split_re = SENT_SPLIT_RE.get_or_init(|| Regex::new(r"[.!?]+(?:\s+|$)").unwrap());

    let protected = abbr_re.replace_all(trimmed, "ABBR");
    let protected = init_re.replace_all(&protected, "$1");
    let protected = dec_re.replace_all(&protected, "${1}_${2}");

    split_re
        .split(&protected)
        .filter(|seg| count_words(seg) >= 2)
        .count()
        .max(1)
}

pub fn effective_abstract_word_count(summary: &str) -> usize {
    let effective = strip_rss_boilerplate_for_check(summary);
    count_words(&effective)
}

/// Returns `true` ONLY when an entry lacks a full-paragraph abstract (e.g. empty, placeholder,
/// or just 1–2 short teaser sentences rather than a paragraph), excluding errata/corrections.
pub fn needs_abstract_enrichment(summary: &str, title: &str) -> bool {
    if is_non_abstract_article_title(title) {
        return false;
    }
    let effective = strip_rss_boilerplate_for_check(summary);
    if effective.is_empty() {
        return true;
    }
    let words = count_words(&effective);
    if words < 40 {
        return true;
    }
    let sentences = count_sentences(&effective);
    sentences <= 2 && words < 80
}

fn normalize_title_for_match(raw: &str) -> Vec<String> {
    let cleaned = clean_abstract_markup(raw).to_lowercase();
    let mut tokens = Vec::new();
    let mut current = String::new();
    for ch in cleaned.chars() {
        if ch.is_alphanumeric() {
            current.push(ch);
        } else if !current.is_empty() {
            tokens.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

pub fn titles_match_strictly(expected_title: &str, candidate_title: &str) -> bool {
    let a = normalize_title_for_match(expected_title);
    let b = normalize_title_for_match(candidate_title);
    if a.len() < 4 || b.len() < 4 {
        return false;
    }
    if a == b {
        return true;
    }
    let len_diff = a.len().abs_diff(b.len());
    if len_diff > 3 {
        return false;
    }
    let set_a: HashSet<&str> = a.iter().map(|s| s.as_str()).collect();
    let set_b: HashSet<&str> = b.iter().map(|s| s.as_str()).collect();
    let inter = set_a.intersection(&set_b).count();
    let union = set_a.union(&set_b).count();
    union > 0 && (inter as f64 / union as f64) >= 0.90
}

fn is_local_or_reserved_example(feed_url: &str, link: &str, doi: &str) -> bool {
    let f = feed_url.trim().to_ascii_lowercase();
    let l = link.trim().to_ascii_lowercase();
    let d = doi.trim().to_ascii_lowercase();
    if f.starts_with("file://") {
        return true;
    }
    if d.starts_with("10.1000/") || d.starts_with("10.5555/") {
        return true;
    }
    for reserved in ["//example.org", "//www.example.org", "//example.com", "//www.example.com"] {
        if l.contains(reserved) || f.contains(reserved) {
            return true;
        }
    }
    false
}

fn parse_arxiv_atom_summary(xml_bytes: &[u8]) -> Option<String> {
    let xml_str = String::from_utf8_lossy(xml_bytes);
    let mut reader = Reader::from_str(&xml_str);
    reader.config_mut().trim_text(false);

    let mut in_entry = false;
    let mut in_summary = false;
    let mut buf = String::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_ascii_lowercase();
                if tag == "entry" {
                    in_entry = true;
                } else if in_entry && tag == "summary" {
                    in_summary = true;
                    buf.clear();
                }
            }
            Ok(Event::Text(e)) if in_summary => {
                if let Ok(t) = e.unescape() {
                    buf.push_str(&t);
                }
            }
            Ok(Event::CData(e)) if in_summary => {
                buf.push_str(&String::from_utf8_lossy(e.as_ref()));
            }
            Ok(Event::End(e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_ascii_lowercase();
                if in_entry && tag == "summary" {
                    let cleaned = clean_abstract_markup(&buf);
                    if !cleaned.is_empty() {
                        return Some(cleaned);
                    }
                    in_summary = false;
                } else if tag == "entry" {
                    break;
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    None
}

async fn fetch_arxiv_abstract(
    client: &reqwest::Client,
    endpoints: &EnricherEndpoints,
    arxiv_id: &str,
) -> Option<String> {
    let url = format!(
        "{}?id_list={}",
        endpoints.arxiv_api_url.trim_end_matches('/'),
        percent_encode_component(arxiv_id)
    );
    let resp = client.get(&url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let bytes = resp.bytes().await.ok()?;
    parse_arxiv_atom_summary(&bytes)
}

async fn fetch_crossref_abstract(
    client: &reqwest::Client,
    endpoints: &EnricherEndpoints,
    doi: Option<&str>,
    title: &str,
) -> (Option<String>, Option<String>) {
    let base = endpoints.crossref_api_url.trim_end_matches('/');

    if let Some(d) = doi.filter(|s| !s.is_empty()) {
        let url = format!("{}/works/{}", base, encode_doi_path(d));
        if let Ok(resp) = client.get(&url).header("Accept", "application/json").send().await {
            if resp.status().is_success() {
                if let Ok(val) = resp.json::<Value>().await {
                    let raw_abs = val
                        .get("message")
                        .and_then(|m| m.get("abstract"))
                        .and_then(|a| a.as_str())
                        .map(clean_abstract_markup)
                        .filter(|s| !s.is_empty());
                    return (raw_abs, None);
                }
            }
        }
        return (None, None);
    }

    if normalize_title_for_match(title).len() < 4 {
        return (None, None);
    }

    let url = format!(
        "{}/works?query.title={}&rows=3",
        base,
        percent_encode_component(title)
    );
    let Ok(resp) = client.get(&url).header("Accept", "application/json").send().await else {
        return (None, None);
    };
    if !resp.status().is_success() {
        return (None, None);
    }
    let Ok(val) = resp.json::<Value>().await else {
        return (None, None);
    };
    let Some(items) = val
        .get("message")
        .and_then(|m| m.get("items"))
        .and_then(|i| i.as_array())
    else {
        return (None, None);
    };

    for item in items {
        let cand_title = item
            .get("title")
            .and_then(|t| t.as_array())
            .and_then(|arr| arr.first())
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if titles_match_strictly(title, cand_title) {
            let resolved_doi = item
                .get("DOI")
                .and_then(|v| v.as_str())
                .and_then(extract_doi_from_text);
            let abs = item
                .get("abstract")
                .and_then(|v| v.as_str())
                .map(clean_abstract_markup)
                .filter(|s| !s.is_empty());
            return (abs, resolved_doi);
        }
    }

    (None, None)
}

async fn fetch_openalex_abstract(
    client: &reqwest::Client,
    endpoints: &EnricherEndpoints,
    doi: Option<&str>,
    title: &str,
) -> (Option<String>, Option<String>) {
    let base = endpoints.openalex_api_url.trim_end_matches('/');

    if let Some(d) = doi.filter(|s| !s.is_empty()) {
        let url = format!(
            "{}/works/https://doi.org/{}?select=id,doi,title,abstract_inverted_index",
            base,
            encode_doi_path(d)
        );
        if let Ok(resp) = client.get(&url).header("Accept", "application/json").send().await {
            if resp.status().is_success() {
                if let Ok(val) = resp.json::<Value>().await {
                    let abs = val
                        .get("abstract_inverted_index")
                        .and_then(reconstruct_openalex_abstract);
                    return (abs, None);
                }
            }
        }
        return (None, None);
    }

    if normalize_title_for_match(title).len() < 4 {
        return (None, None);
    }

    let url = format!(
        "{}/works?search={}&per-page=3&select=id,doi,title,abstract_inverted_index",
        base,
        percent_encode_component(title)
    );
    let Ok(resp) = client.get(&url).header("Accept", "application/json").send().await else {
        return (None, None);
    };
    if !resp.status().is_success() {
        return (None, None);
    }
    let Ok(val) = resp.json::<Value>().await else {
        return (None, None);
    };
    let Some(results) = val.get("results").and_then(|r| r.as_array()) else {
        return (None, None);
    };

    for work in results {
        let cand_title = work
            .get("title")
            .or_else(|| work.get("display_name"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if titles_match_strictly(title, cand_title) {
            let resolved_doi = work
                .get("doi")
                .and_then(|v| v.as_str())
                .and_then(extract_doi_from_text);
            let abs = work
                .get("abstract_inverted_index")
                .and_then(reconstruct_openalex_abstract);
            return (abs, resolved_doi);
        }
    }

    (None, None)
}

async fn fetch_europepmc_abstract(
    client: &reqwest::Client,
    endpoints: &EnricherEndpoints,
    doi: Option<&str>,
    title: &str,
) -> Option<String> {
    let base = endpoints.europepmc_api_url.trim_end_matches('/');

    if let Some(d) = doi.filter(|s| !s.is_empty()) {
        let query = format!("DOI:\"{}\"", d);
        let url = format!(
            "{}/search?query={}&resultType=core&format=json&pageSize=1",
            base,
            percent_encode_component(&query)
        );
        if let Ok(resp) = client.get(&url).header("Accept", "application/json").send().await {
            if resp.status().is_success() {
                if let Ok(val) = resp.json::<Value>().await {
                    if let Some(abs) = val
                        .get("resultList")
                        .and_then(|rl| rl.get("result"))
                        .and_then(|arr| arr.as_array())
                        .and_then(|arr| arr.first())
                        .and_then(|item| item.get("abstractText"))
                        .and_then(|v| v.as_str())
                        .map(clean_abstract_markup)
                        .filter(|s| !s.is_empty())
                    {
                        return Some(abs);
                    }
                }
            }
        }
        return None;
    }

    if normalize_title_for_match(title).len() < 4 {
        return None;
    }

    let safe_title = title.replace('"', " ");
    let query = format!("TITLE:\"{}\"", safe_title);
    let url = format!(
        "{}/search?query={}&resultType=core&format=json&pageSize=3",
        base,
        percent_encode_component(&query)
    );
    let resp = client.get(&url).header("Accept", "application/json").send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let val = resp.json::<Value>().await.ok()?;
    let results = val
        .get("resultList")
        .and_then(|rl| rl.get("result"))
        .and_then(|arr| arr.as_array())?;

    for item in results {
        let cand_title = item.get("title").and_then(|v| v.as_str()).unwrap_or("");
        if titles_match_strictly(title, cand_title) {
            if let Some(abs) = item
                .get("abstractText")
                .and_then(|v| v.as_str())
                .map(clean_abstract_markup)
                .filter(|s| !s.is_empty())
            {
                return Some(abs);
            }
        }
    }
    None
}

async fn fetch_semanticscholar_abstract(
    client: &reqwest::Client,
    endpoints: &EnricherEndpoints,
    doi: Option<&str>,
    arxiv_id: Option<&str>,
) -> Option<String> {
    let base = endpoints.semanticscholar_api_url.trim_end_matches('/');
    let paper_id = if let Some(d) = doi.filter(|s| !s.is_empty()) {
        format!("DOI:{}", encode_doi_path(d))
    } else if let Some(aid) = arxiv_id.filter(|s| !s.is_empty()) {
        format!("ARXIV:{}", percent_encode_component(aid))
    } else {
        return None;
    };

    let url = format!("{}/paper/{}?fields=title,abstract", base, paper_id);
    let resp = client.get(&url).header("Accept", "application/json").send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let val = resp.json::<Value>().await.ok()?;
    val.get("abstract")
        .and_then(|v| v.as_str())
        .map(clean_abstract_markup)
        .filter(|s| !s.is_empty())
}

async fn enrich_single_entry(
    client: &reqwest::Client,
    endpoints: &EnricherEndpoints,
    title: String,
    existing_summary: String,
    link: String,
    feed_url: String,
    initial_doi: Option<String>,
    initial_arxiv_id: Option<String>,
) -> Option<String> {
    if !needs_abstract_enrichment(&existing_summary, &title) {
        return None;
    }

    let (inferred_doi, inferred_arxiv) =
        extract_identifiers_from_parts(&[&link, &existing_summary, &feed_url]);
    let mut doi = initial_doi.filter(|s| !s.is_empty()).or(inferred_doi);
    let arxiv_id = initial_arxiv_id.filter(|s| !s.is_empty()).or(inferred_arxiv);

    if !endpoints.allow_local_or_example_urls
        && is_local_or_reserved_example(&feed_url, &link, doi.as_deref().unwrap_or(""))
    {
        return None;
    }

    let orig_words = effective_abstract_word_count(&existing_summary);
    let mut best_fallback: Option<(usize, String)> = None;

    let mut evaluate_candidate = |cand: Option<String>| -> Option<String> {
        let cleaned = cand?;
        let cand_words = effective_abstract_word_count(&cleaned);
        if cand_words <= orig_words + 15 {
            return None;
        }
        if !needs_abstract_enrichment(&cleaned, &title) {
            return Some(cleaned);
        }
        if cand_words >= 25 {
            let replace_best = match best_fallback {
                Some((prev_w, _)) => cand_words > prev_w,
                None => true,
            };
            if replace_best {
                best_fallback = Some((cand_words, cleaned));
            }
        }
        None
    };

    // 1. arXiv Atom API (instant when arXiv ID is present)
    if let Some(ref aid) = arxiv_id {
        let cand = fetch_arxiv_abstract(client, endpoints, aid).await;
        if let Some(win) = evaluate_candidate(cand) {
            return Some(win);
        }
    }

    // 2. Crossref REST API (also resolves missing DOIs via strict title matching)
    {
        let (cand, resolved_doi) =
            fetch_crossref_abstract(client, endpoints, doi.as_deref(), &title).await;
        if doi.is_none() && resolved_doi.is_some() {
            doi = resolved_doi;
        }
        if let Some(win) = evaluate_candidate(cand) {
            return Some(win);
        }
    }

    // 3. OpenAlex REST API (reconstructs `abstract_inverted_index`)
    {
        let (cand, resolved_doi) =
            fetch_openalex_abstract(client, endpoints, doi.as_deref(), &title).await;
        if doi.is_none() && resolved_doi.is_some() {
            doi = resolved_doi;
        }
        if let Some(win) = evaluate_candidate(cand) {
            return Some(win);
        }
    }

    // 4. Europe PMC REST API
    {
        let cand = fetch_europepmc_abstract(client, endpoints, doi.as_deref(), &title).await;
        if let Some(win) = evaluate_candidate(cand) {
            return Some(win);
        }
    }

    // 5. Semantic Scholar Graph API
    {
        let cand = fetch_semanticscholar_abstract(
            client,
            endpoints,
            doi.as_deref(),
            arxiv_id.as_deref(),
        )
        .await;
        if let Some(win) = evaluate_candidate(cand) {
            return Some(win);
        }
    }

    // Only use a shorter partial fallback if the RSS feed had zero prose words at all
    if orig_words == 0 {
        if let Some((_, fb)) = best_fallback {
            return Some(fb);
        }
    }

    None
}

/// Inspects a batch of entry JSON objects, identifies only those missing a full-paragraph
/// abstract, and concurrently enriches their `"summary"` fields in place via free scholarly APIs.
pub async fn enrich_entries_missing_abstracts(
    entries: &mut [Value],
    endpoints: &EnricherEndpoints,
) -> usize {
    if entries.is_empty() {
        return 0;
    }

    let mut candidates: Vec<(
        usize,
        String,
        String,
        String,
        String,
        Option<String>,
        Option<String>,
    )> = Vec::new();

    for (idx, entry) in entries.iter().enumerate() {
        let title = entry
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let summary = entry
            .get("summary")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if !needs_abstract_enrichment(&summary, &title) {
            continue;
        }
        let link = entry
            .get("link")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let feed_url = entry
            .get("feed_url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let doi = entry
            .get("doi")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let arxiv_id = entry
            .get("arxiv_id")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        if !endpoints.allow_local_or_example_urls
            && is_local_or_reserved_example(&feed_url, &link, doi.as_deref().unwrap_or(""))
        {
            continue;
        }

        candidates.push((idx, title, summary, link, feed_url, doi, arxiv_id));
    }

    if candidates.is_empty() {
        return 0;
    }

    let uses_loopback = [
        &endpoints.arxiv_api_url,
        &endpoints.crossref_api_url,
        &endpoints.openalex_api_url,
        &endpoints.europepmc_api_url,
        &endpoints.semanticscholar_api_url,
    ]
    .iter()
    .any(|u| {
        let l = u.trim().to_ascii_lowercase();
        l.starts_with("http://127.0.0.1")
            || l.starts_with("https://127.0.0.1")
            || l.starts_with("http://localhost")
            || l.starts_with("https://localhost")
            || l.starts_with("http://[::1]")
            || l.starts_with("https://[::1]")
    });

    let mut builder = reqwest::Client::builder()
        .user_agent(POLITE_USER_AGENT)
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(6))
        .redirect(reqwest::redirect::Policy::limited(5));
    if uses_loopback {
        builder = builder.no_proxy();
    }
    let Ok(client) = builder.build() else {
        return 0;
    };

    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_ENRICHMENTS));
    let shared_endpoints = Arc::new(endpoints.clone());
    let mut join_set = JoinSet::new();

    for (idx, title, summary, link, feed_url, doi, arxiv_id) in candidates {
        let sem = Arc::clone(&semaphore);
        let cli = client.clone();
        let ep = Arc::clone(&shared_endpoints);
        join_set.spawn(async move {
            let _permit = sem.acquire_owned().await.ok()?;
            let enriched =
                enrich_single_entry(&cli, &ep, title, summary, link, feed_url, doi, arxiv_id)
                    .await?;
            Some((idx, enriched))
        });
    }

    let mut enriched_count = 0usize;
    while let Some(res) = join_set.join_next().await {
        if let Ok(Some((idx, new_summary))) = res {
            if let Some(obj) = entries.get_mut(idx).and_then(|v| v.as_object_mut()) {
                obj.insert("summary".to_string(), Value::String(new_summary));
                enriched_count += 1;
            }
        }
    }

    enriched_count
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_needs_abstract_enrichment_heuristics() {
        // 1. Empty and placeholder summaries need enrichment
        assert!(needs_abstract_enrichment("", "Cortical Circuit Dynamics"));
        assert!(needs_abstract_enrichment(
            "No abstract available.",
            "Cortical Circuit Dynamics"
        ));
        assert!(needs_abstract_enrichment(
            "Graphical abstract available",
            "Catalytic Cycle of Iron Complexes"
        ));

        // 2. Nature-style bibliographic header + 1-sentence standfirst needs enrichment
        let nature_teaser = "Nature, Published online: 20 November 2026; doi:10.1038/s41586-026-08123-4\n\
            Single-cell transcriptomics reveals how astrocyte state transitions regulate synaptic plasticity during sleep.";
        assert!(needs_abstract_enrichment(
            nature_teaser,
            "Astrocyte State Transitions Regulate Synaptic Plasticity"
        ));

        // 3. Science-style 2-sentence teaser needs enrichment
        let science_teaser = "Science, Volume 390, Issue 6775, Page 412-418, November 2026.\n\
            Cryo-electron microscopy captures gating transitions in human mechanosensitive channels. \
            These structures explain how lipid tension opens the transmembrane pore.";
        assert!(needs_abstract_enrichment(
            science_teaser,
            "Gating Transitions in Mechanosensitive Channels"
        ));

        // 4. Full multi-sentence paragraph abstract (including abbreviations like et al., Fig., e.g., decimals) MUST NOT trigger enrichment
        let full_abstract = "Understanding how neural circuits integrate sensory evidence across timescales remains a central challenge in systems neuroscience. \
            Building on prior observations (Smith et al., 2024; e.g. cortical gain modulation at p < 0.05), we recorded simultaneously from 12,400 neurons across six cortical areas in behaving mice. \
            We found that recurrentpopulation dynamics encode latent decision variables along low-dimensional manifolds that remain stable across weeks. \
            Optogenetic perturbation of feedback projections selectively disrupted integration over intervals longer than 1.5 seconds without impairing sensory detection. \
            Together, these findings establish a distributed circuit mechanism for hierarchical temporal integration.";
        assert!(!needs_abstract_enrichment(
            full_abstract,
            "Distributed Circuit Mechanisms for Temporal Integration"
        ));

        // 5. Errata / Author Corrections are skipped
        assert!(!needs_abstract_enrichment(
            "Correction to Fig. 2.",
            "Author Correction: Distributed Circuit Mechanisms for Temporal Integration"
        ));
        assert!(!needs_abstract_enrichment(
            "Erratum regarding author affiliation.",
            "Erratum: Quantum Hall Transport"
        ));
    }

    #[test]
    fn test_doi_and_arxiv_extraction() {
        // Nature URL slug -> 10.1038/...
        let (doi1, arx1) = extract_identifiers_from_parts(&[
            "https://www.nature.com/articles/s41586-026-08123-4?utm_source=feed",
        ]);
        assert_eq!(doi1.as_deref(), Some("10.1038/s41586-026-08123-4"));
        assert!(arx1.is_none());

        // Science link with /doi/abs/ and query param
        let (doi2, _) = extract_identifiers_from_parts(&[
            "https://www.science.org/doi/abs/10.1126/science.adq9876?af=R",
        ]);
        assert_eq!(doi2.as_deref(), Some("10.1126/science.adq9876"));

        // Elsevier SII DOI containing balanced parentheses
        let (doi3, _) =
            extract_identifiers_from_parts(&["doi:10.1016/S0896-6273(26)00812-3."]);
        assert_eq!(doi3.as_deref(), Some("10.1016/S0896-6273(26)00812-3"));

        // Versioned bioRxiv URL strips v1 suffix
        let (doi4, _) = extract_identifiers_from_parts(&[
            "https://www.biorxiv.org/content/10.1101/2026.11.18.624123v2.full",
        ]);
        assert_eq!(doi4.as_deref(), Some("10.1101/2026.11.18.624123"));

        // arXiv link extracts both arxiv_id and canonical arXiv DOI
        let (doi5, arx5) =
            extract_identifiers_from_parts(&["https://arxiv.org/abs/2611.12345v1"]);
        assert_eq!(arx5.as_deref(), Some("2611.12345"));
        assert_eq!(doi5.as_deref(), Some("10.48550/arXiv.2611.12345"));
    }

    #[test]
    fn test_jats_cleaning_and_openalex_inverted_index_reconstruction() {
        let jats = "<jats:title>Abstract</jats:title><jats:p>We study CO<jats:sub>2</jats:sub> reduction on <jats:italic>Cu</jats:italic> surfaces.</jats:p>";
        assert_eq!(
            clean_abstract_markup(jats),
            "We study CO2 reduction on Cu surfaces."
        );

        // Ensure "Abstract representations..." (adjective) is NOT stripped
        assert_eq!(
            clean_abstract_markup("Abstract representations emerge in prefrontal cortex."),
            "Abstract representations emerge in prefrontal cortex."
        );

        let inv = json!({
            "Abstract:": [0],
            "Quantum": [1],
            "coherence": [2, 6],
            "enhances": [3],
            "energy": [4],
            "transfer;": [5],
            "persists": [7],
            "at": [8],
            "300K.": [9]
        });
        assert_eq!(
            reconstruct_openalex_abstract(&inv).as_deref(),
            Some("Quantum coherence enhances energy transfer; coherence persists at 300K.")
        );
    }

    #[tokio::test]
    async fn test_arxiv_parsing_strict_titles_and_semanticscholar_fallthrough() {
        use axum::{
            extract::{Path as AxumPath, Query as AxumQuery},
            http::StatusCode,
            response::IntoResponse,
            routing::get,
            Json, Router,
        };
        use std::collections::HashMap;

        // 1. Strict title matching rejects short generic titles ("Editorial") and unrelated papers
        assert!(!titles_match_strictly("Editorial", "Editorial"));
        assert!(!titles_match_strictly("In Brief", "In Brief"));
        assert!(titles_match_strictly(
            "Single-Cell Multiomics of Human Cortical Development",
            "Single-cell multiomics of human cortical development."
        ));
        assert!(!titles_match_strictly(
            "Single-Cell Multiomics of Human Cortical Development",
            "Single-Cell Multiomics of Mouse Retinal Development"
        ));

        // 2. Mock server testing:
        //    - Entry A: arXiv ID resolved directly from arXiv Atom XML
        //    - Entry B: Crossref returns only a 1-sentence standfirst (not a full paragraph!) ->
        //               waterfall continues through OpenAlex (404) and Europe PMC (empty) to Semantic Scholar!
        let arxiv_atom = r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <title>ArXiv Query</title>
  <entry>
    <id>http://arxiv.org/abs/2611.54321v1</id>
    <title>Scalable Diffusion Transformers for Molecular Dynamics</title>
    <summary>  Equivariant score-based generative models enable rapid sampling of Boltzmann ensembles across biomolecular conformational landscapes.
Here we introduce a linear-attention diffusion transformer trained on millisecond-scale all-atom trajectories of 1,200 globular proteins.
Our architecture achieves a hundredfold speedup over conventional Langevin integration while preserving free-energy barriers and transition rates.
We validate zero-shot transferability on unseen membrane receptors and intrinsically disordered peptides.  </summary>
  </entry>
</feed>"#;

        let s2_full_abstract = " Non-equilibrium phase transitions in driven open quantum systems exhibit universality classes distinct from thermal equilibrium. \
            Using tensor-network simulations and trapped-ion experiments up to 56 spins, we map the dissipative critical point across varying Lindblad jump rates. \
            Finite-size scaling of steady-state entanglement entropy reveals anomalous dynamical exponents that violate classical Hohenberg-Halperin predictions. \
            These observations establish an experimental benchmark for driven-dissipative criticality.";

        let app = Router::new()
            .route(
                "/arxiv",
                get(move |AxumQuery(q): AxumQuery<HashMap<String, String>>| async move {
                    if q.get("id_list").map(|s| s.as_str()) == Some("2611.54321") {
                        (StatusCode::OK, arxiv_atom).into_response()
                    } else {
                        (StatusCode::NOT_FOUND, "").into_response()
                    }
                }),
            )
            .route(
                "/crossref/works/{*doi}",
                get(|AxumPath(doi): AxumPath<String>| async move {
                    if doi == "10.1103/PhysRevX.16.041099" {
                        // Crossref only has a 1-sentence teaser -> waterfall MUST keep looking!
                        (
                            StatusCode::OK,
                            Json(json!({
                                "message": {
                                    "DOI": doi,
                                    "abstract": "<jats:p>We study driven-dissipative criticality in trapped ions.</jats:p>"
                                }
                            })),
                        )
                            .into_response()
                    } else {
                        (StatusCode::NOT_FOUND, Json(json!({}))).into_response()
                    }
                }),
            )
            .route(
                "/openalex/works/{*doi_path}",
                get(|| async { (StatusCode::NOT_FOUND, Json(json!({}))) }),
            )
            .route(
                "/europepmc/search",
                get(|| async { Json(json!({ "resultList": { "result": [] } })) }),
            )
            .route(
                "/s2/paper/{*paper_id}",
                get(move |AxumPath(paper_id): AxumPath<String>| async move {
                    if paper_id == "DOI:10.1103/PhysRevX.16.041099" {
                        (
                            StatusCode::OK,
                            Json(json!({
                                "title": "Driven-Dissipative Criticality in Trapped-Ion Spin Chains",
                                "abstract": s2_full_abstract
                            })),
                        )
                            .into_response()
                    } else {
                        (StatusCode::NOT_FOUND, Json(json!({}))).into_response()
                    }
                }),
            );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        let endpoints = EnricherEndpoints {
            arxiv_api_url: format!("http://{}/arxiv", addr),
            crossref_api_url: format!("http://{}/crossref", addr),
            openalex_api_url: format!("http://{}/openalex", addr),
            europepmc_api_url: format!("http://{}/europepmc", addr),
            semanticscholar_api_url: format!("http://{}/s2", addr),
            allow_local_or_example_urls: true,
        };

        let mut batch = vec![
            json!({
                "title": "Scalable Diffusion Transformers for Molecular Dynamics",
                "link": "https://arxiv.org/abs/2611.54321v1",
                "summary": "Short preprint announcement.",
            }),
            json!({
                "title": "Driven-Dissipative Criticality in Trapped-Ion Spin Chains",
                "link": "https://journals.aps.org/prx/abstract/10.1103/PhysRevX.16.041099",
                "doi": "10.1103/PhysRevX.16.041099",
                "summary": "Driven quantum spin chains show novel phase transitions.",
            }),
        ];

        let enriched = enrich_entries_missing_abstracts(&mut batch, &endpoints).await;
        assert_eq!(enriched, 2);
        assert!(batch[0]["summary"]
            .as_str()
            .unwrap()
            .contains("1,200 globular proteins"));
        assert!(batch[1]["summary"]
            .as_str()
            .unwrap()
            .contains("Hohenberg-Halperin predictions"));
    }
}
