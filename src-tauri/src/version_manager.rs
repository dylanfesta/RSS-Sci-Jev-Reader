//! Versioned data-folder backup & resilient TOML fallback recovery manager.
//!
//! Keeps `~/SmartRSSReader` (or the user's chosen Data & AI Skills Folder) as the stable path,
//! while automatically snapshotting `<data_dir>/backups/v<previous_version>/` (`smart_rss.db`,
//! `secrets.enc`, and human-readable `smart_rss_config.toml`) BEFORE upgrading in-place whenever
//! the application version changes.
//!
//! Also maintains a rolling `<data_dir>/backups/latest_feeds_and_rules.toml` safety snapshot
//! whenever feeds or Jev filter rules are modified, and automatically recovers all feeds, tags,
//! rules, and settings from TOML if a future database schema change or file corruption occurs.

use crate::config_toml::serialize_feeds_and_queries_to_toml;
use crate::database::{utc_now_iso, Database};
use crate::sqlite_conn::SqliteConn;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const SCHEMA_VERSION: i64 = 2;
pub const LEGACY_UNVERSIONED_FALLBACK: &str = "0.1.0";
pub const ROLLING_TOML_FILENAME: &str = "latest_feeds_and_rules.toml";

const PRESERVED_SETTING_KEYS: &[&str] = &[
    "ai_provider",
    "provider_api_url",
    "provider_wire_format",
    "jev_model",
    "custom_provider_name",
    "custom_base_url",
    "custom_portal_url",
    "max_entries_per_feed",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradeReport {
    pub app_version: String,
    pub previous_version: Option<String>,
    pub upgraded: bool,
    pub recovered_from_toml: bool,
    pub backup_dir: Option<String>,
    pub toml_backup_path: Option<String>,
    pub feeds_preserved: usize,
    pub rules_preserved: usize,
    pub message: Option<String>,
}

impl UpgradeReport {
    pub fn fresh(app_version: &str) -> Self {
        Self {
            app_version: app_version.to_string(),
            previous_version: None,
            upgraded: false,
            recovered_from_toml: false,
            backup_dir: None,
            toml_backup_path: None,
            feeds_preserved: 0,
            rules_preserved: 0,
            message: None,
        }
    }
}

pub enum ExistingDbState {
    MissingOrEmpty,
    Readable {
        detected_version: String,
        had_explicit_version: bool,
        preserved_settings: HashMap<String, String>,
        toml_snapshot: Option<String>,
        feeds_count: usize,
        rules_count: usize,
    },
    CorruptOrUnreadable {
        error: String,
    },
}

pub fn sanitize_version_dir_name(raw_version: &str) -> String {
    let trimmed = raw_version.trim();
    let without_v = trimmed
        .strip_prefix('v')
        .or_else(|| trimmed.strip_prefix('V'))
        .unwrap_or(trimmed);
    let clean: String = without_v
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if clean.is_empty() {
        "v0.1.0".to_string()
    } else {
        format!("v{clean}")
    }
}

fn row_str(row: &HashMap<String, Value>, key: &str) -> Option<String> {
    row.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn row_bool_default(row: &HashMap<String, Value>, key: &str, default: bool) -> bool {
    match row.get(key) {
        Some(v) => {
            if let Some(b) = v.as_bool() {
                b
            } else if let Some(n) = v.as_i64() {
                n != 0
            } else if let Some(s) = v.as_str() {
                !matches!(s.trim().to_ascii_lowercase().as_str(), "0" | "false" | "no")
            } else {
                default
            }
        }
        None => default,
    }
}

fn row_f64_default(row: &HashMap<String, Value>, key: &str, default: f64) -> f64 {
    row.get(key)
        .and_then(|v| {
            v.as_f64()
                .or_else(|| v.as_i64().map(|i| i as f64))
                .or_else(|| v.as_str().and_then(|s| s.parse::<f64>().ok()))
        })
        .unwrap_or(default)
}

/// Inspects an existing `smart_rss.db` BEFORE running `init_schema()` or migrations,
/// extracting its stored `app_version`, user settings, and a tolerant TOML snapshot of
/// all feeds and filter rules even if the schema is from an older alpha build.
pub fn inspect_existing_db(db_path: &Path) -> ExistingDbState {
    let Ok(meta) = std::fs::metadata(db_path) else {
        return ExistingDbState::MissingOrEmpty;
    };
    if meta.len() == 0 {
        return ExistingDbState::MissingOrEmpty;
    }

    let conn = match SqliteConn::open_existing_no_pragma(db_path) {
        Ok(c) => c,
        Err(err) => return ExistingDbState::CorruptOrUnreadable { error: err },
    };

    let master_rows = match conn.query_rows(
        "SELECT name FROM sqlite_master WHERE type = 'table'",
        &[],
    ) {
        Ok(r) => r,
        Err(err) => return ExistingDbState::CorruptOrUnreadable { error: err },
    };

    let table_names: Vec<String> = master_rows
        .iter()
        .filter_map(|r| row_str(r, "name"))
        .collect();

    let has_user_tables = table_names
        .iter()
        .any(|t| matches!(t.as_str(), "settings" | "feeds" | "queries" | "entries"));
    if !has_user_tables {
        return ExistingDbState::MissingOrEmpty;
    }

    let mut preserved_settings = HashMap::new();
    let mut explicit_version: Option<String> = None;

    if table_names.iter().any(|t| t == "settings") {
        if let Ok(srows) = conn.query_rows("SELECT key, value FROM settings", &[]) {
            for r in srows {
                let Some(k) = row_str(&r, "key") else {
                    continue;
                };
                let v = row_str(&r, "value").unwrap_or_default();
                if k == "app_version" && !v.trim().is_empty() {
                    explicit_version = Some(v.trim().to_string());
                } else if PRESERVED_SETTING_KEYS.contains(&k.as_str()) && !v.trim().is_empty() {
                    preserved_settings.insert(k, v);
                }
            }
        }
    }

    let mut feeds_vec: Vec<Value> = Vec::new();
    if table_names.iter().any(|t| t == "feeds") {
        if let Ok(frows) = conn.query_rows("SELECT * FROM feeds ORDER BY id ASC", &[]) {
            for r in frows {
                let url = row_str(&r, "url")
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default();
                if url.is_empty() {
                    continue;
                }
                let title = row_str(&r, "title")
                    .or_else(|| row_str(&r, "name"))
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or_else(|| url.clone());
                let category = row_str(&r, "category")
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or_else(|| "General".to_string());
                let tags: Vec<String> = row_str(&r, "tags_json")
                    .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
                    .and_then(|v| Database::parse_tags_from_value(&v))
                    .unwrap_or_default();
                let enabled = row_bool_default(&r, "enabled", true);

                feeds_vec.push(json!({
                    "title": title,
                    "url": url,
                    "category": category,
                    "tags": tags,
                    "enabled": enabled,
                }));
            }
        }
    }

    let mut queries_vec: Vec<Value> = Vec::new();
    if table_names.iter().any(|t| t == "queries") {
        if let Ok(qrows) = conn.query_rows("SELECT * FROM queries ORDER BY id ASC", &[]) {
            for r in qrows {
                let name = row_str(&r, "name")
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default();
                let instructions = row_str(&r, "instructions")
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default();
                if name.is_empty() || instructions.is_empty() {
                    continue;
                }
                let raw_qtype = row_str(&r, "query_type").unwrap_or_else(|| "noul".to_string());
                let qtype = if raw_qtype.trim() == "score" {
                    "score"
                } else {
                    "noul"
                };
                let criteria: Value = row_str(&r, "criteria_json")
                    .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
                    .unwrap_or_else(|| {
                        if qtype == "score" {
                            json!(["Low", "Medium", "High"])
                        } else {
                            json!({})
                        }
                    });
                let weight = row_f64_default(&r, "weight", 1.0);
                let enabled = row_bool_default(&r, "enabled", true);

                queries_vec.push(json!({
                    "name": name,
                    "query_type": qtype,
                    "instructions": instructions,
                    "criteria": criteria,
                    "weight": weight,
                    "enabled": enabled,
                }));
            }
        }
    }

    let feeds_count = feeds_vec.len();
    let rules_count = queries_vec.len();
    let toml_snapshot = if feeds_count > 0 || rules_count > 0 {
        let (toml_text, _, _) = serialize_feeds_and_queries_to_toml(&feeds_vec, &queries_vec);
        Some(toml_text)
    } else {
        None
    };

    let had_explicit_version = explicit_version.is_some();
    let detected_version =
        explicit_version.unwrap_or_else(|| LEGACY_UNVERSIONED_FALLBACK.to_string());

    ExistingDbState::Readable {
        detected_version,
        had_explicit_version,
        preserved_settings,
        toml_snapshot,
        feeds_count,
        rules_count,
    }
}

fn copy_db_snapshot_safely(src_db: &Path, dst_db: &Path) -> Result<(), String> {
    if let Ok(conn) = SqliteConn::open_existing_no_pragma(src_db) {
        let escaped_dst = dst_db.to_string_lossy().replace('\'', "''");
        let vacuum_sql = format!("VACUUM INTO '{escaped_dst}';");
        if conn.execute_batch(&vacuum_sql).is_ok() && dst_db.exists() {
            return Ok(());
        }
    }

    let _ = std::fs::remove_file(dst_db);
    std::fs::copy(src_db, dst_db)
        .map_err(|e| format!("Failed to copy SQLite DB to backup: {}", e))?;

    for suffix in ["-wal", "-shm"] {
        let src_side = PathBuf::from(format!("{}{}", src_db.display(), suffix));
        if src_side.exists() {
            let dst_side = PathBuf::from(format!("{}{}", dst_db.display(), suffix));
            let _ = std::fs::copy(&src_side, &dst_side);
        }
    }
    Ok(())
}

pub fn create_versioned_backup(
    data_dir: &Path,
    db_path: &Path,
    prev_version: &str,
    target_version: &str,
    toml_snapshot: Option<&str>,
    feeds_count: usize,
    rules_count: usize,
    preserved_settings: &HashMap<String, String>,
    reason: &str,
) -> Result<(PathBuf, Option<PathBuf>), String> {
    let backups_root = data_dir.join("backups");
    std::fs::create_dir_all(&backups_root)
        .map_err(|e| format!("Failed to create backups directory: {}", e))?;

    let base_folder = sanitize_version_dir_name(prev_version);
    let mut backup_dir = backups_root.join(&base_folder);
    if backup_dir.exists() {
        let stamp = Utc::now().format("%Y%m%d_%H%M%S").to_string();
        backup_dir = backups_root.join(format!("{}_{}", base_folder, stamp));
    }
    std::fs::create_dir_all(&backup_dir)
        .map_err(|e| format!("Failed to create version backup folder '{}': {}", backup_dir.display(), e))?;

    if db_path.exists() {
        let dst_db = backup_dir.join("smart_rss.db");
        let _ = copy_db_snapshot_safely(db_path, &dst_db);
    }

    let src_secrets = data_dir.join("secrets.enc");
    let had_secrets = if src_secrets.exists() {
        let _ = std::fs::copy(&src_secrets, backup_dir.join("secrets.enc"));
        true
    } else {
        false
    };

    let rolling_path = backups_root.join(ROLLING_TOML_FILENAME);
    let effective_toml: Option<String> = match toml_snapshot {
        Some(t) if !t.trim().is_empty() => Some(t.to_string()),
        _ => std::fs::read_to_string(&rolling_path)
            .ok()
            .filter(|s| !s.trim().is_empty()),
    };

    let mut written_toml_path: Option<PathBuf> = None;
    if let Some(ref t) = effective_toml {
        let p = backup_dir.join("smart_rss_config.toml");
        if std::fs::write(&p, t).is_ok() {
            written_toml_path = Some(p);
        }
        let _ = std::fs::write(&rolling_path, t);
    }

    let manifest = json!({
        "previous_version": prev_version,
        "upgraded_to_version": target_version,
        "reason": reason,
        "created_at": utc_now_iso(),
        "feeds_count": feeds_count,
        "rules_count": rules_count,
        "has_secrets_backup": had_secrets,
        "has_toml_backup": written_toml_path.is_some(),
        "preserved_settings": preserved_settings,
    });
    let _ = std::fs::write(
        backup_dir.join("backup_manifest.json"),
        serde_json::to_string_pretty(&manifest).unwrap_or_default(),
    );

    Ok((backup_dir, written_toml_path))
}

pub fn find_best_recovery_toml(data_dir: &Path, inline_toml: Option<&str>) -> Option<String> {
    if let Some(t) = inline_toml {
        if !t.trim().is_empty() {
            return Some(t.to_string());
        }
    }

    let backups_root = data_dir.join("backups");
    let rolling = backups_root.join(ROLLING_TOML_FILENAME);
    if let Ok(content) = std::fs::read_to_string(&rolling) {
        if !content.trim().is_empty() {
            return Some(content);
        }
    }

    let Ok(entries) = std::fs::read_dir(&backups_root) else {
        return None;
    };
    let mut subdirs: Vec<(std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            if !p.is_dir() {
                return None;
            }
            let toml_file = p.join("smart_rss_config.toml");
            if !toml_file.exists() {
                return None;
            }
            let mtime = std::fs::metadata(&toml_file)
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            Some((mtime, toml_file))
        })
        .collect();

    subdirs.sort_by(|a, b| b.0.cmp(&a.0));
    for (_, toml_file) in subdirs {
        if let Ok(content) = std::fs::read_to_string(&toml_file) {
            if !content.trim().is_empty() {
                return Some(content);
            }
        }
    }

    None
}

pub fn write_rolling_toml_snapshot(db: &Database) {
    let db_path = db.get_db_path();
    let Some(data_dir) = db_path.parent() else {
        return;
    };
    let Ok((toml_text, feeds_count, rules_count)) = crate::config_toml::export_to_toml_string(db)
    else {
        return;
    };
    if feeds_count == 0 && rules_count == 0 {
        return;
    }
    let backups_root = data_dir.join("backups");
    if std::fs::create_dir_all(&backups_root).is_ok() {
        let _ = std::fs::write(backups_root.join(ROLLING_TOML_FILENAME), toml_text);
    }
}

pub fn list_version_backups(data_dir: &Path) -> Vec<Value> {
    let backups_root = data_dir.join("backups");
    let Ok(entries) = std::fs::read_dir(&backups_root) else {
        return Vec::new();
    };

    let mut items: Vec<(String, Value)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let folder_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let manifest_path = path.join("backup_manifest.json");
        let manifest_val: Value = std::fs::read_to_string(&manifest_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(|| json!({}));

        let created_at = manifest_val
            .get("created_at")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        items.push((
            created_at.clone(),
            json!({
                "folder_name": folder_name,
                "path": path.to_string_lossy(),
                "previous_version": manifest_val.get("previous_version").cloned().unwrap_or(json!(folder_name)),
                "upgraded_to_version": manifest_val.get("upgraded_to_version").cloned().unwrap_or(Value::Null),
                "created_at": created_at,
                "feeds_count": manifest_val.get("feeds_count").cloned().unwrap_or(json!(0)),
                "rules_count": manifest_val.get("rules_count").cloned().unwrap_or(json!(0)),
                "has_db_backup": path.join("smart_rss.db").exists(),
                "has_secrets_backup": path.join("secrets.enc").exists(),
                "toml_backup_path": if path.join("smart_rss_config.toml").exists() {
                    Value::String(path.join("smart_rss_config.toml").to_string_lossy().to_string())
                } else {
                    Value::Null
                },
            }),
        ));
    }

    items.sort_by(|a, b| b.0.cmp(&a.0));
    items.into_iter().map(|(_, v)| v).collect()
}
