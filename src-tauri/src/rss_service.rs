//! RSS/Atom/RDF feed fetching and normalization service for scientific journals.

use chrono::{DateTime, Utc};
use quick_xml::events::Event;
use quick_xml::Reader;
use regex::Regex;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::time::Duration;

pub const USER_AGENT: &str =
    "Mozilla/5.0 (X11; Fedora; Linux x86_64; rv:132.0) Gecko/20100101 Firefox/132.0 SmartRSSReader/0.1";

pub fn unescape_html_entities(input: &str) -> String {
    let mut s = input
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ");

    if s.contains("&#") {
        if let Ok(re_dec) = Regex::new(r"&#(\d+);") {
            s = re_dec
                .replace_all(&s, |caps: &regex::Captures| {
                    caps.get(1)
                        .and_then(|m| m.as_str().parse::<u32>().ok())
                        .and_then(char::from_u32)
                        .map(|c| c.to_string())
                        .unwrap_or_default()
                })
                .into_owned();
        }
        if let Ok(re_hex) = Regex::new(r"(?i)&#x([0-9a-f]+);") {
            s = re_hex
                .replace_all(&s, |caps: &regex::Captures| {
                    caps.get(1)
                        .and_then(|m| u32::from_str_radix(m.as_str(), 16).ok())
                        .and_then(char::from_u32)
                        .map(|c| c.to_string())
                        .unwrap_or_default()
                })
                .into_owned();
        }
    }
    s
}

pub fn clean_html_text(raw: &str) -> String {
    if raw.is_empty() {
        return String::new();
    }
    let block_re = Regex::new(r"(?i)</?(p|div|br|li|tr|h[1-6])\b[^>]*>").unwrap();
    let tag_re = Regex::new(r"<[^>]+>").unwrap();
    let ws_re = Regex::new(r"\s+").unwrap();

    let text = block_re.replace_all(raw, "\n");
    let text = tag_re.replace_all(&text, " ");
    let text = unescape_html_entities(&text);

    let lines: Vec<String> = text
        .lines()
        .map(|line| ws_re.replace_all(line, " ").trim().to_string())
        .filter(|line| !line.is_empty())
        .collect();

    lines.join("\n").trim().to_string()
}

pub fn format_published_date(raw: &str) -> String {
    let cleaned = clean_html_text(raw);
    if cleaned.is_empty() {
        return String::new();
    }
    if let Ok(dt) = DateTime::parse_from_rfc2822(&cleaned) {
        return dt
            .with_timezone(&Utc)
            .format("%Y-%m-%d %H:%M UTC")
            .to_string();
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(&cleaned.replace('Z', "+00:00")) {
        return dt
            .with_timezone(&Utc)
            .format("%Y-%m-%d %H:%M UTC")
            .to_string();
    }
    cleaned
}

pub fn compute_entry_uid(
    id: Option<&str>,
    prism_doi: Option<&str>,
    dc_identifier: Option<&str>,
    link: Option<&str>,
    title: &str,
    feed_url: &str,
) -> String {
    let raw_id = id
        .filter(|s| !s.trim().is_empty())
        .or_else(|| prism_doi.filter(|s| !s.trim().is_empty()))
        .or_else(|| dc_identifier.filter(|s| !s.trim().is_empty()))
        .or_else(|| link.filter(|s| !s.trim().is_empty()))
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| format!("{feed_url}::{title}"));

    let mut hasher = Sha256::new();
    hasher.update(raw_id.trim().as_bytes());
    let digest = hex::encode(hasher.finalize());
    digest.chars().take(32).collect()
}

#[derive(Default)]
struct RawEntryItem {
    id: String,
    prism_doi: String,
    dc_identifier: String,
    title: String,
    link: String,
    authors: Vec<String>,
    author_fallback: String,
    published: String,
    updated: String,
    summary: String,
    content: String,
}

fn local_tag_name(raw_name: &[u8]) -> String {
    let s = String::from_utf8_lossy(raw_name).to_lowercase();
    s.trim().to_string()
}

