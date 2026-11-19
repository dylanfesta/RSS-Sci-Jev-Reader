//! Human-readable TOML import/export for SmartRSSReader feeds (with tags) and Jev filter rules.

use crate::database::{utc_now_iso, Database};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedFeed {
    pub name: String,
    pub url: String,
    pub category: String,
    pub tags: Vec<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedRule {
    pub name: String,
    pub query_type: String,
    pub weight: f64,
    pub enabled: bool,
    pub instructions: String,
    pub criteria: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub true_description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub false_description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub levels: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FeedConflict {
    pub key: String,
    pub existing: Value,
    pub incoming: NormalizedFeed,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuleConflict {
    pub key: String,
    pub existing: Value,
    pub incoming: NormalizedRule,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportPreview {
    pub total_feeds: usize,
    pub total_rules: usize,
    pub new_feeds: Vec<NormalizedFeed>,
    pub conflicting_feeds: Vec<FeedConflict>,
    pub new_rules: Vec<NormalizedRule>,
    pub conflicting_rules: Vec<RuleConflict>,
    pub has_conflicts: bool,
    pub conflict_count: usize,
}

#[derive(Debug, Clone, Default)]
pub struct ImportApplyOptions {
    pub override_all: bool,
    pub override_feed_urls: HashSet<String>,
    pub override_rule_names: HashSet<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportSummary {
    pub feeds_added: usize,
    pub feeds_overridden: usize,
    pub feeds_skipped: usize,
    pub rules_added: usize,
    pub rules_overridden: usize,
    pub rules_skipped: usize,
}

#[derive(Debug, Deserialize)]
struct RawTomlConfig {
    #[serde(default)]
    #[allow(dead_code)]
    version: Option<u32>,
    #[serde(default)]
    feeds: Vec<RawTomlFeed>,
    #[serde(default, alias = "queries", alias = "filters")]
    rules: Vec<RawTomlRule>,
}

#[derive(Debug, Deserialize)]
struct RawTomlFeed {
    #[serde(default, alias = "title")]
    name: String,
    url: String,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default = "default_true")]
    enabled: bool,
}

#[derive(Debug, Deserialize)]
struct RawTomlRule {
    name: String,
    #[serde(rename = "type", alias = "query_type")]
    query_type: String,
    #[serde(default = "default_weight")]
    weight: f64,
    #[serde(default = "default_true")]
    enabled: bool,
    instructions: String,
    #[serde(default, alias = "true_desc")]
    true_description: Option<String>,
    #[serde(default, alias = "false_desc")]
    false_description: Option<String>,
    #[serde(default)]
    levels: Option<Vec<String>>,
    #[serde(default)]
    criteria: Option<toml::Value>,
}

fn default_true() -> bool {
    true
}

fn default_weight() -> f64 {
    1.0
}

fn toml_str(s: &str) -> String {
    toml::Value::String(s.to_string()).to_string()
}

fn format_float(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{:.1}", v)
    } else {
        format!("{}", v)
    }
}

fn format_inline_string_array(items: &[String]) -> String {
    if items.is_empty() {
        return "[]".to_string();
    }
    let parts: Vec<String> = items.iter().map(|s| toml_str(s)).collect();
    format!("[{}]", parts.join(", "))
}

pub fn export_to_toml_string(db: &Database) -> Result<(String, usize, usize), String> {
    let feeds = db.list_feeds()?;
    let queries = db.list_queries(false)?;
    let now = utc_now_iso();

    let mut out = String::new();
    out.push_str("# SmartRSSReader Configuration — RSS Feeds & Jev Filter Rules\n");
    out.push_str(&format!("# Exported at: {}\n", now));
    out.push_str("version = 1\n\n");

    out.push_str("# ==============================================================================\n");
    out.push_str("# RSS / Atom Feeds\n");
    out.push_str("# ==============================================================================\n");

    if feeds.is_empty() {
        out.push_str("# (No RSS feeds configured)\n");
    } else {
        for f in &feeds {
            let name = f.get("title").and_then(|v| v.as_str()).unwrap_or("");
            let url = f.get("url").and_then(|v| v.as_str()).unwrap_or("");
            let category = f
                .get("category")
                .and_then(|v| v.as_str())
                .filter(|s| !s.trim().is_empty())
                .unwrap_or("General");
            let tags: Vec<String> = f
                .get("tags")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|x| x.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            let enabled = f.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);

            out.push('\n');
            out.push_str("[[feeds]]\n");
            out.push_str(&format!("name = {}\n", toml_str(name)));
            out.push_str(&format!("url = {}\n", toml_str(url)));
            out.push_str(&format!("category = {}\n", toml_str(category)));
            out.push_str(&format!("tags = {}\n", format_inline_string_array(&tags)));
            out.push_str(&format!("enabled = {}\n", enabled));
        }
    }

    out.push('\n');
    out.push_str("# ==============================================================================\n");
    out.push_str("# Jev Filter & Ranking Rules\n");
    out.push_str("# - type = \"noul\"  -> Yes/No probability in [0.0, 1.0]\n");
    out.push_str("# - type = \"score\" -> Ordered rubric levels from lowest [0] to highest [K-1]\n");
    out.push_str("# - Positive weight boosts matching papers; negative weight penalizes them.\n");
    out.push_str("# ==============================================================================\n");

    if queries.is_empty() {
        out.push_str("# (No filter rules configured)\n");
    } else {
        for q in &queries {
            let name = q.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let qtype = q.get("query_type").and_then(|v| v.as_str()).unwrap_or("noul");
            let weight = q.get("weight").and_then(|v| v.as_f64()).unwrap_or(1.0);
            let enabled = q.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
            let instructions = q.get("instructions").and_then(|v| v.as_str()).unwrap_or("");
            let criteria = q.get("criteria");

            out.push('\n');
            out.push_str("[[rules]]\n");
            out.push_str(&format!("name = {}\n", toml_str(name)));
            out.push_str(&format!("type = {}\n", toml_str(qtype)));
            out.push_str(&format!("weight = {}\n", format_float(weight)));
            out.push_str(&format!("enabled = {}\n", enabled));
            out.push_str(&format!("instructions = {}\n", toml_str(instructions)));

            if qtype == "score" {
                let levels: Vec<String> = criteria
                    .and_then(|c| c.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|x| x.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();
                out.push_str("levels = [\n");
                for lvl in &levels {
                    out.push_str(&format!("  {},\n", toml_str(lvl)));
                }
                out.push_str("]\n");
            } else if let Some(obj) = criteria.and_then(|c| c.as_object()) {
                if let Some(t) = obj.get("true").and_then(|v| v.as_str()).filter(|s| !s.trim().is_empty()) {
                    out.push_str(&format!("true_description = {}\n", toml_str(t)));
                }
                if let Some(f) = obj.get("false").and_then(|v| v.as_str()).filter(|s| !s.trim().is_empty()) {
                    out.push_str(&format!("false_description = {}\n", toml_str(f)));
                }
            }
        }
    }

    Ok((out, feeds.len(), queries.len()))
}

pub struct ParsedConfig {
    pub feeds: Vec<NormalizedFeed>,
    pub rules: Vec<NormalizedRule>,
}

pub fn parse_and_validate_toml(raw_toml: &str) -> Result<ParsedConfig, String> {
    let trimmed = raw_toml.trim();
    if trimmed.is_empty() {
        return Err("TOML content is empty.".to_string());
    }

    let raw: RawTomlConfig = toml::from_str(trimmed)
        .map_err(|e| format!("Invalid TOML syntax: {}", e))?;

    let mut feeds = Vec::with_capacity(raw.feeds.len());
    let mut seen_urls = HashSet::new();
    for (idx, rf) in raw.feeds.into_iter().enumerate() {
        let url = rf.url.trim().to_string();
        if url.len() < 4 {
            return Err(format!("Feed #{} has an empty or invalid 'url'.", idx + 1));
        }
        let url_lower = url.to_lowercase();
        if !seen_urls.insert(url_lower) {
            return Err(format!(
                "Duplicate feed URL in TOML file: '{}'. Please remove duplicate entries.",
                url
            ));
        }
        let name = if rf.name.trim().is_empty() {
            url.clone()
        } else {
            rf.name.trim().to_string()
        };
        let category = rf
            .category
            .as_deref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .unwrap_or("General")
            .to_string();
        let tags = Database::normalize_tags(&rf.tags);

        feeds.push(NormalizedFeed {
            name,
            url,
            category,
            tags,
            enabled: rf.enabled,
        });
    }

    let mut rules = Vec::with_capacity(raw.rules.len());
    let mut seen_rule_names = HashSet::new();
    for (idx, rr) in raw.rules.into_iter().enumerate() {
        let name = rr.name.trim().to_string();
        if name.is_empty() {
            return Err(format!("Filter rule #{} is missing a 'name'.", idx + 1));
        }
        let name_lower = name.to_lowercase();
        if !seen_rule_names.insert(name_lower) {
            return Err(format!(
                "Duplicate filter rule name in TOML file: '{}'. Please use unique rule names.",
                name
            ));
        }
        let qtype = rr.query_type.trim().to_lowercase();
        if qtype != "noul" && qtype != "score" {
            return Err(format!(
                "Filter rule '{}' has invalid type '{}'. Expected 'noul' or 'score'.",
                name, rr.query_type
            ));
        }
        if !rr.weight.is_finite() {
            return Err(format!("Filter rule '{}' has a non-finite weight.", name));
        }
        let instructions = rr.instructions.trim().to_string();
        if instructions.is_empty() {
            return Err(format!(
                "Filter rule '{}' is missing 'instructions'.",
                name
            ));
        }

        if qtype == "score" {
            let mut raw_levels = rr.levels.unwrap_or_default();
            if raw_levels.is_empty() {
                if let Some(toml::Value::Array(arr)) = rr.criteria {
                    raw_levels = arr
                        .into_iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect();
                }
            }
            let clean_levels: Vec<String> = raw_levels
                .into_iter()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            if clean_levels.len() < 2 {
                return Err(format!(
                    "Score filter rule '{}' requires at least 2 entries in 'levels'.",
                    name
                ));
            }
            let criteria = json!(clean_levels);
            rules.push(NormalizedRule {
                name,
                query_type: qtype,
                weight: rr.weight,
                enabled: rr.enabled,
                instructions,
                criteria,
                true_description: None,
                false_description: None,
                levels: Some(clean_levels),
            });
        } else {
            let mut true_desc = rr
                .true_description
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            let mut false_desc = rr
                .false_description
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());

            if let Some(toml::Value::Table(tbl)) = rr.criteria {
                if true_desc.is_none() {
                    true_desc = tbl
                        .get("true")
                        .and_then(|v| v.as_str())
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty());
                }
                if false_desc.is_none() {
                    false_desc = tbl
                        .get("false")
                        .and_then(|v| v.as_str())
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty());
                }
            }

            let mut crit_map = serde_json::Map::new();
            if let Some(ref t) = true_desc {
                crit_map.insert("true".to_string(), Value::String(t.clone()));
            }
            if let Some(ref f) = false_desc {
                crit_map.insert("false".to_string(), Value::String(f.clone()));
            }

            rules.push(NormalizedRule {
                name,
                query_type: qtype,
                weight: rr.weight,
                enabled: rr.enabled,
                instructions,
                criteria: Value::Object(crit_map),
                true_description: true_desc,
                false_description: false_desc,
                levels: None,
            });
        }
    }

    if feeds.is_empty() && rules.is_empty() {
        return Err("TOML file contains neither [[feeds]] nor [[rules]].".to_string());
    }

    Ok(ParsedConfig { feeds, rules })
}

