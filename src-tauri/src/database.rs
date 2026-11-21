//! SQLite persistence layer for SmartRSSReader with memory of active and archived entries.

use crate::config_toml::{apply_toml_import, ImportApplyOptions};
use crate::sqlite_conn::{SqlParam, SqliteConn};
use crate::version_manager::{
    create_versioned_backup, find_best_recovery_toml, inspect_existing_db,
    write_rolling_toml_snapshot, ExistingDbState, UpgradeReport, APP_VERSION, SCHEMA_VERSION,
};
use chrono::{DateTime, Duration, Utc};
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

pub const DEFAULT_ARCHIVE_RETENTION_DAYS: i64 = 30;

pub fn utc_now_iso() -> String {
    Utc::now().format("%Y-%m-%dT%H:%M:%S+00:00").to_string()
}

fn val_i64(row: &HashMap<String, Value>, key: &str) -> i64 {
    row.get(key).and_then(|v| v.as_i64()).unwrap_or(0)
}

fn val_f64(row: &HashMap<String, Value>, key: &str, default: f64) -> f64 {
    row.get(key)
        .and_then(|v| v.as_f64().or_else(|| v.as_i64().map(|i| i as f64)))
        .unwrap_or(default)
}

fn val_str(row: &HashMap<String, Value>, key: &str) -> String {
    row.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

fn val_opt_str(row: &HashMap<String, Value>, key: &str) -> Option<String> {
    row.get(key).and_then(|v| {
        if v.is_null() {
            None
        } else {
            v.as_str().map(|s| s.to_string())
        }
    })
}

fn round4(v: f64) -> f64 {
    (v * 10000.0).round() / 10000.0
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

#[derive(Clone)]
pub struct Database {
    db_path: Arc<RwLock<PathBuf>>,
    last_upgrade: Arc<RwLock<UpgradeReport>>,
}

impl Database {
    pub fn new(db_path: impl AsRef<Path>, seed_defaults: bool) -> Result<Self, String> {
        Self::new_with_version(db_path, APP_VERSION, seed_defaults)
    }

    pub fn new_with_version(
        db_path: impl AsRef<Path>,
        app_version: &str,
        seed_defaults: bool,
    ) -> Result<Self, String> {
        let path_buf = db_path.as_ref().to_path_buf();
        if let Some(parent) = path_buf.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let db = Self {
            db_path: Arc::new(RwLock::new(path_buf)),
            last_upgrade: Arc::new(RwLock::new(UpgradeReport::fresh(app_version))),
        };
        db.open_and_upgrade_or_recover(app_version, seed_defaults)?;
        Ok(db)
    }

    pub fn set_db_path(&self, new_path: impl AsRef<Path>, seed_defaults: bool) -> Result<(), String> {
        self.set_db_path_with_version(new_path, APP_VERSION, seed_defaults)
    }

    pub fn set_db_path_with_version(
        &self,
        new_path: impl AsRef<Path>,
        app_version: &str,
        seed_defaults: bool,
    ) -> Result<(), String> {
        let path_buf = new_path.as_ref().to_path_buf();
        if let Some(parent) = path_buf.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(mut g) = self.db_path.write() {
            *g = path_buf;
        }
        self.open_and_upgrade_or_recover(app_version, seed_defaults)
    }

    pub fn get_last_upgrade_report(&self) -> UpgradeReport {
        self.last_upgrade
            .read()
            .map(|r| r.clone())
            .unwrap_or_else(|_| UpgradeReport::fresh(APP_VERSION))
    }

    fn open_and_upgrade_or_recover(
        &self,
        app_version: &str,
        seed_defaults: bool,
    ) -> Result<(), String> {
        let path_buf = self.get_db_path();
        let data_dir = path_buf
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));

        let mut report = UpgradeReport::fresh(app_version);
        let mut inline_toml: Option<String> = None;
        let mut preserved_settings: HashMap<String, String> = HashMap::new();
        let mut pre_feeds_count = 0usize;
        let mut pre_rules_count = 0usize;
        let mut needs_initial_rolling_toml = false;
        let mut backup_created_already = false;

        match inspect_existing_db(&path_buf) {
            ExistingDbState::MissingOrEmpty => {}
            ExistingDbState::Readable {
                detected_version,
                had_explicit_version,
                preserved_settings: ps,
                toml_snapshot,
                feeds_count,
                rules_count,
            } => {
                inline_toml = toml_snapshot;
                preserved_settings = ps;
                pre_feeds_count = feeds_count;
                pre_rules_count = rules_count;

                if detected_version != app_version {
                    if let Ok((bdir, toml_p)) = create_versioned_backup(
                        &data_dir,
                        &path_buf,
                        &detected_version,
                        app_version,
                        inline_toml.as_deref(),
                        feeds_count,
                        rules_count,
                        &preserved_settings,
                        "version_upgrade",
                    ) {
                        backup_created_already = true;
                        report.upgraded = true;
                        report.previous_version = Some(detected_version.clone());
                        report.backup_dir = Some(bdir.to_string_lossy().to_string());
                        report.toml_backup_path =
                            toml_p.map(|p| p.to_string_lossy().to_string());
                        report.feeds_preserved = feeds_count;
                        report.rules_preserved = rules_count;
                        report.message = Some(format!(
                            "Upgraded data folder from v{} to v{} (backup saved in {}).",
                            detected_version.trim_start_matches('v'),
                            app_version.trim_start_matches('v'),
                            bdir.display()
                        ));
                    }
                } else if !had_explicit_version && (feeds_count > 0 || rules_count > 0) {
                    needs_initial_rolling_toml = true;
                }
            }
            ExistingDbState::CorruptOrUnreadable { .. } => {
                if let Ok((bdir, toml_p)) = create_versioned_backup(
                    &data_dir,
                    &path_buf,
                    "corrupt",
                    app_version,
                    None,
                    0,
                    0,
                    &preserved_settings,
                    "corrupt_db_backup",
                ) {
                    backup_created_already = true;
                    report.backup_dir = Some(bdir.to_string_lossy().to_string());
                    report.toml_backup_path = toml_p.map(|p| p.to_string_lossy().to_string());
                }
            }
        }

        match self.init_schema_and_verify() {
            Ok(()) => {
                let _ = self.prune_old_archived_entries(DEFAULT_ARCHIVE_RETENTION_DAYS, None);
                if seed_defaults {
                    self.seed_if_empty()?;
                }
            }
            Err(_schema_err) => {
                // Automatic non-destructive TOML fallback recovery!
                if !backup_created_already && path_buf.exists() {
                    let prev_lbl = report
                        .previous_version
                        .clone()
                        .unwrap_or_else(|| "schema_incompatible".to_string());
                    if let Ok((bdir, toml_p)) = create_versioned_backup(
                        &data_dir,
                        &path_buf,
                        &prev_lbl,
                        app_version,
                        inline_toml.as_deref(),
                        pre_feeds_count,
                        pre_rules_count,
                        &preserved_settings,
                        "schema_incompatible_recovery",
                    ) {
                        report.backup_dir = Some(bdir.to_string_lossy().to_string());
                        report.toml_backup_path =
                            toml_p.map(|p| p.to_string_lossy().to_string());
                    }
                }

                // Remove broken/incompatible DB files and initialize a clean DB
                let _ = std::fs::remove_file(&path_buf);
                for suffix in ["-wal", "-shm"] {
                    let side = PathBuf::from(format!("{}{}", path_buf.display(), suffix));
                    let _ = std::fs::remove_file(side);
                }

                self.init_schema()?;

                let recovery_toml = find_best_recovery_toml(&data_dir, inline_toml.as_deref());
                if let Some(toml_str) = recovery_toml {
                    let opts = ImportApplyOptions {
                        override_all: true,
                        ..Default::default()
                    };
                    if let Ok(summary) = apply_toml_import(self, &toml_str, &opts) {
                        report.feeds_preserved = summary.feeds_added + summary.feeds_overridden;
                        report.rules_preserved = summary.rules_added + summary.rules_overridden;
                    }
                    if !preserved_settings.is_empty() {
                        let _ = self.update_settings(&preserved_settings);
                    }
                } else if seed_defaults {
                    self.seed_if_empty()?;
                }

                report.upgraded = true;
                report.recovered_from_toml = true;
                let bdir_disp = report
                    .backup_dir
                    .as_deref()
                    .unwrap_or("backups/");
                report.message = Some(format!(
                    "Database schema changed or needed recovery — automatically restored {} feed(s) and {} filter rule(s) from TOML backup (previous DB preserved in {}).",
                    report.feeds_preserved, report.rules_preserved, bdir_disp
                ));
            }
        }

        self.stamp_version_metadata(app_version)?;
        if report.upgraded
            || report.recovered_from_toml
            || needs_initial_rolling_toml
            || seed_defaults
        {
            write_rolling_toml_snapshot(self);
        }

        if let Ok(mut g) = self.last_upgrade.write() {
            *g = report;
        }
        Ok(())
    }

    fn stamp_version_metadata(&self, app_version: &str) -> Result<(), String> {
        let conn = self.connect()?;
        conn.execute(
            "INSERT INTO settings (key, value) VALUES ('app_version', ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            &[app_version.into()],
        )?;
        conn.execute(
            "INSERT INTO settings (key, value) VALUES ('schema_version', ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            &[SCHEMA_VERSION.to_string().into()],
        )?;
        Ok(())
    }

    fn init_schema_and_verify(&self) -> Result<(), String> {
        self.init_schema()?;
        let conn = self.connect()?;
        let qc = conn.query_rows("PRAGMA quick_check(1)", &[])?;
        if let Some(first) = qc.first() {
            let status = first
                .values()
                .next()
                .and_then(|v| v.as_str())
                .unwrap_or("ok");
            if !status.eq_ignore_ascii_case("ok") {
                return Err(format!("SQLite integrity check failed: {status}"));
            }
        }
        // Verify all expected columns on core tables are selectable
        conn.query_rows(
            "SELECT id, title, url, category, tags_json, enabled, last_fetched_at, last_error, created_at FROM feeds LIMIT 1",
            &[],
        )?;
        conn.query_rows(
            "SELECT id, name, query_type, instructions, criteria_json, weight, enabled, created_at FROM queries LIMIT 1",
            &[],
        )?;
        conn.query_rows(
            "SELECT id, entry_uid, feed_id, journal_title, title, link, authors, published_at, summary, status, archived_at, evaluated_at, created_at FROM entries LIMIT 1",
            &[],
        )?;
        conn.query_rows(
            "SELECT entry_id, query_id, normalized_score, query_snapshot_json, raw_response_json, evaluated_at FROM entry_scores LIMIT 1",
            &[],
        )?;
        Ok(())
    }

    pub fn get_db_path(&self) -> PathBuf {
        self.db_path.read().unwrap().clone()
    }

    pub fn connect(&self) -> Result<SqliteConn, String> {
        SqliteConn::open(&self.get_db_path())
    }

    fn init_schema(&self) -> Result<(), String> {
        let conn = self.connect()?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS feeds (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                title TEXT NOT NULL,
                url TEXT NOT NULL UNIQUE,
                category TEXT NOT NULL DEFAULT 'General',
                tags_json TEXT NOT NULL DEFAULT '[]',
                enabled INTEGER NOT NULL DEFAULT 1,
                last_fetched_at TEXT,
                last_error TEXT,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS queries (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                query_type TEXT NOT NULL CHECK (query_type IN ('noul', 'score')),
                instructions TEXT NOT NULL,
                criteria_json TEXT NOT NULL DEFAULT '{}',
                weight REAL NOT NULL DEFAULT 1.0,
                enabled INTEGER NOT NULL DEFAULT 1,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS entries (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                entry_uid TEXT NOT NULL UNIQUE,
                feed_id INTEGER,
                journal_title TEXT NOT NULL DEFAULT '',
                title TEXT NOT NULL,
                link TEXT NOT NULL DEFAULT '',
                authors TEXT NOT NULL DEFAULT '',
                published_at TEXT NOT NULL DEFAULT '',
                summary TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'archived')),
                archived_at TEXT,
                evaluated_at TEXT,
                created_at TEXT NOT NULL,
                FOREIGN KEY (feed_id) REFERENCES feeds(id) ON DELETE SET NULL
            );

            CREATE INDEX IF NOT EXISTS idx_entries_status ON entries(status);
            CREATE INDEX IF NOT EXISTS idx_entries_uid ON entries(entry_uid);

            CREATE TABLE IF NOT EXISTS entry_scores (
                entry_id INTEGER NOT NULL,
                query_id INTEGER NOT NULL,
                query_name_snapshot TEXT NOT NULL DEFAULT '',
                query_type_snapshot TEXT NOT NULL DEFAULT 'noul',
                weight_snapshot REAL NOT NULL DEFAULT 1.0,
                normalized_score REAL NOT NULL,
                weighted_score_snapshot REAL NOT NULL DEFAULT 0.0,
                raw_outcome_text TEXT NOT NULL DEFAULT '',
                confidence REAL,
                query_snapshot_json TEXT NOT NULL DEFAULT '{}',
                raw_response_json TEXT NOT NULL DEFAULT '{}',
                evaluated_at TEXT NOT NULL,
                PRIMARY KEY (entry_id, query_id),
                FOREIGN KEY (entry_id) REFERENCES entries(id) ON DELETE CASCADE
            );
            "#,
        )?;

        self.migrate_entries_if_needed(&conn)?;
        self.migrate_feeds_if_needed(&conn)?;
        conn.execute(
            "INSERT OR IGNORE INTO settings (key, value) VALUES ('ai_provider', 'openrouter')",
            &[],
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO settings (key, value) VALUES ('provider_api_url', 'https://openrouter.ai/api/alpha/decisions')",
            &[],
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO settings (key, value) VALUES ('provider_wire_format', 'systemone')",
            &[],
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO settings (key, value) VALUES ('jev_model', 'typesafe/jev-1.14')",
            &[],
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO settings (key, value) VALUES ('custom_provider_name', '')",
            &[],
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO settings (key, value) VALUES ('custom_base_url', '')",
            &[],
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO settings (key, value) VALUES ('custom_portal_url', '')",
            &[],
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO settings (key, value) VALUES ('max_entries_per_feed', '25')",
            &[],
        )?;
        self.migrate_provider_settings_if_needed(&conn)?;
        Ok(())
    }

    fn migrate_feeds_if_needed(&self, conn: &SqliteConn) -> Result<(), String> {
        let cols = conn.query_rows("PRAGMA table_info(feeds)", &[])?;
        let has_tags_json = cols.iter().any(|r| val_str(r, "name") == "tags_json");
        if !has_tags_json {
            conn.execute(
                "ALTER TABLE feeds ADD COLUMN tags_json TEXT NOT NULL DEFAULT '[]'",
                &[],
            )?;
        }
        Ok(())
    }

    fn migrate_provider_settings_if_needed(&self, conn: &SqliteConn) -> Result<(), String> {
        let rows = conn.query_rows("SELECT key, value FROM settings", &[])?;
        let mut map = HashMap::new();
        for r in rows {
            map.insert(val_str(&r, "key"), val_str(&r, "value"));
        }

        let raw_provider = map
            .get("ai_provider")
            .map(|s| s.trim().to_lowercase())
            .unwrap_or_else(|| "openrouter".to_string());
        let custom_base = map
            .get("custom_base_url")
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        let current_api_url = map
            .get("provider_api_url")
            .map(|s| s.trim().to_string())
            .unwrap_or_default();

        // Map legacy 'openai' -> 'openai-decisions', keep any valid Decision provider ID, and normalize removed chat-only IDs
        let effective_provider = if raw_provider == "openai" {
            "openai-decisions".to_string()
        } else if crate::jev_client::is_valid_provider_id(&raw_provider) {
            raw_provider.clone()
        } else if !custom_base.is_empty() {
            "custom".to_string()
        } else {
            "openrouter".to_string()
        };

        if effective_provider != raw_provider {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES ('ai_provider', ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                &[effective_provider.as_str().into()],
            )?;
        }

        let spec = crate::jev_client::get_provider_spec(&effective_provider);

        // Migrate legacy chat base URLs (e.g. .../api/v1) to the provider's native Decisions/SystemOne URL
        let is_legacy_chat_url = matches!(
            current_api_url.trim_end_matches('/'),
            "" | "https://openrouter.ai/api/v1" | "https://nano-gpt.com/api/v1" | "https://api.openai.com/v1" | "https://api.perplexity.ai"
        );
        if is_legacy_chat_url {
            let next_url = if !custom_base.is_empty()
                && !matches!(
                    custom_base.trim_end_matches('/'),
                    "https://openrouter.ai/api/v1" | "https://nano-gpt.com/api/v1" | "https://api.openai.com/v1" | "https://api.perplexity.ai"
                ) {
                custom_base.as_str()
            } else {
                spec.api_url
            };
            conn.execute(
                "INSERT INTO settings (key, value) VALUES ('provider_api_url', ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                &[next_url.into()],
            )?;
        }

        // If an old DB had a non-decision chat model stored (e.g. gpt-4o-mini on nanogpt), reset to spec.default_model
        let current_model = map
            .get("jev_model")
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        if matches!(
            current_model.as_str(),
            "" | "gpt-4o-mini" | "deepseek-chat" | "sonar"
        ) {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES ('jev_model', ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                &[spec.default_model.into()],
            )?;
        }

        Ok(())
    }

    fn migrate_entries_if_needed(&self, conn: &SqliteConn) -> Result<(), String> {
        let cols = conn.query_rows("PRAGMA table_info(entries)", &[])?;
        let has_archived_at = cols.iter().any(|r| val_str(r, "name") == "archived_at");
        if !has_archived_at {
            conn.execute("ALTER TABLE entries ADD COLUMN archived_at TEXT", &[])?;
        }
        conn.execute(
            "UPDATE entries SET archived_at = created_at WHERE status = 'archived' AND (archived_at IS NULL OR archived_at = '')",
            &[],
        )?;
        Ok(())
    }

    fn seed_if_empty(&self) -> Result<(), String> {
        let conn = self.connect()?;
        let feed_rows = conn.query_rows("SELECT COUNT(*) AS cnt FROM feeds", &[])?;
        let query_rows = conn.query_rows("SELECT COUNT(*) AS cnt FROM queries", &[])?;
        let feed_count = feed_rows.first().map(|r| val_i64(r, "cnt")).unwrap_or(0);
        let query_count = query_rows.first().map(|r| val_i64(r, "cnt")).unwrap_or(0);
        let now = utc_now_iso();

        if feed_count == 0 {
            let starter_feeds = [
                (
                    "Nature",
                    "https://www.nature.com/nature.rss",
                    "Multidisciplinary",
                    1i64,
                ),
                (
                    "Science",
                    "https://www.science.org/action/showFeed?type=etoc&feed=rss&jc=science",
                    "Multidisciplinary",
                    1i64,
                ),
                (
                    "Nature Communications",
                    "https://www.nature.com/ncomms.rss",
                    "Multidisciplinary",
                    1i64,
                ),
                (
                    "Science Advances",
                    "https://www.science.org/action/showFeed?type=etoc&feed=rss&jc=sciadv",
                    "Multidisciplinary",
                    1i64,
                ),
            ];
            for (title, url, category, enabled) in starter_feeds {
                conn.execute(
                    "INSERT INTO feeds (title, url, category, enabled, created_at) VALUES (?, ?, ?, ?, ?)",
                    &[
                        title.into(),
                        url.into(),
                        category.into(),
                        enabled.into(),
                        now.clone().into(),
                    ],
                )?;
            }
        }

        if query_count == 0 {
            let starter_queries = [
                (
                    "Neuroscience Paper",
                    "noul",
                    "Is the topic of this paper any one of the following: neuroscience, systems neuroscience, computational neuroscience, or network dynamics with neuroscience applications?",
                    json!({
                        "true": "The paper investigates neuroscience, systems neuroscience, computational neuroscience, or neural network dynamics with neuroscience applications.",
                        "false": "The paper is outside neuroscience and has no direct neuroscience application."
                    })
                    .to_string(),
                    2.0f64,
                    1i64,
                ),
                (
                    "Uses fMRI or EEG (Penalty)",
                    "noul",
                    "Does this paper primarily rely on or report functional MRI (fMRI) or electroencephalography (EEG) experiments/data?",
                    json!({
                        "true": "The study uses, analyzes, or focuses on fMRI (functional magnetic resonance imaging) or EEG (electroencephalography) data.",
                        "false": "The study does not involve fMRI or EEG."
                    })
                    .to_string(),
                    -1.0f64,
                    1i64,
                ),
            ];

            for (name, qtype, instructions, criteria_json, weight, enabled) in starter_queries {
                conn.execute(
                    "INSERT INTO queries (name, query_type, instructions, criteria_json, weight, enabled, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
                    &[
                        name.into(),
                        qtype.into(),
                        instructions.into(),
                        criteria_json.into(),
                        weight.into(),
                        enabled.into(),
                        now.clone().into(),
                    ],
                )?;
            }
        }
        Ok(())
    }

    // --- Settings ---
    pub fn get_settings(&self) -> Result<HashMap<String, String>, String> {
        let conn = self.connect()?;
        let rows = conn.query_rows("SELECT key, value FROM settings", &[])?;
        let mut out = HashMap::new();
        for r in rows {
            out.insert(val_str(&r, "key"), val_str(&r, "value"));
        }
        Ok(out)
    }

    pub fn update_settings(
        &self,
        updates: &HashMap<String, String>,
    ) -> Result<HashMap<String, String>, String> {
        let conn = self.connect()?;
        for (k, v) in updates {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                &[k.as_str().into(), v.as_str().into()],
            )?;
        }
        self.get_settings()
    }

    // --- Feeds ---
    pub fn normalize_tags(raw: &[String]) -> Vec<String> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for item in raw {
            for part in item.split(',') {
                let clean = part.trim();
                if clean.is_empty() {
                    continue;
                }
                let lower = clean.to_lowercase();
                if seen.insert(lower) {
                    out.push(clean.to_string());
                }
            }
        }
        out
    }

    pub fn parse_tags_from_value(val: &Value) -> Option<Vec<String>> {
        if let Some(arr) = val.as_array() {
            let raw: Vec<String> = arr
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
            Some(Self::normalize_tags(&raw))
        } else if let Some(s) = val.as_str() {
            Some(Self::normalize_tags(&[s.to_string()]))
        } else {
            None
        }
    }

    fn row_to_feed(row: &HashMap<String, Value>) -> Value {
        let tags_raw = val_str(row, "tags_json");
        let tags: Vec<String> = serde_json::from_str::<Vec<String>>(&tags_raw)
            .map(|v| Self::normalize_tags(&v))
            .unwrap_or_default();

        let mut m = Map::new();
        m.insert("id".to_string(), json!(val_i64(row, "id")));
        m.insert("title".to_string(), json!(val_str(row, "title")));
        m.insert("url".to_string(), json!(val_str(row, "url")));
        m.insert("category".to_string(), json!(val_str(row, "category")));
        m.insert("tags".to_string(), json!(tags));
        m.insert("enabled".to_string(), json!(val_i64(row, "enabled") != 0));
        m.insert(
            "last_fetched_at".to_string(),
            match val_opt_str(row, "last_fetched_at") {
                Some(s) => json!(s),
                None => Value::Null,
            },
        );
        m.insert(
            "last_error".to_string(),
            match val_opt_str(row, "last_error") {
                Some(s) => json!(s),
                None => Value::Null,
            },
        );
        m.insert("created_at".to_string(), json!(val_str(row, "created_at")));
        if row.contains_key("total_entries") {
            m.insert(
                "total_entries".to_string(),
                json!(val_i64(row, "total_entries")),
            );
            m.insert(
                "active_entries".to_string(),
                json!(val_i64(row, "active_entries")),
            );
            m.insert(
                "archived_entries".to_string(),
                json!(val_i64(row, "archived_entries")),
            );
        }
        Value::Object(m)
    }

    pub fn list_feeds(&self) -> Result<Vec<Value>, String> {
        let conn = self.connect()?;
        let rows = conn.query_rows(
            r#"
            SELECT f.*,
                   COUNT(e.id) AS total_entries,
                   COALESCE(SUM(CASE WHEN e.status = 'active' THEN 1 ELSE 0 END), 0) AS active_entries,
                   COALESCE(SUM(CASE WHEN e.status = 'archived' THEN 1 ELSE 0 END), 0) AS archived_entries
            FROM feeds f
            LEFT JOIN entries e ON e.feed_id = f.id
            GROUP BY f.id
            ORDER BY LOWER(f.title) ASC, f.id ASC
            "#,
            &[],
        )?;
        Ok(rows.iter().map(Self::row_to_feed).collect())
    }

    pub fn get_feed(&self, feed_id: i64) -> Result<Option<Value>, String> {
        let conn = self.connect()?;
        let rows = conn.query_rows("SELECT * FROM feeds WHERE id = ?", &[feed_id.into()])?;
        Ok(rows.first().map(Self::row_to_feed))
    }

    pub fn get_feed_by_url(&self, url: &str) -> Result<Option<Value>, String> {
        let conn = self.connect()?;
        let rows = conn.query_rows(
            "SELECT * FROM feeds WHERE LOWER(url) = LOWER(?)",
            &[url.trim().into()],
        )?;
        Ok(rows.first().map(Self::row_to_feed))
    }

    pub fn create_feed(
        &self,
        title: &str,
        url: &str,
        category: &str,
        enabled: bool,
    ) -> Result<Value, String> {
        self.create_feed_with_tags(title, url, category, &[], enabled)
    }

    pub fn create_feed_with_tags(
        &self,
        title: &str,
        url: &str,
        category: &str,
        tags: &[String],
        enabled: bool,
    ) -> Result<Value, String> {
        let now = utc_now_iso();
        let cat = if category.trim().is_empty() {
            "General"
        } else {
            category.trim()
        };
        let clean_tags = Self::normalize_tags(tags);
        let tags_json = serde_json::to_string(&clean_tags).unwrap_or_else(|_| "[]".to_string());
        let conn = self.connect()?;
        let feed_id = conn.insert(
            "INSERT INTO feeds (title, url, category, tags_json, enabled, created_at) VALUES (?, ?, ?, ?, ?, ?)",
            &[
                title.trim().into(),
                url.trim().into(),
                cat.into(),
                tags_json.into(),
                (if enabled { 1i64 } else { 0i64 }).into(),
                now.into(),
            ],
        )?;
        let res = self
            .get_feed(feed_id)?
            .ok_or_else(|| "Failed to retrieve created feed".to_string())?;
        write_rolling_toml_snapshot(self);
        Ok(res)
    }

    pub fn update_feed(
        &self,
        feed_id: i64,
        title: Option<&str>,
        url: Option<&str>,
        category: Option<&str>,
        tags: Option<&[String]>,
        enabled: Option<bool>,
    ) -> Result<Option<Value>, String> {
        let existing = match self.get_feed(feed_id)? {
            Some(f) => f,
            None => return Ok(None),
        };
        let new_title = title
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| existing["title"].as_str().unwrap_or("").to_string());
        let new_url = url
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| existing["url"].as_str().unwrap_or("").to_string());
        let new_cat = category
            .map(|s| {
                let t = s.trim();
                if t.is_empty() {
                    "General".to_string()
                } else {
                    t.to_string()
                }
            })
            .unwrap_or_else(|| existing["category"].as_str().unwrap_or("General").to_string());
        let new_tags: Vec<String> = match tags {
            Some(t) => Self::normalize_tags(t),
            None => existing["tags"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default(),
        };
        let new_tags_json = serde_json::to_string(&new_tags).unwrap_or_else(|_| "[]".to_string());
        let new_enabled = enabled
            .map(|b| if b { 1i64 } else { 0i64 })
            .unwrap_or_else(|| {
                existing["enabled"]
                    .as_bool()
                    .map(|b| if b { 1i64 } else { 0i64 })
                    .or_else(|| existing["enabled"].as_i64())
                    .unwrap_or(1)
            });

        let conn = self.connect()?;
        conn.execute(
            "UPDATE feeds SET title = ?, url = ?, category = ?, tags_json = ?, enabled = ? WHERE id = ?",
            &[
                new_title.into(),
                new_url.into(),
                new_cat.into(),
                new_tags_json.into(),
                new_enabled.into(),
                feed_id.into(),
            ],
        )?;
        let res = self.get_feed(feed_id)?;
        write_rolling_toml_snapshot(self);
        Ok(res)
    }

    pub fn upsert_feed(
        &self,
        title: &str,
        url: &str,
        category: Option<&str>,
        tags: Option<&[String]>,
        enabled: Option<bool>,
    ) -> Result<Value, String> {
        if let Some(existing) = self.get_feed_by_url(url)? {
            let feed_id = existing["id"].as_i64().unwrap_or(0);
            let title_opt = if title.trim().is_empty() {
                None
            } else {
                Some(title.trim())
            };
            if let Some(updated) =
                self.update_feed(feed_id, title_opt, Some(url), category, tags, enabled)?
            {
                return Ok(updated);
            }
            return Ok(existing);
        }
        self.create_feed_with_tags(
            title,
            url,
            category.unwrap_or("General"),
            tags.unwrap_or(&[]),
            enabled.unwrap_or(true),
        )
    }

    pub fn mark_feed_fetched(&self, feed_id: i64, error: Option<&str>) -> Result<(), String> {
        let conn = self.connect()?;
        let now = utc_now_iso();
        conn.execute(
            "UPDATE feeds SET last_fetched_at = ?, last_error = ? WHERE id = ?",
            &[now.into(), error.into(), feed_id.into()],
        )?;
        Ok(())
    }

    pub fn delete_feed(&self, feed_id: i64) -> Result<bool, String> {
        let conn = self.connect()?;
        let changes = conn.execute("DELETE FROM feeds WHERE id = ?", &[feed_id.into()])?;
        if changes > 0 {
            write_rolling_toml_snapshot(self);
        }
        Ok(changes > 0)
    }

    pub fn delete_feed_by_url(&self, url: &str) -> Result<bool, String> {
        let conn = self.connect()?;
        let changes = conn.execute(
            "DELETE FROM feeds WHERE LOWER(url) = LOWER(?)",
            &[url.trim().into()],
        )?;
        if changes > 0 {
            write_rolling_toml_snapshot(self);
        }
        Ok(changes > 0)
    }

    // --- Jev Queries ---
    fn deserialize_query(row: &HashMap<String, Value>) -> Value {
        let qtype = val_str(row, "query_type");
        let crit_raw = val_str(row, "criteria_json");
        let criteria: Value = serde_json::from_str(&crit_raw).unwrap_or_else(|_| {
            if qtype == "noul" {
                json!({})
            } else {
                json!([])
            }
        });
        json!({
            "id": val_i64(row, "id"),
            "name": val_str(row, "name"),
            "query_type": qtype,
            "instructions": val_str(row, "instructions"),
            "criteria_json": crit_raw,
            "criteria": criteria,
            "weight": val_f64(row, "weight", 1.0),
            "enabled": val_i64(row, "enabled") != 0,
            "created_at": val_str(row, "created_at"),
        })
    }

    pub fn list_queries(&self, only_enabled: bool) -> Result<Vec<Value>, String> {
        let conn = self.connect()?;
        let sql = if only_enabled {
            "SELECT * FROM queries WHERE enabled = 1 ORDER BY id ASC"
        } else {
            "SELECT * FROM queries ORDER BY id ASC"
        };
        let rows = conn.query_rows(sql, &[])?;
        Ok(rows.iter().map(Self::deserialize_query).collect())
    }

    pub fn get_query(&self, query_id: i64) -> Result<Option<Value>, String> {
        let conn = self.connect()?;
        let rows = conn.query_rows("SELECT * FROM queries WHERE id = ?", &[query_id.into()])?;
        Ok(rows.first().map(Self::deserialize_query))
    }

    pub fn get_query_by_name(&self, name: &str) -> Result<Option<Value>, String> {
        let conn = self.connect()?;
        let rows = conn.query_rows(
            "SELECT * FROM queries WHERE LOWER(name) = LOWER(?)",
            &[name.trim().into()],
        )?;
        Ok(rows.first().map(Self::deserialize_query))
    }

    fn clean_query_criteria(query_type: &str, criteria: &Value) -> Result<Value, String> {
        if query_type != "noul" && query_type != "score" {
            return Err("query_type must be 'noul' or 'score'".to_string());
        }
        if query_type == "score" {
            let arr = criteria
                .as_array()
                .ok_or_else(|| "A 'score' query requires at least 2 ordered criteria levels.".to_string())?;
            let clean: Vec<Value> = arr
                .iter()
                .filter_map(|v| {
                    let s = match v {
                        Value::String(s) => s.trim().to_string(),
                        other => other.to_string().trim().to_string(),
                    };
                    if s.is_empty() {
                        None
                    } else {
                        Some(Value::String(s))
                    }
                })
                .collect();
            if clean.len() < 2 {
                return Err(
                    "A 'score' query requires at least 2 ordered criteria levels.".to_string(),
                );
            }
            Ok(Value::Array(clean))
        } else {
            let mut map = Map::new();
            if let Some(obj) = criteria.as_object() {
                for key in ["true", "false"] {
                    if let Some(val) = obj.get(key) {
                        let s = val.as_str().unwrap_or("").trim();
                        if !s.is_empty() {
                            map.insert(key.to_string(), Value::String(s.to_string()));
                        }
                    }
                }
            }
            Ok(Value::Object(map))
        }
    }

    pub fn create_query(
        &self,
        name: &str,
        query_type: &str,
        instructions: &str,
        criteria: &Value,
        weight: f64,
        enabled: bool,
    ) -> Result<Value, String> {
        let clean_criteria = Self::clean_query_criteria(query_type, criteria)?;
        let now = utc_now_iso();
        let conn = self.connect()?;
        let qid = conn.insert(
            "INSERT INTO queries (name, query_type, instructions, criteria_json, weight, enabled, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
            &[
                name.trim().into(),
                query_type.into(),
                instructions.trim().into(),
                clean_criteria.to_string().into(),
                weight.into(),
                (if enabled { 1i64 } else { 0i64 }).into(),
                now.into(),
            ],
        )?;
        let res = self
            .get_query(qid)?
            .ok_or_else(|| "Failed to retrieve created query".to_string())?;
        write_rolling_toml_snapshot(self);
        Ok(res)
    }

    pub fn update_query(
        &self,
        query_id: i64,
        name: Option<&str>,
        query_type: Option<&str>,
        instructions: Option<&str>,
        criteria: Option<&Value>,
        weight: Option<f64>,
        enabled: Option<bool>,
    ) -> Result<Option<Value>, String> {
        let existing = match self.get_query(query_id)? {
            Some(q) => q,
            None => return Ok(None),
        };
        let new_name = name
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| existing["name"].as_str().unwrap_or("").to_string());
        let new_type = query_type
            .map(|s| s.to_string())
            .unwrap_or_else(|| existing["query_type"].as_str().unwrap_or("noul").to_string());
        let new_instr = instructions
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| existing["instructions"].as_str().unwrap_or("").to_string());
        let new_weight = weight.unwrap_or_else(|| existing["weight"].as_f64().unwrap_or(1.0));
        let new_enabled = enabled
            .map(|b| if b { 1i64 } else { 0i64 })
            .unwrap_or_else(|| if existing["enabled"].as_bool().unwrap_or(true) { 1 } else { 0 });

        let raw_criteria = criteria.unwrap_or(&existing["criteria"]);
        let clean_criteria = Self::clean_query_criteria(&new_type, raw_criteria)?;

        let conn = self.connect()?;
        conn.execute(
            "UPDATE queries SET name = ?, query_type = ?, instructions = ?, criteria_json = ?, weight = ?, enabled = ? WHERE id = ?",
            &[
                new_name.into(),
                new_type.into(),
                new_instr.into(),
                clean_criteria.to_string().into(),
                new_weight.into(),
                new_enabled.into(),
                query_id.into(),
            ],
        )?;
        let res = self.get_query(query_id)?;
        write_rolling_toml_snapshot(self);
        Ok(res)
    }

    pub fn upsert_query(
        &self,
        name: Option<&str>,
        query_type: Option<&str>,
        instructions: Option<&str>,
        criteria: Option<&Value>,
        weight: Option<f64>,
        enabled: Option<bool>,
        query_id: Option<i64>,
    ) -> Result<Value, String> {
        let mut existing: Option<Value> = None;
        if let Some(qid) = query_id {
            existing = self.get_query(qid)?;
        }
        if existing.is_none() {
            if let Some(n) = name {
                existing = self.get_query_by_name(n)?;
            }
        }

        if let Some(ex) = existing {
            let qid = ex["id"].as_i64().unwrap_or(0);
            let updated = self.update_query(
                qid,
                name,
                query_type,
                instructions,
                criteria,
                weight,
                enabled,
            )?;
            return updated.ok_or_else(|| format!("Failed to update query id={qid}"));
        }

        let clean_name = name.map(|s| s.trim()).unwrap_or("");
        if clean_name.is_empty() {
            return Err("A query 'name' is required when creating a new query.".to_string());
        }
        let clean_instr = instructions.map(|s| s.trim()).unwrap_or("");
        if clean_instr.is_empty() {
            return Err(format!(
                "Query '{clean_name}' does not exist yet; provide 'instructions' (and 'query_type') to create it."
            ));
        }
        let resolved_type = query_type.unwrap_or("noul");
        let default_crit = if resolved_type == "noul" {
            json!({})
        } else {
            json!([])
        };
        let resolved_criteria = criteria.unwrap_or(&default_crit);
        self.create_query(
            clean_name,
            resolved_type,
            clean_instr,
            resolved_criteria,
            weight.unwrap_or(1.0),
            enabled.unwrap_or(true),
        )
    }

    pub fn delete_query(&self, query_id: i64) -> Result<bool, String> {
        let conn = self.connect()?;
        let changes = conn.execute("DELETE FROM queries WHERE id = ?", &[query_id.into()])?;
        if changes > 0 {
            write_rolling_toml_snapshot(self);
        }
        Ok(changes > 0)
    }

    pub fn delete_query_by_name(&self, name: &str) -> Result<bool, String> {
        let conn = self.connect()?;
        let changes = conn.execute(
            "DELETE FROM queries WHERE LOWER(name) = LOWER(?)",
            &[name.trim().into()],
        )?;
        if changes > 0 {
            write_rolling_toml_snapshot(self);
        }
        Ok(changes > 0)
    }

    // --- Entries & Memory / Deduplication ---
    pub fn get_known_entry_uids(&self) -> Result<HashSet<String>, String> {
        let conn = self.connect()?;
        let rows = conn.query_rows("SELECT entry_uid FROM entries", &[])?;
        Ok(rows.iter().map(|r| val_str(r, "entry_uid")).collect())
    }

    pub fn get_known_entry_summaries(&self) -> Result<HashMap<String, String>, String> {
        let conn = self.connect()?;
        let rows = conn.query_rows("SELECT entry_uid, summary FROM entries", &[])?;
        Ok(rows
            .into_iter()
            .map(|r| (val_str(&r, "entry_uid"), val_str(&r, "summary")))
            .collect())
    }

    fn row_to_entry_basic(row: &HashMap<String, Value>) -> Value {
        json!({
            "id": val_i64(row, "id"),
            "entry_uid": val_str(row, "entry_uid"),
            "feed_id": row.get("feed_id").cloned().unwrap_or(Value::Null),
            "journal_title": val_str(row, "journal_title"),
            "title": val_str(row, "title"),
            "link": val_str(row, "link"),
            "authors": val_str(row, "authors"),
            "published_at": val_str(row, "published_at"),
            "summary": val_str(row, "summary"),
            "status": val_str(row, "status"),
            "archived_at": row.get("archived_at").cloned().unwrap_or(Value::Null),
            "evaluated_at": row.get("evaluated_at").cloned().unwrap_or(Value::Null),
            "created_at": val_str(row, "created_at"),
        })
    }

    pub fn insert_new_entries(&self, parsed_entries: &[Value]) -> Result<Vec<Value>, String> {
        if parsed_entries.is_empty() {
            return Ok(Vec::new());
        }
        let now = utc_now_iso();
        let conn = self.connect()?;
        let existing_rows = conn.query_rows("SELECT entry_uid FROM entries", &[])?;
        let mut existing_uids: HashSet<String> =
            existing_rows.iter().map(|r| val_str(r, "entry_uid")).collect();

        let mut newly_inserted = Vec::new();
        for item in parsed_entries {
            let uid = item["entry_uid"].as_str().unwrap_or("").to_string();
            if uid.is_empty() || existing_uids.contains(&uid) {
                continue;
            }
            let feed_id_param = match item.get("feed_id").and_then(|v| v.as_i64()) {
                Some(fid) => SqlParam::Int(fid),
                None => SqlParam::Null,
            };
            let journal_title = item["journal_title"].as_str().unwrap_or("");
            let title = item["title"].as_str().unwrap_or("Untitled");
            let link = item["link"].as_str().unwrap_or("");
            let authors = item["authors"].as_str().unwrap_or("");
            let published_at = item["published_at"].as_str().unwrap_or("");
            let summary = item["summary"].as_str().unwrap_or("");

            let new_id = conn.insert(
                r#"
                INSERT INTO entries (
                    entry_uid, feed_id, journal_title, title, link,
                    authors, published_at, summary, status, created_at
                )
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'active', ?)
                "#,
                &[
                    uid.clone().into(),
                    feed_id_param,
                    journal_title.into(),
                    title.into(),
                    link.into(),
                    authors.into(),
                    published_at.into(),
                    summary.into(),
                    now.clone().into(),
                ],
            )?;
            existing_uids.insert(uid);
            let rows = conn.query_rows("SELECT * FROM entries WHERE id = ?", &[new_id.into()])?;
            if let Some(r) = rows.first() {
                newly_inserted.push(Self::row_to_entry_basic(r));
            }
        }
        Ok(newly_inserted)
    }

    pub fn save_entry_scores(
        &self,
        entry_id: i64,
        scores_by_query_id: &HashMap<i64, Value>,
    ) -> Result<(), String> {
        let now = utc_now_iso();
        let conn = self.connect()?;
        let query_rows = conn.query_rows("SELECT * FROM queries", &[])?;
        let mut queries_map = HashMap::new();
        for r in &query_rows {
            let q = Self::deserialize_query(r);
            if let Some(id) = q["id"].as_i64() {
                queries_map.insert(id, q);
            }
        }

        for (&qid, score_data) in scores_by_query_id {
            let q_live = queries_map.get(&qid);
            let qname = score_data
                .get("query_name_snapshot")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .or_else(|| {
                    q_live
                        .and_then(|q| q["name"].as_str())
                        .map(|s| s.to_string())
                })
                .unwrap_or_else(|| format!("Query #{qid}"));

            let qtype = score_data
                .get("query_type_snapshot")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .or_else(|| {
                    q_live
                        .and_then(|q| q["query_type"].as_str())
                        .map(|s| s.to_string())
                })
                .unwrap_or_else(|| "noul".to_string());

            let qweight = score_data
                .get("weight_snapshot")
                .and_then(|v| v.as_f64())
                .or_else(|| q_live.and_then(|q| q["weight"].as_f64()))
                .unwrap_or(1.0);

            let norm = score_data
                .get("normalized_score")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);

            let weighted_snap = score_data
                .get("weighted_score_snapshot")
                .and_then(|v| v.as_f64())
                .unwrap_or_else(|| round4(qweight * norm));

            let raw_resp = score_data
                .get("raw_response")
                .cloned()
                .unwrap_or_else(|| json!({}));

            let mut conf_val = score_data.get("confidence").and_then(|v| v.as_f64());
            if conf_val.is_none() {
                if let Some(c) = raw_resp.get("confidence").and_then(|v| v.as_f64()) {
                    conf_val = Some(c);
                } else if qtype == "noul" {
                    conf_val = Some(norm.max(1.0 - norm));
                }
            }

            let mut raw_text = score_data
                .get("raw_outcome_text")
                .or_else(|| score_data.get("display_value"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            if raw_text.is_empty() {
                if qtype == "noul" {
                    let verdict = if norm >= 0.5 { "Yes" } else { "No" };
                    let c_pct = conf_val.unwrap_or_else(|| norm.max(1.0 - norm)) * 100.0;
                    raw_text = format!("{verdict} ({c_pct:.1}% conf · P(Yes)={:.1}%)", norm * 100.0);
                } else {
                    let sc = raw_resp
                        .get("score")
                        .and_then(|v| v.as_f64())
                        .unwrap_or_else(|| round4(1.0 + norm * 2.0));
                    let max_l = raw_resp
                        .get("max_level")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(3);
                    let c_str = match conf_val {
                        Some(c) => format!(" · {:.1}% conf", c * 100.0),
                        None => String::new(),
                    };
                    raw_text = format!("{sc} / {max_l} ({:.0}%{c_str})", norm * 100.0);
                }
            }

            let q_snap = score_data.get("query_snapshot").cloned().unwrap_or_else(|| {
                json!({
                    "id": qid,
                    "name": qname,
                    "query_type": qtype,
                    "weight": qweight,
                    "instructions": q_live.and_then(|q| q["instructions"].as_str()).unwrap_or(""),
                    "criteria": q_live.map(|q| q["criteria"].clone()).unwrap_or_else(|| json!({})),
                })
            });

            let conf_param = match conf_val {
                Some(c) => SqlParam::Float(c),
                None => SqlParam::Null,
            };

            conn.execute(
                r#"
                INSERT INTO entry_scores (
                    entry_id, query_id, query_name_snapshot, query_type_snapshot,
                    weight_snapshot, normalized_score, weighted_score_snapshot,
                    raw_outcome_text, confidence, query_snapshot_json,
                    raw_response_json, evaluated_at
                )
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT(entry_id, query_id) DO UPDATE SET
                    query_name_snapshot = excluded.query_name_snapshot,
                    query_type_snapshot = excluded.query_type_snapshot,
                    weight_snapshot = excluded.weight_snapshot,
                    normalized_score = excluded.normalized_score,
                    weighted_score_snapshot = excluded.weighted_score_snapshot,
                    raw_outcome_text = excluded.raw_outcome_text,
                    confidence = excluded.confidence,
                    query_snapshot_json = excluded.query_snapshot_json,
                    raw_response_json = excluded.raw_response_json,
                    evaluated_at = excluded.evaluated_at
                "#,
                &[
                    entry_id.into(),
                    qid.into(),
                    qname.into(),
                    qtype.into(),
                    qweight.into(),
                    norm.into(),
                    weighted_snap.into(),
                    raw_text.into(),
                    conf_param,
                    q_snap.to_string().into(),
                    raw_resp.to_string().into(),
                    now.clone().into(),
                ],
            )?;
        }

        conn.execute(
            "UPDATE entries SET evaluated_at = ? WHERE id = ?",
            &[now.into(), entry_id.into()],
        )?;
        Ok(())
    }

    pub fn get_unscored_active_entries(
        &self,
        active_query_ids: &[i64],
    ) -> Result<Vec<Value>, String> {
        if active_query_ids.is_empty() {
            return Ok(Vec::new());
        }
        let conn = self.connect()?;
        let active_rows = conn.query_rows(
            "SELECT * FROM entries WHERE status = 'active' ORDER BY id DESC",
            &[],
        )?;
        if active_rows.is_empty() {
            return Ok(Vec::new());
        }
        let score_rows = conn.query_rows(
            r#"
            SELECT es.entry_id, es.query_id
            FROM entry_scores es
            JOIN entries e ON e.id = es.entry_id
            WHERE e.status = 'active'
            "#,
            &[],
        )?;

        let mut scored_map: HashMap<i64, HashSet<i64>> = HashMap::new();
        for r in score_rows {
            let eid = val_i64(&r, "entry_id");
            let qid = val_i64(&r, "query_id");
            scored_map.entry(eid).or_default().insert(qid);
        }

        let required_set: HashSet<i64> = active_query_ids.iter().copied().collect();
        let mut missing_entries = Vec::new();
        for row in active_rows {
            let eid = val_i64(&row, "id");
            let existing_qids = scored_map.get(&eid);
            let mut missing_qids: Vec<i64> = required_set
                .iter()
                .filter(|qid| existing_qids.map(|s| !s.contains(qid)).unwrap_or(true))
                .copied()
                .collect();
            if !missing_qids.is_empty() {
                missing_qids.sort_unstable();
                let mut val = Self::row_to_entry_basic(&row);
                if let Some(obj) = val.as_object_mut() {
                    obj.insert("missing_query_ids".to_string(), json!(missing_qids));
                }
                missing_entries.push(val);
            }
        }
        Ok(missing_entries)
    }

    pub fn list_entries(
        &self,
        status: &str,
        feed_id: Option<i64>,
        search: Option<&str>,
    ) -> Result<Vec<Value>, String> {
        let slice = feed_id.as_ref().map(std::slice::from_ref);
        self.list_entries_filtered(status, slice, search)
    }

    pub fn list_entries_filtered(
        &self,
        status: &str,
        feed_ids: Option<&[i64]>,
        search: Option<&str>,
    ) -> Result<Vec<Value>, String> {
        let conn = self.connect()?;
        let query_rows = conn.query_rows("SELECT * FROM queries", &[])?;
        let mut all_queries_map: HashMap<i64, Value> = HashMap::new();
        for r in &query_rows {
            let q = Self::deserialize_query(r);
            if let Some(id) = q["id"].as_i64() {
                all_queries_map.insert(id, q);
            }
        }

        let mut sql = "SELECT * FROM entries WHERE status = ?".to_string();
        let mut params: Vec<SqlParam> = vec![status.into()];
        if let Some(fids) = feed_ids {
            if fids.is_empty() {
                return Ok(Vec::new());
            } else if fids.len() == 1 {
                sql.push_str(" AND feed_id = ?");
                params.push(fids[0].into());
            } else {
                let placeholders = std::iter::repeat("?")
                    .take(fids.len())
                    .collect::<Vec<_>>()
                    .join(",");
                sql.push_str(&format!(" AND feed_id IN ({placeholders})"));
                for &fid in fids {
                    params.push(fid.into());
                }
            }
        }
        if let Some(s) = search {
            let trimmed = s.trim();
            if !trimmed.is_empty() {
                let q_like = format!("%{trimmed}%");
                sql.push_str(
                    " AND (title LIKE ? OR summary LIKE ? OR authors LIKE ? OR journal_title LIKE ?)",
                );
                params.push(q_like.clone().into());
                params.push(q_like.clone().into());
                params.push(q_like.clone().into());
                params.push(q_like.into());
            }
        }

        let entry_rows = conn.query_rows(&sql, &params)?;
        if entry_rows.is_empty() {
            return Ok(Vec::new());
        }

        let entry_ids: Vec<i64> = entry_rows.iter().map(|r| val_i64(r, "id")).collect();
        let placeholders = std::iter::repeat("?")
            .take(entry_ids.len())
            .collect::<Vec<_>>()
            .join(",");
        let score_sql = format!("SELECT * FROM entry_scores WHERE entry_id IN ({placeholders})");
        let score_params: Vec<SqlParam> = entry_ids.iter().map(|&id| id.into()).collect();
        let score_rows = conn.query_rows(&score_sql, &score_params)?;

        let mut scores_by_entry: HashMap<i64, Vec<Value>> = HashMap::new();
        for srd in score_rows {
            let entry_id = val_i64(&srd, "entry_id");
            let qid = val_i64(&srd, "query_id");
            let q_live = all_queries_map.get(&qid);

            let raw_resp: Value =
                serde_json::from_str(&val_str(&srd, "raw_response_json")).unwrap_or_else(|_| json!({}));
            let q_snap: Value =
                serde_json::from_str(&val_str(&srd, "query_snapshot_json")).unwrap_or_else(|_| json!({}));

            let snap_name = {
                let s = val_str(&srd, "query_name_snapshot");
                if !s.is_empty() {
                    s
                } else if let Some(n) = q_snap.get("name").and_then(|v| v.as_str()) {
                    n.to_string()
                } else if let Some(ql) = q_live {
                    ql["name"].as_str().unwrap_or("").to_string()
                } else {
                    format!("Query #{qid}")
                }
            };

            let snap_type = {
                let s = val_str(&srd, "query_type_snapshot");
                if !s.is_empty() {
                    s
                } else if let Some(t) = q_snap.get("query_type").and_then(|v| v.as_str()) {
                    t.to_string()
                } else if let Some(ql) = q_live {
                    ql["query_type"].as_str().unwrap_or("noul").to_string()
                } else {
                    "noul".to_string()
                }
            };

            let snap_weight = srd
                .get("weight_snapshot")
                .and_then(|v| v.as_f64())
                .or_else(|| q_snap.get("weight").and_then(|v| v.as_f64()))
                .or_else(|| q_live.and_then(|ql| ql["weight"].as_f64()))
                .unwrap_or(1.0);

            let norm_score = val_f64(&srd, "normalized_score", 0.0);
            let mut conf_val = srd.get("confidence").and_then(|v| v.as_f64());
            if conf_val.is_none() {
                if let Some(c) = raw_resp.get("confidence").and_then(|v| v.as_f64()) {
                    conf_val = Some(c);
                } else if snap_type == "noul" {
                    conf_val = Some(norm_score.max(1.0 - norm_score));
                }
            }

            let mut raw_outcome = val_str(&srd, "raw_outcome_text");
            if raw_outcome.is_empty() {
                if snap_type == "noul" {
                    let verdict = if norm_score >= 0.5 { "Yes" } else { "No" };
                    let c_pct =
                        conf_val.unwrap_or_else(|| norm_score.max(1.0 - norm_score)) * 100.0;
                    raw_outcome =
                        format!("{verdict} ({c_pct:.1}% conf · P(Yes)={:.1}%)", norm_score * 100.0);
                } else {
                    let sc = raw_resp
                        .get("score")
                        .and_then(|v| v.as_f64())
                        .unwrap_or_else(|| round4(1.0 + norm_score * 2.0));
                    let max_l = raw_resp
                        .get("max_level")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(3);
                    let c_str = match conf_val {
                        Some(c) => format!(" · {:.1}% conf", c * 100.0),
                        None => String::new(),
                    };
                    raw_outcome = format!("{sc} / {max_l} ({:.0}%{c_str})", norm_score * 100.0);
                }
            }

            let query_deleted = q_live.is_none();
            let mut query_modified = false;
            if let Some(ql) = q_live {
                if q_snap.is_object() {
                    let live_name = ql["name"].as_str().unwrap_or("");
                    let snap_n = q_snap
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or(&snap_name);
                    let live_type = ql["query_type"].as_str().unwrap_or("");
                    let snap_t = q_snap
                        .get("query_type")
                        .and_then(|v| v.as_str())
                        .unwrap_or(&snap_type);
                    let snap_instr = q_snap
                        .get("instructions")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let live_instr = ql["instructions"].as_str().unwrap_or("");
                    let crit_diff = match q_snap.get("criteria") {
                        Some(sc) if !sc.is_null() => {
                            let is_non_empty = sc
                                .as_object()
                                .map(|o| !o.is_empty())
                                .or_else(|| sc.as_array().map(|a| !a.is_empty()))
                                .unwrap_or(false);
                            is_non_empty && (&ql["criteria"] != sc)
                        }
                        _ => false,
                    };

                    if live_name != snap_n
                        || live_type != snap_t
                        || (!snap_instr.is_empty() && live_instr != snap_instr)
                        || crit_diff
                    {
                        query_modified = true;
                    }
                }
            }

            let (effective_weight, is_enabled) = if query_deleted || query_modified {
                (snap_weight, true)
            } else if let Some(ql) = q_live {
                (
                    ql["weight"].as_f64().unwrap_or(snap_weight),
                    ql["enabled"].as_bool().unwrap_or(true),
                )
            } else {
                (snap_weight, true)
            };

            let weighted_contrib = if is_enabled {
                round4(effective_weight * norm_score)
            } else {
                0.0
            };

            let weighted_score_at_run = srd
                .get("weighted_score_snapshot")
                .and_then(|v| v.as_f64())
                .unwrap_or_else(|| round4(snap_weight * norm_score));

            scores_by_entry.entry(entry_id).or_default().push(json!({
                "query_id": qid,
                "query_name": snap_name,
                "query_type": snap_type,
                "weight": effective_weight,
                "weight_at_run": snap_weight,
                "enabled": is_enabled,
                "normalized_score": norm_score,
                "confidence": conf_val.map(round4),
                "raw_outcome_text": raw_outcome,
                "weighted_contribution": weighted_contrib,
                "weighted_score_at_run": weighted_score_at_run,
                "query_modified_since_run": query_modified,
                "query_deleted_since_run": query_deleted,
                "query_snapshot": q_snap,
                "raw_response": raw_resp,
                "evaluated_at": val_str(&srd, "evaluated_at"),
            }));
        }

        let mut results: Vec<Value> = Vec::with_capacity(entry_rows.len());
        for er in entry_rows {
            let eid = val_i64(&er, "id");
            let mut escores = scores_by_entry.remove(&eid).unwrap_or_default();
            escores.sort_by_key(|x| x["query_id"].as_i64().unwrap_or(0));

            let weighted_sum: f64 = escores
                .iter()
                .filter(|item| item["enabled"].as_bool().unwrap_or(false))
                .map(|item| item["weighted_contribution"].as_f64().unwrap_or(0.0))
                .sum();

            let entry_pos_weight: f64 = escores
                .iter()
                .filter(|item| item["enabled"].as_bool().unwrap_or(false))
                .map(|item| item["weight"].as_f64().unwrap_or(0.0))
                .filter(|&w| w > 0.0)
                .sum();

            let entry_abs_weight: f64 = escores
                .iter()
                .filter(|item| item["enabled"].as_bool().unwrap_or(false))
                .map(|item| item["weight"].as_f64().unwrap_or(0.0).abs())
                .sum();

            let denom_weight = if entry_pos_weight > 0.0 {
                entry_pos_weight
            } else {
                entry_abs_weight
            };

            let relevance_pct = if denom_weight > 0.0 {
                (weighted_sum / denom_weight) * 100.0
            } else {
                0.0
            };

            let is_scored = !escores.is_empty();
            let mut d = Self::row_to_entry_basic(&er);
            if let Some(obj) = d.as_object_mut() {
                obj.insert("query_scores".to_string(), json!(escores));
                obj.insert("weighted_score".to_string(), json!(round4(weighted_sum)));
                obj.insert(
                    "max_possible_score".to_string(),
                    json!(round4(entry_pos_weight)),
                );
                obj.insert("relevance_pct".to_string(), json!(round1(relevance_pct)));
                obj.insert("is_scored".to_string(), json!(is_scored));
            }
            results.push(d);
        }

        results.sort_by(|a, b| {
            let a_scored = a["is_scored"].as_bool().unwrap_or(false);
            let b_scored = b["is_scored"].as_bool().unwrap_or(false);
            let a_w = a["weighted_score"].as_f64().unwrap_or(0.0);
            let b_w = b["weighted_score"].as_f64().unwrap_or(0.0);
            let a_pub = a["published_at"].as_str().unwrap_or("");
            let b_pub = b["published_at"].as_str().unwrap_or("");
            let a_id = a["id"].as_i64().unwrap_or(0);
            let b_id = b["id"].as_i64().unwrap_or(0);

            b_scored
                .cmp(&a_scored)
                .then_with(|| b_w.partial_cmp(&a_w).unwrap_or(std::cmp::Ordering::Equal))
                .then_with(|| b_pub.cmp(a_pub))
                .then_with(|| b_id.cmp(&a_id))
        });

        Ok(results)
    }

    pub fn get_feed_relevance_stats(&self, status: &str) -> Result<Map<String, Value>, String> {
        let entries = self.list_entries_filtered(status, None, None)?;
        let mut by_feed: HashMap<i64, (usize, Vec<f64>)> = HashMap::new();
        for e in &entries {
            if let Some(fid) = e.get("feed_id").and_then(|v| v.as_i64()) {
                let entry_stat = by_feed.entry(fid).or_insert_with(|| (0, Vec::new()));
                entry_stat.0 += 1;
                if e.get("is_scored").and_then(|v| v.as_bool()).unwrap_or(false) {
                    let pct = e
                        .get("relevance_pct")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);
                    entry_stat.1.push(pct);
                }
            }
        }

        let mut out = Map::new();
        for (fid, (total, scored_pcts)) in by_feed {
            out.insert(
                fid.to_string(),
                json!({
                    "total": total,
                    "scored_pcts": scored_pcts,
                }),
            );
        }
        Ok(out)
    }

    pub fn set_entry_status(&self, entry_id: i64, status: &str) -> Result<Option<Value>, String> {
        if status != "active" && status != "archived" {
            return Err("status must be 'active' or 'archived'".to_string());
        }
        let archived_at: Option<String> = if status == "archived" {
            Some(utc_now_iso())
        } else {
            None
        };
        let conn = self.connect()?;
        let changes = conn.execute(
            "UPDATE entries SET status = ?, archived_at = ? WHERE id = ?",
            &[status.into(), archived_at.into(), entry_id.into()],
        )?;
        if changes == 0 {
            return Ok(None);
        }
        let rows = conn.query_rows("SELECT * FROM entries WHERE id = ?", &[entry_id.into()])?;
        Ok(rows.first().map(Self::row_to_entry_basic))
    }

    pub fn archive_entries_bulk(&self, entry_ids: &[i64]) -> Result<usize, String> {
        if entry_ids.is_empty() {
            return Ok(0);
        }
        let now = utc_now_iso();
        let placeholders = std::iter::repeat("?")
            .take(entry_ids.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "UPDATE entries SET status = 'archived', archived_at = ? WHERE id IN ({placeholders}) AND status = 'active'"
        );
        let mut params: Vec<SqlParam> = Vec::with_capacity(entry_ids.len() + 1);
        params.push(now.into());
        for &id in entry_ids {
            params.push(id.into());
        }
        let conn = self.connect()?;
        conn.execute(&sql, &params)
    }

    pub fn prune_old_archived_entries(
        &self,
        days: i64,
        now: Option<DateTime<Utc>>,
    ) -> Result<usize, String> {
        let ref_now = now.unwrap_or_else(Utc::now);
        let cutoff = ref_now - Duration::days(days);
        let cutoff_iso = cutoff.format("%Y-%m-%dT%H:%M:%S+00:00").to_string();

        let conn = self.connect()?;
        let rows = conn.query_rows(
            "SELECT id, archived_at, created_at FROM entries WHERE status = 'archived'",
            &[],
        )?;

        let mut deleted_ids: Vec<i64> = Vec::new();
        for r in rows {
            let ts_str = val_opt_str(&r, "archived_at")
                .filter(|s| !s.trim().is_empty())
                .or_else(|| val_opt_str(&r, "created_at"))
                .unwrap_or_default();
            let trimmed = ts_str.trim();
            if trimmed.is_empty() {
                continue;
            }
            let normalized = trimmed.replace('Z', "+00:00");
            if let Ok(dt) = DateTime::parse_from_rfc3339(&normalized) {
                if dt.with_timezone(&Utc) < cutoff {
                    deleted_ids.push(val_i64(&r, "id"));
                }
            } else if trimmed < cutoff_iso.as_str() {
                deleted_ids.push(val_i64(&r, "id"));
            }
        }

        if deleted_ids.is_empty() {
            return Ok(0);
        }

        let placeholders = std::iter::repeat("?")
            .take(deleted_ids.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!("DELETE FROM entries WHERE status = 'archived' AND id IN ({placeholders})");
        let params: Vec<SqlParam> = deleted_ids.into_iter().map(|id| id.into()).collect();
        conn.execute(&sql, &params)
    }

    pub fn reset_all_entries(&self) -> Result<usize, String> {
        let conn = self.connect()?;
        let deleted_count = conn.execute("DELETE FROM entries", &[])?;
        conn.execute("DELETE FROM entry_scores", &[])?;
        conn.execute(
            "UPDATE feeds SET last_fetched_at = NULL, last_error = NULL",
            &[],
        )?;
        Ok(deleted_count)
    }

    pub fn get_counts(&self) -> Result<Value, String> {
        let conn = self.connect()?;
        let active_c = conn
            .query_rows(
                "SELECT COUNT(*) AS cnt FROM entries WHERE status = 'active'",
                &[],
            )?
            .first()
            .map(|r| val_i64(r, "cnt"))
            .unwrap_or(0);
        let archived_c = conn
            .query_rows(
                "SELECT COUNT(*) AS cnt FROM entries WHERE status = 'archived'",
                &[],
            )?
            .first()
            .map(|r| val_i64(r, "cnt"))
            .unwrap_or(0);
        let feeds_c = conn
            .query_rows("SELECT COUNT(*) AS cnt FROM feeds", &[])?
            .first()
            .map(|r| val_i64(r, "cnt"))
            .unwrap_or(0);
        let queries_c = conn
            .query_rows("SELECT COUNT(*) AS cnt FROM queries", &[])?
            .first()
            .map(|r| val_i64(r, "cnt"))
            .unwrap_or(0);

        Ok(json!({
            "active_entries": active_c,
            "archived_entries": archived_c,
            "total_remembered_entries": active_c + archived_c,
            "feeds": feeds_c,
            "queries": queries_c,
        }))
    }

    pub fn update_entry_timestamps_for_test(
        &self,
        entry_id: i64,
        archived_at: Option<&str>,
        created_at: &str,
    ) -> Result<(), String> {
        let conn = self.connect()?;
        if let Some(arch) = archived_at {
            conn.execute(
                "UPDATE entries SET archived_at = ?1, created_at = ?2 WHERE id = ?3",
                &[
                    SqlParam::Text(arch.to_string()),
                    SqlParam::Text(created_at.to_string()),
                    SqlParam::Int(entry_id),
                ],
            )?;
        } else {
            conn.execute(
                "UPDATE entries SET created_at = ?1 WHERE id = ?2",
                &[
                    SqlParam::Text(created_at.to_string()),
                    SqlParam::Int(entry_id),
                ],
            )?;
        }
        Ok(())
    }
}