pub fn parse_feed_content(
    content: &[u8],
    feed_url: &str,
    feed_id: Option<i64>,
    feed_title_override: Option<&str>,
    max_entries: usize,
) -> Result<Value, String> {
    let raw_str = String::from_utf8_lossy(content);
    let trimmed = raw_str.trim();
    let lower_head: String = trimmed.chars().take(1024).collect::<String>().to_lowercase();

    if lower_head.contains("<html") || lower_head.contains("<!doctype html") {
        return Err(
            "URL returned an HTML webpage (e.g., sign-in or landing page) instead of an RSS/Atom/RDF feed."
                .to_string(),
        );
    }

    let mut reader = Reader::from_str(&raw_str);
    reader.config_mut().trim_text(false);

    let mut feed_version = String::new();
    let mut feed_title_raw = String::new();
    let mut feed_desc_raw = String::new();
    let mut raw_entries: Vec<RawEntryItem> = Vec::new();

    let mut tag_stack: Vec<String> = Vec::new();
    let mut current_item: Option<RawEntryItem> = None;
    let mut in_author_block = false;
    let mut current_text = String::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let tag = local_tag_name(e.name().as_ref());
                if tag == "rss" {
                    feed_version = "rss20".to_string();
                } else if tag == "feed" {
                    feed_version = "atom10".to_string();
                } else if tag == "rdf:rdf" || tag == "rdf" {
                    feed_version = "rss10".to_string();
                }

                if tag == "item" || tag == "entry" {
                    current_item = Some(RawEntryItem::default());
                } else if (tag == "author" || tag == "dc:creator") && current_item.is_some() {
                    in_author_block = true;
                } else if tag == "link" {
                    // Check Atom <link href="..."/>
                    let mut href_val = String::new();
                    let mut rel_val = String::new();
                    for attr in e.attributes().flatten() {
                        let k = String::from_utf8_lossy(attr.key.as_ref()).to_lowercase();
                        let v = String::from_utf8_lossy(&attr.value).into_owned();
                        if k == "href" {
                            href_val = v;
                        } else if k == "rel" {
                            rel_val = v;
                        }
                    }
                    if let Some(ref mut item) = current_item {
                        if !href_val.is_empty()
                            && (item.link.is_empty() || rel_val == "alternate" || rel_val.is_empty())
                        {
                            item.link = href_val;
                        }
                    }
                }

                tag_stack.push(tag);
                current_text.clear();
            }
            Ok(Event::Empty(e)) => {
                let tag = local_tag_name(e.name().as_ref());
                if tag == "link" {
                    let mut href_val = String::new();
                    let mut rel_val = String::new();
                    for attr in e.attributes().flatten() {
                        let k = String::from_utf8_lossy(attr.key.as_ref()).to_lowercase();
                        let v = String::from_utf8_lossy(&attr.value).into_owned();
                        if k == "href" {
                            href_val = v;
                        } else if k == "rel" {
                            rel_val = v;
                        }
                    }
                    if let Some(ref mut item) = current_item {
                        if !href_val.is_empty()
                            && (item.link.is_empty() || rel_val == "alternate" || rel_val.is_empty())
                        {
                            item.link = href_val;
                        }
                    }
                }
            }
            Ok(Event::Text(e)) => {
                let txt = e.unescape().unwrap_or_else(|_| {
                    std::borrow::Cow::Owned(String::from_utf8_lossy(e.as_ref()).into_owned())
                });
                current_text.push_str(&txt);
            }
            Ok(Event::CData(e)) => {
                let txt = String::from_utf8_lossy(e.as_ref());
                current_text.push_str(&txt);
            }
            Ok(Event::End(e)) => {
                let tag = local_tag_name(e.name().as_ref());
                let text_val = current_text.trim().to_string();

                if tag == "item" || tag == "entry" {
                    if let Some(finished) = current_item.take() {
                        raw_entries.push(finished);
                    }
                } else if let Some(ref mut item) = current_item {
                    match tag.as_str() {
                        "guid" | "id" => {
                            if item.id.is_empty() && !text_val.is_empty() {
                                item.id = text_val;
                            }
                        }
                        "prism:doi" | "prism_doi" => {
                            if item.prism_doi.is_empty() && !text_val.is_empty() {
                                item.prism_doi = text_val;
                            }
                        }
                        "dc:identifier" | "dc_identifier" => {
                            if item.dc_identifier.is_empty() && !text_val.is_empty() {
                                item.dc_identifier = text_val;
                            }
                        }
                        "title" => {
                            if item.title.is_empty() && !text_val.is_empty() {
                                item.title = text_val;
                            }
                        }
                        "link" => {
                            if item.link.is_empty() && !text_val.is_empty() {
                                item.link = text_val;
                            }
                        }
                        "name" if in_author_block => {
                            if !text_val.is_empty() {
                                item.authors.push(clean_html_text(&text_val));
                            }
                        }
                        "author" | "dc:creator" | "dc_creator" => {
                            in_author_block = false;
                            if !text_val.is_empty() && item.authors.is_empty() {
                                if item.author_fallback.is_empty() {
                                    item.author_fallback = clean_html_text(&text_val);
                                } else {
                                    item.authors.push(item.author_fallback.clone());
                                    item.authors.push(clean_html_text(&text_val));
                                }
                            }
                        }
                        "pubdate" | "published" | "dc:date" | "dc_date" => {
                            if item.published.is_empty() && !text_val.is_empty() {
                                item.published = text_val;
                            }
                        }
                        "updated" => {
                            if item.updated.is_empty() && !text_val.is_empty() {
                                item.updated = text_val;
                            }
                        }
                        "description" | "summary" => {
                            if item.summary.is_empty() && !text_val.is_empty() {
                                item.summary = text_val;
                            }
                        }
                        "content" | "content:encoded" => {
                            if item.content.is_empty() && !text_val.is_empty() {
                                item.content = text_val;
                            }
                        }
                        _ => {}
                    }
                } else {
                    // Feed-level metadata (not inside an item/entry)
                    match tag.as_str() {
                        "title" => {
                            if feed_title_raw.is_empty() && !text_val.is_empty() {
                                feed_title_raw = text_val;
                            }
                        }
                        "description" | "subtitle" => {
                            if feed_desc_raw.is_empty() && !text_val.is_empty() {
                                feed_desc_raw = text_val;
                            }
                        }
                        _ => {}
                    }
                }

                tag_stack.pop();
                current_text.clear();
            }
            Ok(Event::Eof) => break,
            Err(err) => {
                if raw_entries.is_empty() {
                    return Err(format!("Failed to parse feed XML: {err}"));
                }
                break;
            }
            _ => {}
        }
    }

    if feed_version.is_empty() && raw_entries.is_empty() {
        return Err("Response is not a recognized RSS, Atom, or RDF feed.".to_string());
    }

    let cleaned_feed_title = clean_html_text(&feed_title_raw);
    let detected_title = if !cleaned_feed_title.is_empty() {
        cleaned_feed_title
    } else if let Some(ov) = feed_title_override.filter(|s| !s.trim().is_empty()) {
        ov.trim().to_string()
    } else {
        feed_url.to_string()
    };

    let journal_title = feed_title_override
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| detected_title.clone());

    let limit = max_entries.max(1);
    let mut entries_out: Vec<Value> = Vec::new();

    for item in raw_entries.into_iter().take(limit) {
        let title_clean = clean_html_text(&item.title);
        let title = if title_clean.is_empty() {
            "Untitled Entry".to_string()
        } else {
            title_clean
        };
        let link = item.link.trim().to_string();

        let authors = if !item.authors.is_empty() {
            let names: Vec<String> = item
                .authors
                .into_iter()
                .filter(|n| !n.trim().is_empty())
                .collect();
            if names.len() > 8 {
                format!("{} et al.", names[..8].join(", "))
            } else {
                names.join(", ")
            }
        } else {
            item.author_fallback
        };

        let summary_source = if !item.content.is_empty() {
            &item.content
        } else {
            &item.summary
        };
        let summary = clean_html_text(summary_source);

        let pub_raw = if !item.published.is_empty() {
            &item.published
        } else {
            &item.updated
        };
        let published_at = format_published_date(pub_raw);

        let uid = compute_entry_uid(
            Some(&item.id),
            Some(&item.prism_doi),
            Some(&item.dc_identifier),
            Some(&link),
            &title,
            feed_url,
        );

        entries_out.push(json!({
            "entry_uid": uid,
            "feed_id": feed_id,
            "journal_title": journal_title,
            "title": title,
            "link": link,
            "authors": authors,
            "published_at": published_at,
            "summary": summary,
        }));
    }

    Ok(json!({
        "feed_title": detected_title,
        "feed_url": feed_url,
        "feed_version": feed_version,
        "feed_description": clean_html_text(&feed_desc_raw),
        "entry_count": entries_out.len(),
        "entries": entries_out,
    }))
}

pub async fn fetch_and_parse_feed(
    url: &str,
    feed_id: Option<i64>,
    feed_title_override: Option<&str>,
    max_entries: usize,
) -> Result<Value, String> {
    let clean_url = url.trim();
    if let Some(local_path) = clean_url.strip_prefix("file://") {
        let bytes = std::fs::read(local_path)
            .map_err(|e| format!("Could not read local RSS file {}: {}", local_path, e))?;
        return parse_feed_content(&bytes, clean_url, feed_id, feed_title_override, max_entries);
    }

    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .get(clean_url)
        .header(
            "Accept",
            "application/rss+xml, application/atom+xml, application/rdf+xml, application/xml, text/xml, */*;q=0.8",
        )
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        return Err(format!("HTTP status {} for URL {}", resp.status(), clean_url));
    }

    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
    parse_feed_content(&bytes, clean_url, feed_id, feed_title_override, max_entries)
}