pub fn preview_toml_import(db: &Database, raw_toml: &str) -> Result<ImportPreview, String> {
    let parsed = parse_and_validate_toml(raw_toml)?;
    let total_feeds = parsed.feeds.len();
    let total_rules = parsed.rules.len();

    let mut new_feeds = Vec::new();
    let mut conflicting_feeds = Vec::new();
    for feed in parsed.feeds {
        if let Some(existing) = db.get_feed_by_url(&feed.url)? {
            conflicting_feeds.push(FeedConflict {
                key: feed.url.trim().to_lowercase(),
                existing,
                incoming: feed,
            });
        } else {
            new_feeds.push(feed);
        }
    }

    let mut new_rules = Vec::new();
    let mut conflicting_rules = Vec::new();
    for rule in parsed.rules {
        if let Some(existing) = db.get_query_by_name(&rule.name)? {
            conflicting_rules.push(RuleConflict {
                key: rule.name.trim().to_lowercase(),
                existing,
                incoming: rule,
            });
        } else {
            new_rules.push(rule);
        }
    }

    let conflict_count = conflicting_feeds.len() + conflicting_rules.len();

    Ok(ImportPreview {
        total_feeds,
        total_rules,
        new_feeds,
        conflicting_feeds,
        new_rules,
        conflicting_rules,
        has_conflicts: conflict_count > 0,
        conflict_count,
    })
}

pub fn apply_toml_import(
    db: &Database,
    raw_toml: &str,
    opts: &ImportApplyOptions,
) -> Result<ImportSummary, String> {
    let parsed = parse_and_validate_toml(raw_toml)?;

    let mut summary = ImportSummary {
        feeds_added: 0,
        feeds_overridden: 0,
        feeds_skipped: 0,
        rules_added: 0,
        rules_overridden: 0,
        rules_skipped: 0,
    };

    for feed in parsed.feeds {
        let key = feed.url.trim().to_lowercase();
        if let Some(existing) = db.get_feed_by_url(&feed.url)? {
            if opts.override_all || opts.override_feed_urls.contains(&key) {
                let fid = existing["id"].as_i64().unwrap_or(0);
                db.update_feed(
                    fid,
                    Some(&feed.name),
                    Some(&feed.url),
                    Some(&feed.category),
                    Some(&feed.tags),
                    Some(feed.enabled),
                )?;
                summary.feeds_overridden += 1;
            } else {
                summary.feeds_skipped += 1;
            }
        } else {
            db.create_feed_with_tags(
                &feed.name,
                &feed.url,
                &feed.category,
                &feed.tags,
                feed.enabled,
            )?;
            summary.feeds_added += 1;
        }
    }

    for rule in parsed.rules {
        let key = rule.name.trim().to_lowercase();
        if let Some(existing) = db.get_query_by_name(&rule.name)? {
            if opts.override_all || opts.override_rule_names.contains(&key) {
                let qid = existing["id"].as_i64().unwrap_or(0);
                db.update_query(
                    qid,
                    Some(&rule.name),
                    Some(&rule.query_type),
                    Some(&rule.instructions),
                    Some(&rule.criteria),
                    Some(rule.weight),
                    Some(rule.enabled),
                )?;
                summary.rules_overridden += 1;
            } else {
                summary.rules_skipped += 1;
            }
        } else {
            db.create_query(
                &rule.name,
                &rule.query_type,
                &rule.instructions,
                &rule.criteria,
                rule.weight,
                rule.enabled,
            )?;
            summary.rules_added += 1;
        }
    }

    Ok(summary)
}
