pub mod agent_cli;
pub mod crypto_store;
pub mod database;
pub mod jev_client;
pub mod rss_service;
pub mod skills_provisioner;
pub mod sqlite_conn;

use crate::crypto_store::{CryptoError, CryptoStore};
use crate::database::{Database, DEFAULT_ARCHIVE_RETENTION_DAYS};
use crate::jev_client::{
    evaluate_entry_queries_with_provider, fetch_provider_models, get_provider_spec,
    list_providers_json, resolve_active_provider_config, test_single_query_with_provider,
};
use crate::rss_service::fetch_and_parse_feed;
use crate::skills_provisioner::WorkspaceManager;
use axum::{
    body::Bytes,
    extract::{Request, State},
    http::{header, Method, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::any,
    Router,
};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tauri::Emitter;

const EMBEDDED_INDEX_HTML: &str = include_str!("../../src/index.html");
const EMBEDDED_STYLES_CSS: &str = include_str!("../../src/styles.css");
const EMBEDDED_APP_JS: &str = include_str!("../../src/app.js");

#[derive(Clone)]
pub struct AppState {
    pub db: Database,
    pub crypto: CryptoStore,
    pub workspace: WorkspaceManager,
}

impl AppState {
    pub fn new(cli_data_dir: Option<PathBuf>, seed_defaults: bool) -> Result<Self, String> {
        let workspace = match cli_data_dir {
            Some(d) => WorkspaceManager::new_for_dir(d),
            None => WorkspaceManager::new(None),
        };
        let db = Database::new(workspace.get_db_path(), seed_defaults)?;
        let crypto = CryptoStore::new(workspace.get_secrets_path());
        Ok(Self {
            db,
            crypto,
            workspace,
        })
    }
}

fn parse_query_params(query_str: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for pair in query_str.split('&') {
        if pair.is_empty() {
            continue;
        }
        let mut parts = pair.splitn(2, '=');
        let k = parts.next().unwrap_or("");
        let v = parts.next().unwrap_or("");
        map.insert(url_decode(k), url_decode(v));
    }
    map
}

fn url_decode(input: &str) -> String {
    let mut bytes = Vec::with_capacity(input.len());
    let raw = input.as_bytes();
    let mut i = 0;
    while i < raw.len() {
        if raw[i] == b'+' {
            bytes.push(b' ');
            i += 1;
        } else if raw[i] == b'%' && i + 2 < raw.len() {
            if let Ok(hex_str) = std::str::from_utf8(&raw[i + 1..i + 3]) {
                if let Ok(val) = u8::from_str_radix(hex_str, 16) {
                    bytes.push(val);
                    i += 3;
                    continue;
                }
            }
            bytes.push(raw[i]);
            i += 1;
        } else {
            bytes.push(raw[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&bytes).to_string()
}

fn json_as_bool(v: &Value) -> Option<bool> {
    if let Some(b) = v.as_bool() {
        Some(b)
    } else if let Some(n) = v.as_i64() {
        Some(n != 0)
    } else if let Some(s) = v.as_str() {
        match s.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Some(true),
            "0" | "false" | "no" | "off" => Some(false),
            _ => None,
        }
    } else {
        None
    }
}

fn build_auth_status_json(state: &AppState) -> Value {
    let settings = state.db.get_settings().unwrap_or_default();
    let active_prov = resolve_active_provider_config(&settings);
    let masked_for_active = state
        .crypto
        .get_masked_key_for_provider(&active_prov.provider_id);
    let is_unlocked_for_active = if active_prov.requires_key {
        masked_for_active.is_some()
    } else {
        true
    };
    json!({
        "has_saved_key": state.crypto.has_saved_key(),
        "is_unlocked": is_unlocked_for_active,
        "any_unlocked": state.crypto.is_unlocked(),
        "masked_key": masked_for_active,
        "unlocked_providers": state.crypto.list_unlocked_providers(),
        "active_provider_id": active_prov.provider_id,
        "active_provider_name": active_prov.provider_name,
        "requires_key": active_prov.requires_key,
        "portal_url": active_prov.portal_url,
        "active_provider": active_prov,
    })
}

pub async fn dispatch_api_request(
    state: &AppState,
    method: &str,
    full_path: &str,
    body: Option<Value>,
) -> Result<Value, (u16, String)> {
    let method_upper = method.trim().to_uppercase();
    let (path, query_str) = match full_path.split_once('?') {
        Some((p, q)) => (p, q),
        None => (full_path, ""),
    };
    let query_params = parse_query_params(query_str);
    let body_obj = body.unwrap_or_else(|| json!({}));

    // --- Auth / Encryption Endpoints ---
    if path == "/api/auth/status" && method_upper == "GET" {
        return Ok(build_auth_status_json(state));
    }

    if path == "/api/auth/setup" && method_upper == "POST" {
        let api_key = body_obj
            .get("api_key")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let password = body_obj
            .get("password")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let provider_id = body_obj
            .get("provider_id")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        if let Some(ref pid) = provider_id {
            let pid_lower = pid.to_lowercase();
            let current_settings = state.db.get_settings().unwrap_or_default();
            let mut updates = HashMap::new();
            updates.insert("ai_provider".to_string(), pid_lower.clone());
            if let Some(m) = body_obj.get("jev_model").and_then(|v| v.as_str()) {
                if !m.trim().is_empty() {
                    updates.insert("jev_model".to_string(), m.trim().to_string());
                }
            } else if pid_lower != "openrouter" {
                let existing_model = current_settings
                    .get("jev_model")
                    .map(|s| s.as_str())
                    .unwrap_or("");
                if existing_model.is_empty() || existing_model.starts_with("typesafe/jev") {
                    let spec = get_provider_spec(&pid_lower);
                    updates.insert("jev_model".to_string(), spec.default_model.to_string());
                }
            }
            if let Some(bu) = body_obj.get("custom_base_url").and_then(|v| v.as_str()) {
                updates.insert("custom_base_url".to_string(), bu.trim().to_string());
            }
            if let Some(pu) = body_obj.get("custom_portal_url").and_then(|v| v.as_str()) {
                updates.insert("custom_portal_url".to_string(), pu.trim().to_string());
            }
            if let Some(cn) = body_obj.get("custom_provider_name").and_then(|v| v.as_str()) {
                updates.insert("custom_provider_name".to_string(), cn.trim().to_string());
            }
            let _ = state.db.update_settings(&updates);
        }

        let settings = state.db.get_settings().unwrap_or_default();
        let active_prov = resolve_active_provider_config(&settings);
        let eff_pid = provider_id
            .as_deref()
            .unwrap_or(&active_prov.provider_id);

        if !api_key.trim().is_empty() || active_prov.requires_key {
            state
                .crypto
                .save_api_key_for_provider(Some(eff_pid), api_key, password)
                .map_err(|e| (400, e.to_string()))?;
        }
        return Ok(build_auth_status_json(state));
    }

    if path == "/api/auth/unlock" && method_upper == "POST" {
        let password = body_obj
            .get("password")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        match state.crypto.unlock_api_key(password) {
            Ok(_) => return Ok(build_auth_status_json(state)),
            Err(CryptoError::NotFound(m)) => return Err((404, m)),
            Err(CryptoError::DecryptionError(m)) => return Err((401, m)),
            Err(e) => return Err((400, e.to_string())),
        }
    }

    if path == "/api/auth/lock" && method_upper == "POST" {
        state.crypto.lock();
        return Ok(build_auth_status_json(state));
    }

    if path == "/api/auth/key" && method_upper == "DELETE" {
        state.crypto.delete_saved_key();
        return Ok(build_auth_status_json(state));
    }

    // --- Workspace / AI Skills Folder Endpoints ---
    if path == "/api/workspace" && method_upper == "GET" {
        return Ok(state.workspace.get_status());
    }

    if path == "/api/workspace" && method_upper == "PUT" {
        let data_dir = body_obj
            .get("data_dir")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let new_dir = state
            .workspace
            .set_data_dir(data_dir)
            .map_err(|e| (400, e))?;
        state
            .db
            .set_db_path(new_dir.join("smart_rss.db"), true)
            .map_err(|e| (400, e))?;
        state.crypto.set_secret_path(new_dir.join("secrets.enc"));
        return Ok(state.workspace.get_status());
    }

    if path == "/api/workspace/resync" && method_upper == "POST" {
        return state
            .workspace
            .provision_workspace()
            .map_err(|e| (400, e));
    }

    if path == "/api/workspace/open" && method_upper == "POST" {
        state
            .workspace
            .open_in_file_manager()
            .map_err(|e| (400, e))?;
        return Ok(json!({ "opened": true }));
    }

    if path == "/api/open-url" && method_upper == "POST" {
        let url = body_obj
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if url.is_empty() || (!url.starts_with("http://") && !url.starts_with("https://")) {
            return Err((400, "A valid http(s) URL is required.".to_string()));
        }
        open::that(url).map_err(|e| (400, format!("Failed to open URL: {}", e)))?;
        return Ok(json!({ "opened": true }));
    }

    // --- Providers & Settings Endpoints ---
    if path == "/api/providers" && method_upper == "GET" {
        let settings = state.db.get_settings().map_err(|e| (400, e))?;
        let active_provider = resolve_active_provider_config(&settings);
        return Ok(json!({
            "providers": list_providers_json(),
            "active_provider": active_provider,
            "settings": settings,
        }));
    }

    if path == "/api/providers/models" && method_upper == "POST" {
        let mut settings = state.db.get_settings().map_err(|e| (400, e))?;
        if let Some(pid) = body_obj.get("provider_id").and_then(|v| v.as_str()) {
            if !pid.trim().is_empty() {
                settings.insert("ai_provider".to_string(), pid.trim().to_lowercase());
            }
        }
        if let Some(bu) = body_obj.get("custom_base_url").and_then(|v| v.as_str()) {
            settings.insert("custom_base_url".to_string(), bu.trim().to_string());
        }
        let cfg = resolve_active_provider_config(&settings);
        let api_key = state.crypto.get_api_key_for_provider(&cfg.provider_id);
        let models = fetch_provider_models(&cfg, api_key.as_deref())
            .await
            .map_err(|e| (400, e))?;
        return Ok(json!({
            "provider_id": cfg.provider_id,
            "models": models,
        }));
    }

    if path == "/api/settings" && method_upper == "GET" {
        let pruned_archived = state
            .db
            .prune_old_archived_entries(DEFAULT_ARCHIVE_RETENTION_DAYS, None)
            .unwrap_or(0);
        let settings = state.db.get_settings().map_err(|e| (400, e))?;
        let counts = state.db.get_counts().map_err(|e| (400, e))?;
        let active_provider = resolve_active_provider_config(&settings);
        return Ok(json!({
            "settings": settings,
            "counts": counts,
            "pruned_archived": pruned_archived,
            "providers": list_providers_json(),
            "active_provider": active_provider,
        }));
    }

    if path == "/api/settings" && method_upper == "PUT" {
        let mut updates = HashMap::new();
        if let Some(p) = body_obj.get("ai_provider").and_then(|v| v.as_str()) {
            if !p.trim().is_empty() {
                let spec = get_provider_spec(p);
                updates.insert("ai_provider".to_string(), spec.id.to_string());
            }
        }
        if let Some(m) = body_obj.get("jev_model").and_then(|v| v.as_str()) {
            if !m.trim().is_empty() {
                updates.insert("jev_model".to_string(), m.trim().to_string());
            }
        }
        if let Some(cn) = body_obj.get("custom_provider_name").and_then(|v| v.as_str()) {
            updates.insert("custom_provider_name".to_string(), cn.trim().to_string());
        }
        if let Some(bu) = body_obj.get("custom_base_url").and_then(|v| v.as_str()) {
            updates.insert("custom_base_url".to_string(), bu.trim().to_string());
        }
        if let Some(pu) = body_obj.get("custom_portal_url").and_then(|v| v.as_str()) {
            updates.insert("custom_portal_url".to_string(), pu.trim().to_string());
        }
        if let Some(max_e) = body_obj.get("max_entries_per_feed") {
            let parsed = max_e
                .as_i64()
                .or_else(|| max_e.as_str().and_then(|s| s.parse::<i64>().ok()))
                .unwrap_or(25)
                .clamp(1, 200);
            updates.insert("max_entries_per_feed".to_string(), parsed.to_string());
        }
        let settings = state.db.update_settings(&updates).map_err(|e| (400, e))?;
        let counts = state.db.get_counts().map_err(|e| (400, e))?;
        let active_provider = resolve_active_provider_config(&settings);
        return Ok(json!({
            "settings": settings,
            "counts": counts,
            "providers": list_providers_json(),
            "active_provider": active_provider,
        }));
    }

    // --- Feeds Endpoints ---
    if path == "/api/feeds" && method_upper == "GET" {
        let feeds = state.db.list_feeds().map_err(|e| (400, e))?;
        return Ok(json!({ "feeds": feeds }));
    }

    if path == "/api/feeds" && method_upper == "POST" {
        let title = body_obj
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let url = body_obj
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let category = body_obj
            .get("category")
            .and_then(|v| v.as_str())
            .unwrap_or("General");
        let enabled = body_obj
            .get("enabled")
            .and_then(json_as_bool)
            .unwrap_or(true);

        if title.trim().is_empty() || url.trim().len() < 4 {
            return Err((400, "Could not create feed: title and valid url are required.".to_string()));
        }
        let feed = state
            .db
            .create_feed(title, url, category, enabled)
            .map_err(|e| (400, format!("Could not create feed: {}", e)))?;
        return Ok(json!({ "feed": feed }));
    }

    if path == "/api/feeds/test" && method_upper == "POST" {
        let mut target_url = body_obj
            .get("url")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let feed_id = body_obj.get("feed_id").and_then(|v| v.as_i64());
        let max_entries = body_obj
            .get("max_entries")
            .and_then(|v| v.as_u64())
            .unwrap_or(20) as usize;
        let mut feed_title_override: Option<String> = None;

        if let Some(fid) = feed_id {
            if target_url.as_deref().unwrap_or("").trim().is_empty() {
                let feed_obj = state
                    .db
                    .get_feed(fid)
                    .map_err(|e| (400, e))?
                    .ok_or_else(|| (404, "Feed not found.".to_string()))?;
                target_url = feed_obj
                    .get("url")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                feed_title_override = feed_obj
                    .get("title")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
            }
        }

        let url_str = target_url.unwrap_or_default();
        if url_str.trim().is_empty() {
            return Err((400, "A feed URL or feed_id is required.".to_string()));
        }

        let mut parsed = fetch_and_parse_feed_with_context(
            &url_str,
            feed_id,
            feed_title_override.as_deref(),
            max_entries,
        )
        .await
        .map_err(|e| (400, format!("RSS fetch/parse failed: {}", e)))?;

        let known_uids = state.db.get_known_entry_uids().map_err(|e| (400, e))?;
        if let Some(entries) = parsed.get_mut("entries").and_then(|v| v.as_array_mut()) {
            for entry in entries {
                let uid = entry
                    .get("entry_uid")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let seen = known_uids.contains(uid);
                if let Some(obj) = entry.as_object_mut() {
                    obj.insert("already_seen".to_string(), Value::Bool(seen));
                }
            }
        }
        return Ok(parsed);
    }

    if path == "/api/feeds/refresh" && method_upper == "POST" {
        let req_feed_id = body_obj.get("feed_id").and_then(|v| v.as_i64());
        let evaluate_unscored_active = body_obj
            .get("evaluate_unscored_active")
            .and_then(json_as_bool)
            .unwrap_or(false);

        let all_feeds = state.db.list_feeds().map_err(|e| (400, e))?;
        let target_feeds: Vec<Value> = if let Some(fid) = req_feed_id {
            all_feeds
                .into_iter()
                .filter(|f| f.get("id").and_then(|v| v.as_i64()) == Some(fid))
                .collect()
        } else {
            all_feeds
                .into_iter()
                .filter(|f| f.get("enabled").and_then(json_as_bool).unwrap_or(false))
                .collect()
        };

        if target_feeds.is_empty() && !evaluate_unscored_active {
            return Err((400, "No enabled RSS feeds to update.".to_string()));
        }

        let settings = state.db.get_settings().map_err(|e| (400, e))?;
        let max_per_feed = settings
            .get("max_entries_per_feed")
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(25);
        let provider_cfg = resolve_active_provider_config(&settings);

        let mut feed_errors: Vec<Value> = Vec::new();
        let mut all_parsed_entries: Vec<Value> = Vec::new();

        for f in &target_feeds {
            let fid = f.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
            let furl = f.get("url").and_then(|v| v.as_str()).unwrap_or("");
            let ftitle = f.get("title").and_then(|v| v.as_str()).unwrap_or("");
            match fetch_and_parse_feed(
                furl,
                Some(fid),
                Some(ftitle),
                max_per_feed,
            )
            .await
            {
                Ok(res) => {
                    let _ = state.db.mark_feed_fetched(fid, None);
                    if let Some(arr) = res.get("entries").and_then(|v| v.as_array()) {
                        all_parsed_entries.extend(arr.iter().cloned());
                    }
                }
                Err(err_msg) => {
                    let _ = state.db.mark_feed_fetched(fid, Some(&err_msg));
                    feed_errors.push(json!({
                        "feed_id": fid,
                        "title": ftitle,
                        "error": err_msg,
                    }));
                }
            }
        }

        let total_parsed = all_parsed_entries.len();
        let newly_inserted = state
            .db
            .insert_new_entries(&all_parsed_entries)
            .map_err(|e| (400, e))?;
        let skipped_known = total_parsed.saturating_sub(newly_inserted.len());

        let enabled_queries = state.db.list_queries(true).map_err(|e| (400, e))?;
        let api_key = state
            .crypto
            .get_api_key_for_provider(&provider_cfg.provider_id);

        let mut entries_to_evaluate: Vec<(Value, Vec<Value>)> = Vec::new();
        for entry in &newly_inserted {
            if !enabled_queries.is_empty() {
                entries_to_evaluate.push((entry.clone(), enabled_queries.clone()));
            }
        }

        if evaluate_unscored_active && !enabled_queries.is_empty() {
            let new_ids: std::collections::HashSet<i64> = newly_inserted
                .iter()
                .filter_map(|e| e.get("id").and_then(|v| v.as_i64()))
                .collect();
            let q_map: HashMap<i64, Value> = enabled_queries
                .iter()
                .filter_map(|q| {
                    q.get("id")
                        .and_then(|v| v.as_i64())
                        .map(|id| (id, q.clone()))
                })
                .collect();
            let q_ids: Vec<i64> = q_map.keys().copied().collect();
            let unscored_active = state
                .db
                .get_unscored_active_entries(&q_ids)
                .map_err(|e| (400, e))?;
            for entry in unscored_active {
                let eid = entry.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
                if new_ids.contains(&eid) {
                    continue;
                }
                let missing_qs: Vec<Value> = entry
                    .get("missing_query_ids")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|x| x.as_i64().and_then(|qid| q_map.get(&qid).cloned()))
                            .collect()
                    })
                    .unwrap_or_default();
                if !missing_qs.is_empty() {
                    entries_to_evaluate.push((entry, missing_qs));
                }
            }
        }

        let mut evaluated_count = 0usize;
        let mut evaluation_errors: Vec<Value> = Vec::new();

        if !entries_to_evaluate.is_empty() && !enabled_queries.is_empty() {
            let effective_key = if let Some(ref k) = api_key {
                Some(k.clone())
            } else if !provider_cfg.requires_key {
                Some(String::new())
            } else {
                None
            };

            if let Some(ref key_str) = effective_key {
                for (entry_obj, qs) in entries_to_evaluate {
                    let eid = entry_obj.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
                    let etitle = entry_obj
                        .get("title")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    match evaluate_entry_queries_with_provider(
                        &entry_obj,
                        &qs,
                        key_str,
                        &provider_cfg,
                    )
                    .await
                    {
                        Ok(scores) => {
                            if let Err(e) = state.db.save_entry_scores(eid, &scores) {
                                evaluation_errors.push(json!({
                                    "entry_id": eid,
                                    "title": etitle,
                                    "error": e,
                                }));
                            } else {
                                evaluated_count += 1;
                            }
                        }
                        Err(e) => {
                            evaluation_errors.push(json!({
                                "entry_id": eid,
                                "title": etitle,
                                "error": e,
                            }));
                        }
                    }
                }
            } else {
                evaluation_errors.push(json!({
                    "entry_id": Value::Null,
                    "title": "API Key Locked",
                    "error": format!(
                        "Fetched {} new entries, but the {} API key is locked. Unlock your API key and click 'Score Unscored Active Entries' to rank them.",
                        newly_inserted.len(),
                        provider_cfg.provider_name
                    ),
                }));
            }
        }

        let counts = state.db.get_counts().map_err(|e| (400, e))?;
        return Ok(json!({
            "feeds_fetched": target_feeds.len(),
            "feed_errors": feed_errors,
            "total_parsed": total_parsed,
            "skipped_known": skipped_known,
            "newly_inserted": newly_inserted.len(),
            "entries_evaluated": evaluated_count,
            "evaluation_errors": evaluation_errors,
            "counts": counts,
        }));
    }

    if let Some(rest) = path.strip_prefix("/api/feeds/") {
        if let Ok(feed_id) = rest.parse::<i64>() {
            if method_upper == "PUT" {
                let title = body_obj.get("title").and_then(|v| v.as_str());
                let url = body_obj.get("url").and_then(|v| v.as_str());
                let category = body_obj.get("category").and_then(|v| v.as_str());
                let enabled = body_obj.get("enabled").and_then(json_as_bool);
                let updated = state
                    .db
                    .update_feed(feed_id, title, url, category, enabled)
                    .map_err(|e| (400, format!("Could not update feed: {}", e)))?;
                match updated {
                    Some(f) => return Ok(json!({ "feed": f })),
                    None => return Err((404, "Feed not found.".to_string())),
                }
            } else if method_upper == "DELETE" {
                let ok = state.db.delete_feed(feed_id).map_err(|e| (400, e))?;
                if !ok {
                    return Err((404, "Feed not found.".to_string()));
                }
                return Ok(json!({ "deleted": true }));
            }
        }
    }

    // --- Jev Queries Endpoints ---
    if path == "/api/queries" && method_upper == "GET" {
        let queries = state.db.list_queries(false).map_err(|e| (400, e))?;
        return Ok(json!({ "queries": queries }));
    }

    if path == "/api/queries" && method_upper == "POST" {
        let name = body_obj
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let query_type = body_obj
            .get("query_type")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let instructions = body_obj
            .get("instructions")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let criteria = body_obj
            .get("criteria")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let weight = body_obj
            .get("weight")
            .and_then(|v| v.as_f64())
            .unwrap_or(1.0);
        let enabled = body_obj
            .get("enabled")
            .and_then(json_as_bool)
            .unwrap_or(true);

        let query = state
            .db
            .create_query(name, query_type, instructions, &criteria, weight, enabled)
            .map_err(|e| (400, e))?;
        return Ok(json!({ "query": query }));
    }

    if path == "/api/queries/test" && method_upper == "POST" {
        let settings = state.db.get_settings().map_err(|e| (400, e))?;
        let provider_cfg = resolve_active_provider_config(&settings);
        let api_key_opt = state
            .crypto
            .get_api_key_for_provider(&provider_cfg.provider_id);

        let api_key = if let Some(k) = api_key_opt {
            k
        } else if !provider_cfg.requires_key {
            String::new()
        } else {
            return Err((
                401,
                format!(
                    "{} API key is locked or not configured. Please unlock it first.",
                    provider_cfg.provider_name
                ),
            ));
        };

        let entry_payload = body_obj
            .get("entry")
            .cloned()
            .ok_or_else(|| (400, "Missing entry payload.".to_string()))?;

        let query_obj = if let Some(qid) = body_obj.get("query_id").and_then(|v| v.as_i64()) {
            state
                .db
                .get_query(qid)
                .map_err(|e| (400, e))?
                .ok_or_else(|| (404, "Selected Jev query not found.".to_string()))?
        } else if let Some(cq) = body_obj.get("custom_query") {
            if cq.is_object() {
                cq.clone()
            } else {
                return Err((400, "Provide either query_id or custom_query.".to_string()));
            }
        } else {
            return Err((400, "Provide either query_id or custom_query.".to_string()));
        };

        let result =
            test_single_query_with_provider(&entry_payload, &query_obj, &api_key, &provider_cfg)
                .await
                .map_err(|e| (400, e))?;
        return Ok(result);
    }

    if let Some(rest) = path.strip_prefix("/api/queries/") {
        if let Ok(query_id) = rest.parse::<i64>() {
            if method_upper == "PUT" {
                let name = body_obj.get("name").and_then(|v| v.as_str());
                let query_type = body_obj.get("query_type").and_then(|v| v.as_str());
                let instructions = body_obj.get("instructions").and_then(|v| v.as_str());
                let criteria = body_obj.get("criteria");
                let weight = body_obj.get("weight").and_then(|v| v.as_f64());
                let enabled = body_obj.get("enabled").and_then(json_as_bool);

                let updated = state
                    .db
                    .update_query(
                        query_id,
                        name,
                        query_type,
                        instructions,
                        criteria,
                        weight,
                        enabled,
                    )
                    .map_err(|e| (400, e))?;
                match updated {
                    Some(q) => return Ok(json!({ "query": q })),
                    None => return Err((404, "Query not found.".to_string())),
                }
            } else if method_upper == "DELETE" {
                let ok = state.db.delete_query(query_id).map_err(|e| (400, e))?;
                if !ok {
                    return Err((404, "Query not found.".to_string()));
                }
                return Ok(json!({ "deleted": true }));
            }
        }
    }

    // --- Entries Endpoints ---
    if path == "/api/entries" && method_upper == "GET" {
        let status = query_params
            .get("status")
            .map(|s| s.as_str())
            .unwrap_or("active");
        if status != "active" && status != "archived" {
            return Err((400, "Invalid status parameter.".to_string()));
        }
        if status == "archived" {
            let _ = state
                .db
                .prune_old_archived_entries(DEFAULT_ARCHIVE_RETENTION_DAYS, None);
        }
        let feed_ids: Option<Vec<i64>> = if let Some(fids_raw) = query_params.get("feed_ids") {
            let trimmed = fids_raw.trim();
            if trimmed == "none" {
                Some(Vec::new())
            } else if trimmed.is_empty() {
                None
            } else {
                let parsed: Vec<i64> = trimmed
                    .split(',')
                    .filter_map(|part| part.trim().parse::<i64>().ok())
                    .collect();
                Some(parsed)
            }
        } else {
            query_params
                .get("feed_id")
                .and_then(|s| s.parse::<i64>().ok())
                .map(|fid| vec![fid])
        };
        let search = query_params
            .get("search")
            .map(|s| s.as_str())
            .filter(|s| !s.is_empty());

        let entries = state
            .db
            .list_entries_filtered(status, feed_ids.as_deref(), search)
            .map_err(|e| (400, e))?;
        let counts = state.db.get_counts().map_err(|e| (400, e))?;
        let feed_stats = state
            .db
            .get_feed_relevance_stats(status)
            .map_err(|e| (400, e))?;
        return Ok(json!({
            "entries": entries,
            "counts": counts,
            "feed_stats": feed_stats,
        }));
    }

    if path == "/api/entries/archive-bulk" && method_upper == "POST" {
        let entry_ids: Vec<i64> = body_obj
            .get("entry_ids")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|x| x.as_i64()).collect())
            .unwrap_or_default();
        let count = state
            .db
            .archive_entries_bulk(&entry_ids)
            .map_err(|e| (400, e))?;
        let counts = state.db.get_counts().map_err(|e| (400, e))?;
        return Ok(json!({
            "archived_count": count,
            "counts": counts,
        }));
    }

    if path == "/api/entries/reset-all" && method_upper == "POST" {
        let deleted_count = state.db.reset_all_entries().map_err(|e| (400, e))?;
        let counts = state.db.get_counts().map_err(|e| (400, e))?;
        return Ok(json!({
            "deleted_count": deleted_count,
            "counts": counts,
        }));
    }

    if let Some(rest) = path.strip_prefix("/api/entries/") {
        if let Some(id_str) = rest.strip_suffix("/archive") {
            if method_upper == "POST" {
                if let Ok(entry_id) = id_str.parse::<i64>() {
                    let updated = state
                        .db
                        .set_entry_status(entry_id, "archived")
                        .map_err(|e| (400, e))?;
                    match updated {
                        Some(entry) => {
                            let counts = state.db.get_counts().map_err(|e| (400, e))?;
                            return Ok(json!({ "entry": entry, "counts": counts }));
                        }
                        None => return Err((404, "Entry not found.".to_string())),
                    }
                }
            }
        } else if let Some(id_str) = rest.strip_suffix("/unarchive") {
            if method_upper == "POST" {
                if let Ok(entry_id) = id_str.parse::<i64>() {
                    let updated = state
                        .db
                        .set_entry_status(entry_id, "active")
                        .map_err(|e| (400, e))?;
                    match updated {
                        Some(entry) => {
                            let counts = state.db.get_counts().map_err(|e| (400, e))?;
                            return Ok(json!({ "entry": entry, "counts": counts }));
                        }
                        None => return Err((404, "Entry not found.".to_string())),
                    }
                }
            }
        }
    }

    // --- Agent-Friendly Idempotent Endpoints ---
    if path == "/api/agent/feeds" && method_upper == "POST" {
        let url = body_obj
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if url.len() < 4 {
            return Err((400, "Feed URL is required.".to_string()));
        }
        let mut resolved_title = body_obj
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let category = body_obj.get("category").and_then(|v| v.as_str());
        let enabled = body_obj.get("enabled").and_then(json_as_bool);
        let validate_first = body_obj
            .get("validate_first")
            .and_then(json_as_bool)
            .unwrap_or(false);

        let mut preview_info: Option<Value> = None;
        if validate_first || resolved_title.is_empty() {
            match fetch_and_parse_feed(&url, None, None, 5).await {
                Ok(p) => {
                    if resolved_title.is_empty() {
                        resolved_title = p
                            .get("feed_title")
                            .and_then(|v| v.as_str())
                            .filter(|s| !s.trim().is_empty())
                            .unwrap_or(&url)
                            .to_string();
                    }
                    preview_info = Some(p);
                }
                Err(e) => {
                    if validate_first {
                        return Err((400, format!("RSS validation failed: {}", e)));
                    }
                    resolved_title = url.clone();
                }
            }
        }

        let feed = state
            .db
            .upsert_feed(&resolved_title, &url, category, enabled)
            .map_err(|e| (400, e))?;
        let counts = state.db.get_counts().map_err(|e| (400, e))?;
        let mut out = json!({
            "feed": feed,
            "counts": counts,
        });
        if let Some(p) = preview_info {
            out["validation"] = json!({
                "detected_title": p.get("feed_title").cloned().unwrap_or(Value::Null),
                "entry_count": p.get("entry_count").cloned().unwrap_or(Value::Null),
            });
        }
        return Ok(out);
    }

    if path == "/api/agent/queries" && method_upper == "POST" {
        let query_id = body_obj.get("id").and_then(|v| v.as_i64());
        let name = body_obj.get("name").and_then(|v| v.as_str());
        let query_type = body_obj.get("query_type").and_then(|v| v.as_str());
        let instructions = body_obj.get("instructions").and_then(|v| v.as_str());
        let criteria = body_obj.get("criteria");
        let weight = body_obj.get("weight").and_then(|v| v.as_f64());
        let enabled = body_obj.get("enabled").and_then(json_as_bool);

        let query = state
            .db
            .upsert_query(
                name,
                query_type,
                instructions,
                criteria,
                weight,
                enabled,
                query_id,
            )
            .map_err(|e| (400, e))?;
        let top_active = state
            .db
            .list_entries("active", None, None)
            .map_err(|e| (400, e))?;
        let preview: Vec<Value> = top_active
            .into_iter()
            .take(5)
            .map(|e| {
                json!({
                    "id": e.get("id").cloned().unwrap_or(Value::Null),
                    "title": e.get("title").cloned().unwrap_or(Value::Null),
                    "weighted_score": e.get("weighted_score").cloned().unwrap_or(Value::Null),
                    "relevance_pct": e.get("relevance_pct").cloned().unwrap_or(Value::Null),
                })
            })
            .collect();
        return Ok(json!({
            "query": query,
            "top_active_entries_preview": preview,
        }));
    }

    Err((404, format!("Endpoint not found: {} {}", method_upper, path)))
}

async fn fetch_and_parse_feed_with_context(
    url: &str,
    feed_id: Option<i64>,
    feed_title_override: Option<&str>,
    max_entries: usize,
) -> Result<Value, String> {
    fetch_and_parse_feed(
        url,
        feed_id,
        feed_title_override,
        max_entries,
    )
    .await
}

#[tauri::command]
async fn api_dispatch(
    state: tauri::State<'_, AppState>,
    method: String,
    path: String,
    body: Option<Value>,
) -> Result<Value, String> {
    match dispatch_api_request(&state, &method, &path, body).await {
        Ok(val) => Ok(val),
        Err((_code, detail)) => Err(detail),
    }
}

#[derive(Clone)]
pub struct HttpServerState {
    pub app_state: AppState,
    pub app_handle: Option<tauri::AppHandle>,
}

pub fn build_axum_router(
    app_state: AppState,
    app_handle: Option<tauri::AppHandle>,
) -> Router {
    let shared = Arc::new(HttpServerState {
        app_state,
        app_handle,
    });
    Router::new()
        .route("/", any(handle_index))
        .route("/static/{*file}", any(handle_static))
        .route("/api/{*rest}", any(handle_api))
        .with_state(shared)
}

async fn handle_index(State(st): State<Arc<HttpServerState>>) -> impl IntoResponse {
    let _ = st
        .app_state
        .db
        .prune_old_archived_entries(DEFAULT_ARCHIVE_RETENTION_DAYS, None);
    Html(EMBEDDED_INDEX_HTML)
}

async fn handle_static(req: Request) -> Response {
    let path = req.uri().path();
    if path.ends_with("styles.css") {
        (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
            EMBEDDED_STYLES_CSS,
        )
            .into_response()
    } else if path.ends_with("app.js") {
        (
            StatusCode::OK,
            [(
                header::CONTENT_TYPE,
                "application/javascript; charset=utf-8",
            )],
            EMBEDDED_APP_JS,
        )
            .into_response()
    } else {
        (StatusCode::NOT_FOUND, "Not found").into_response()
    }
}

async fn handle_api(
    State(st): State<Arc<HttpServerState>>,
    method: Method,
    req: Request,
) -> Response {
    let uri = req.uri().clone();
    let full_path = match uri.query() {
        Some(q) => format!("{}?{}", uri.path(), q),
        None => uri.path().to_string(),
    };
    let body_bytes: Bytes = axum::body::to_bytes(req.into_body(), 4 * 1024 * 1024)
        .await
        .unwrap_or_default();
    let body_val: Option<Value> = if body_bytes.is_empty() {
        None
    } else {
        serde_json::from_slice(&body_bytes).ok()
    };

    match dispatch_api_request(&st.app_state, method.as_str(), &full_path, body_val).await {
        Ok(val) => {
            if method != Method::GET {
                if let Some(ref handle) = st.app_handle {
                    let _ = handle.emit("db-updated", json!({ "source": "http" }));
                }
            }
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/json")],
                serde_json::to_string(&val).unwrap_or_else(|_| "{}".to_string()),
            )
                .into_response()
        }
        Err((code, detail)) => {
            let status = StatusCode::from_u16(code).unwrap_or(StatusCode::BAD_REQUEST);
            (
                status,
                [(header::CONTENT_TYPE, "application/json")],
                serde_json::to_string(&json!({ "detail": detail }))
                    .unwrap_or_else(|_| "{}".to_string()),
            )
                .into_response()
        }
    }
}

fn db_files_signature(db_path: &std::path::Path) -> Option<(SystemTime, u64)> {
    let wal_path = PathBuf::from(format!("{}-wal", db_path.display()));
    let mut latest_mtime = SystemTime::UNIX_EPOCH;
    let mut total_len = 0u64;
    let mut found = false;

    for p in [db_path, wal_path.as_path()] {
        if let Ok(meta) = std::fs::metadata(p) {
            found = true;
            total_len = total_len.wrapping_add(meta.len());
            if let Ok(m) = meta.modified() {
                if m > latest_mtime {
                    latest_mtime = m;
                }
            }
        }
    }
    if found {
        Some((latest_mtime, total_len))
    } else {
        None
    }
}

pub fn run_tauri_app(cli_data_dir: Option<PathBuf>) {
    let app_state = AppState::new(cli_data_dir, true).expect("Failed to initialize AppState");

    tauri::Builder::default()
        .manage(app_state.clone())
        .invoke_handler(tauri::generate_handler![api_dispatch])
        .setup(move |app| {
            let handle = app.handle().clone();
            let http_state = app_state.clone();
            let watch_state = app_state.clone();
            let watch_handle = handle.clone();

            // 1. Start loopback HTTP server on 127.0.0.1:8765 so agent HTTP calls & browser access work
            tauri::async_runtime::spawn(async move {
                let router = build_axum_router(http_state, Some(handle));
                if let Ok(listener) = tokio::net::TcpListener::bind("127.0.0.1:8765").await {
                    let _ = axum::serve(listener, router).await;
                }
            });

            // 2. Watch SQLite DB + WAL modification timestamps so CLI skills (`./smart-rss ...`) update the UI live
            tauri::async_runtime::spawn(async move {
                let mut last_sig = db_files_signature(&watch_state.db.get_db_path());
                loop {
                    tokio::time::sleep(Duration::from_millis(1200)).await;
                    let current_sig = db_files_signature(&watch_state.db.get_db_path());
                    if current_sig != last_sig {
                        last_sig = current_sig;
                        let _ = watch_handle.emit("db-updated", json!({ "source": "fs-watcher" }));
                    }
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_cli::{execute_cli_resource, AgentAPI, CliArgs};
    use crate::jev_client::{build_question_spec, normalize_jev_answer};
    use crate::rss_service::parse_feed_content;
    use chrono::{Duration as ChronoDuration, Utc};
    use clap::Parser;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static TEST_COUNTER: AtomicUsize = AtomicUsize::new(1);

    fn create_temp_dir(prefix: &str) -> PathBuf {
        let idx = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "smart_rss_tauri_test_{}_{}_{}",
            prefix,
            std::process::id(),
            idx
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    const SAMPLE_RSS_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
  <channel>
    <title>Journal of Quantum Biology</title>
    <description>Latest research articles</description>
    <item>
      <guid>doi:10.1000/jqb.2026.001</guid>
      <title>Single-Cell Quantum Coherence in Photosynthetic Complexes</title>
      <link>https://example.org/articles/001</link>
      <author>Alice Smith, Bob Jones</author>
      <pubDate>Tue, 10 Nov 2026 08:00:00 GMT</pubDate>
      <description><![CDATA[<p>We demonstrate <b>quantum coherence</b> using novel computational modeling.</p>]]></description>
    </item>
    <item>
      <guid>doi:10.1000/jqb.2026.002</guid>
      <title>Author Correction: Editorial Guidelines</title>
      <link>https://example.org/articles/002</link>
      <author>Editorial Board</author>
      <pubDate>Tue, 10 Nov 2026 07:00:00 GMT</pubDate>
      <description>Erratum regarding formatting rules.</description>
    </item>
  </channel>
</rss>
"#;

    #[test]
    fn test_crypto_store_roundtrip_and_wrong_password() {
        let base_path = create_temp_dir("crypto");
        let secret_file = base_path.join("secrets.enc");
        let store = CryptoStore::new(&secret_file);
        assert!(!store.has_saved_key());
        assert!(!store.is_unlocked());

        store
            .save_api_key(
                "sk-or-v1-test-secret-key-12345",
                "correct-horse-battery-staple",
            )
            .unwrap();
        assert!(store.has_saved_key());
        assert!(store.is_unlocked());
        assert_eq!(
            store.get_api_key().as_deref(),
            Some("sk-or-v1-test-secret-key-12345")
        );

        // Lock clears RAM
        store.lock();
        assert!(!store.is_unlocked());
        assert!(store.get_api_key().is_none());

        // Wrong password fails
        let err = store.unlock_api_key("wrong-password");
        assert!(matches!(err, Err(CryptoError::DecryptionError(_))));
        assert!(!store.is_unlocked());

        // Right password unlocks
        let unlocked = store
            .unlock_api_key("correct-horse-battery-staple")
            .unwrap();
        assert_eq!(unlocked, "sk-or-v1-test-secret-key-12345");
        assert!(store.is_unlocked());
        let _ = std::fs::remove_dir_all(&base_path);
    }

    #[test]
    fn test_rss_parsing_and_cleaning() {
        let parsed = parse_feed_content(
            SAMPLE_RSS_XML.as_bytes(),
            "https://example.org/rss.xml",
            Some(1),
            None,
            50,
        )
        .unwrap();
        assert_eq!(parsed["feed_title"], "Journal of Quantum Biology");
        assert_eq!(parsed["entry_count"], 2);
        let first = &parsed["entries"][0];
        assert_eq!(
            first["title"],
            "Single-Cell Quantum Coherence in Photosynthetic Complexes"
        );
        assert_eq!(
            first["summary"],
            "We demonstrate quantum coherence using novel computational modeling."
        );

        // Well-formed HTML sign-in page must be rejected
        let html_signin =
            "<html><head><title>Sign In</title></head><body><h1>Please log in</h1></body></html>";
        let err = parse_feed_content(
            html_signin.as_bytes(),
            "https://example.org/login",
            None,
            None,
            50,
        );
        assert!(err.is_err());
        assert!(err.unwrap_err().contains("HTML webpage"));
    }

    #[test]
    fn test_database_memory_skips_active_and_archived_entries() {
        let base_path = create_temp_dir("db_mem");
        let db = Database::new(base_path.join("test.db"), false).unwrap();
        let feed = db
            .create_feed(
                "Quantum Bio",
                "https://example.org/rss.xml",
                "Biology",
                true,
            )
            .unwrap();
        let fid = feed["id"].as_i64().unwrap();

        let parsed = parse_feed_content(
            SAMPLE_RSS_XML.as_bytes(),
            feed["url"].as_str().unwrap(),
            Some(fid),
            feed["title"].as_str(),
            50,
        )
        .unwrap();
        let entries_arr = parsed["entries"].as_array().unwrap();

        // First insert: both entries are new
        let inserted_1 = db.insert_new_entries(entries_arr).unwrap();
        assert_eq!(inserted_1.len(), 2);

        // Archive the second entry
        let entry_2_id = inserted_1[1]["id"].as_i64().unwrap();
        db.set_entry_status(entry_2_id, "archived").unwrap();

        let counts = db.get_counts().unwrap();
        assert_eq!(counts["active_entries"], 1);
        assert_eq!(counts["archived_entries"], 1);
        assert_eq!(counts["total_remembered_entries"], 2);

        // Second insert with same feed entries + 1 brand new entry
        let mut combined = entries_arr.clone();
        combined.push(json!({
            "entry_uid": "brand_new_uid_003",
            "feed_id": fid,
            "journal_title": feed["title"],
            "title": "Brand New Paper on Protein Folding",
            "link": "https://example.org/articles/003",
            "authors": "Carol Dan",
            "published_at": "2026-11-10 09:00 UTC",
            "summary": "Deep learning for protein structures.",
        }));
        let inserted_2 = db.insert_new_entries(&combined).unwrap();
        assert_eq!(inserted_2.len(), 1);
        assert_eq!(inserted_2[0]["entry_uid"], "brand_new_uid_003");
        let _ = std::fs::remove_dir_all(&base_path);
    }

    #[test]
    fn test_jev_normalization_and_weighted_ranking_recalculation() {
        let base_path = create_temp_dir("rank");
        let db = Database::new(base_path.join("rank.db"), false).unwrap();
        let q_noul = db
            .create_query(
                "Is Original Research",
                "noul",
                "Is this original research?",
                &json!({"true": "Original study", "false": "Correction or editorial"}),
                1.0,
                true,
            )
            .unwrap();
        let q_score = db
            .create_query(
                "ML Depth",
                "score",
                "Rate machine learning depth",
                &json!(["None", "Moderate", "Deep"]),
                2.0,
                true,
            )
            .unwrap();

        let spec_noul = build_question_spec(&q_noul).unwrap();
        assert_eq!(spec_noul["type"], "noul");
        let spec_score = build_question_spec(&q_score).unwrap();
        assert_eq!(spec_score["type"], "score");
        assert_eq!(spec_score["criteria"].as_array().unwrap().len(), 3);

        let norm_noul_a = normalize_jev_answer(&q_noul, &json!({"noul": 0.90})).unwrap();
        assert!((norm_noul_a["normalized_score"].as_f64().unwrap() - 0.90).abs() < 1e-6);

        let norm_score_a = normalize_jev_answer(
            &q_score,
            &json!({"probabilities": [0.1, 0.2, 0.7], "score": 2.6, "confidence": 0.92}),
        )
        .unwrap();
        assert!((norm_score_a["normalized_score"].as_f64().unwrap() - 0.80).abs() < 1e-6);

        let entries = db
            .insert_new_entries(&[
                json!({
                    "entry_uid": "paper_a",
                    "feed_id": Value::Null,
                    "journal_title": "Nature",
                    "title": "Paper A (High ML, High Original)",
                    "link": "",
                    "authors": "",
                    "published_at": "",
                    "summary": "",
                }),
                json!({
                    "entry_uid": "paper_b",
                    "feed_id": Value::Null,
                    "journal_title": "Science",
                    "title": "Paper B (Perfect Original, Low ML)",
                    "link": "",
                    "authors": "",
                    "published_at": "",
                    "summary": "",
                }),
            ])
            .unwrap();
        let id_a = entries[0]["id"].as_i64().unwrap();
        let id_b = entries[1]["id"].as_i64().unwrap();
        let q_noul_id = q_noul["id"].as_i64().unwrap();
        let q_score_id = q_score["id"].as_i64().unwrap();

        let mut scores_a = HashMap::new();
        scores_a.insert(
            q_noul_id,
            json!({"normalized_score": 0.80, "raw_response": {"noul": 0.80}}),
        );
        scores_a.insert(
            q_score_id,
            json!({"normalized_score": 0.90, "raw_response": {"score": 2.8}}),
        );
        db.save_entry_scores(id_a, &scores_a).unwrap();

        let mut scores_b = HashMap::new();
        scores_b.insert(
            q_noul_id,
            json!({"normalized_score": 1.00, "raw_response": {"noul": 1.00}}),
        );
        scores_b.insert(
            q_score_id,
            json!({"normalized_score": 0.20, "raw_response": {"score": 1.4}}),
        );
        db.save_entry_scores(id_b, &scores_b).unwrap();

        let ranked = db.list_entries("active", None, None).unwrap();
        assert_eq!(ranked[0]["id"].as_i64().unwrap(), id_a);
        assert!((ranked[0]["weighted_score"].as_f64().unwrap() - 2.60).abs() < 1e-6);
        assert_eq!(ranked[1]["id"].as_i64().unwrap(), id_b);
        assert!((ranked[1]["weighted_score"].as_f64().unwrap() - 1.40).abs() < 1e-6);

        // Change weights dynamically
        db.update_query(q_noul_id, None, None, None, None, Some(10.0), None)
            .unwrap();
        db.update_query(q_score_id, None, None, None, None, Some(0.1), None)
            .unwrap();

        let reranked = db.list_entries("active", None, None).unwrap();
        assert_eq!(reranked[0]["id"].as_i64().unwrap(), id_b);
        assert!((reranked[0]["weighted_score"].as_f64().unwrap() - 10.02).abs() < 1e-6);
        assert_eq!(reranked[1]["id"].as_i64().unwrap(), id_a);
        assert!((reranked[1]["weighted_score"].as_f64().unwrap() - 8.09).abs() < 1e-6);

        // Negative-weight penalty query
        let q_penalty = db
            .create_query(
                "Uses fMRI or EEG (Penalty)",
                "noul",
                "Does this paper use fMRI or EEG?",
                &json!({"true": "Uses fMRI or EEG", "false": "Does not use fMRI or EEG"}),
                -1.0,
                true,
            )
            .unwrap();
        let q_pen_id = q_penalty["id"].as_i64().unwrap();
        let mut pen_map = HashMap::new();
        pen_map.insert(
            q_pen_id,
            json!({"normalized_score": 0.95, "raw_response": {"noul": 0.95}}),
        );
        db.save_entry_scores(id_b, &pen_map).unwrap();

        let penalized = db.list_entries("active", None, None).unwrap();
        assert!((penalized[0]["weighted_score"].as_f64().unwrap() - 9.07).abs() < 1e-6);
        let _ = std::fs::remove_dir_all(&base_path);
    }

    #[tokio::test]
    async fn test_api_endpoints_end_to_end() {
        let base_path = create_temp_dir("api_e2e");
        let state = AppState::new(Some(base_path.join("app_data")), true).unwrap();

        // 1. Check initial auth status
        let r_status = dispatch_api_request(&state, "GET", "/api/auth/status", None)
            .await
            .unwrap();
        assert_eq!(r_status["has_saved_key"], false);
        assert_eq!(r_status["is_unlocked"], false);

        // 2. Setup encrypted API key
        let r_setup = dispatch_api_request(
            &state,
            "POST",
            "/api/auth/setup",
            Some(json!({
                "api_key": "sk-or-v1-demo-9999",
                "password": "fedora-password"
            })),
        )
        .await
        .unwrap();
        assert_eq!(r_setup["is_unlocked"], true);

        // 3. Lock & Unlock
        dispatch_api_request(&state, "POST", "/api/auth/lock", None)
            .await
            .unwrap();
        let r_locked = dispatch_api_request(&state, "GET", "/api/auth/status", None)
            .await
            .unwrap();
        assert_eq!(r_locked["is_unlocked"], false);

        let r_bad = dispatch_api_request(
            &state,
            "POST",
            "/api/auth/unlock",
            Some(json!({"password": "wrong"})),
        )
        .await;
        assert_eq!(r_bad.unwrap_err().0, 401);

        let r_good = dispatch_api_request(
            &state,
            "POST",
            "/api/auth/unlock",
            Some(json!({"password": "fedora-password"})),
        )
        .await
        .unwrap();
        assert_eq!(r_good["is_unlocked"], true);

        // 4. Feeds & Queries seeded
        let r_feeds = dispatch_api_request(&state, "GET", "/api/feeds", None)
            .await
            .unwrap();
        assert!(r_feeds["feeds"].as_array().unwrap().len() >= 3);

        let r_queries = dispatch_api_request(&state, "GET", "/api/queries", None)
            .await
            .unwrap();
        assert!(r_queries["queries"].as_array().unwrap().len() >= 2);

        // 5. Test HTTP Agent API endpoints (/api/agent/feeds & /api/agent/queries)
        let r_agent_feed = dispatch_api_request(
            &state,
            "POST",
            "/api/agent/feeds",
            Some(json!({
                "url": "https://www.nature.com/neuro.rss",
                "title": "Nature Neuroscience",
                "category": "Neuroscience",
                "enabled": true,
                "validate_first": false
            })),
        )
        .await
        .unwrap();
        assert_eq!(r_agent_feed["feed"]["title"], "Nature Neuroscience");

        let r_agent_query = dispatch_api_request(
            &state,
            "POST",
            "/api/agent/queries",
            Some(json!({
                "name": "Uses fMRI or EEG (Penalty)",
                "weight": -2.5
            })),
        )
        .await
        .unwrap();
        assert!((r_agent_query["query"]["weight"].as_f64().unwrap() - (-2.5)).abs() < 1e-6);

        let _ = std::fs::remove_dir_all(&base_path);
    }

    #[tokio::test]
    async fn test_agent_cli_and_programmatic_api() {
        let base_path = create_temp_dir("agent_cli");
        let agent_dir = base_path.join("agent_data");
        let api = AgentAPI::new(Some(agent_dir.clone()), true).unwrap();

        let f_res = api
            .upsert_feed(
                "https://www.cell.com/neuron/current.rss",
                Some("Neuron"),
                Some("Neuroscience"),
                Some(true),
                false,
            )
            .await
            .unwrap();
        assert_eq!(f_res["feed"]["title"], "Neuron");

        let f_res2 = api
            .upsert_feed(
                "https://www.cell.com/neuron/current.rss",
                Some("Neuron (Cell Press)"),
                Some("Neuroscience"),
                Some(true),
                false,
            )
            .await
            .unwrap();
        assert_eq!(f_res2["feed"]["id"], f_res["feed"]["id"]);
        assert_eq!(f_res2["feed"]["title"], "Neuron (Cell Press)");

        // CLI commands (`queries set`, `queries weight`, `feeds list`, `entries list`)
        let args1 = CliArgs::try_parse_from([
            "smart-rss",
            "queries",
            "set",
            "--name",
            "Single-Cell Omics",
            "--type",
            "noul",
            "--weight",
            "2.0",
            "--instructions",
            "Does this paper use single-cell sequencing?",
            "--true-desc",
            "Uses single-cell sequencing",
            "--false-desc",
            "No single-cell data",
        ])
        .unwrap();
        execute_cli_resource(args1.resource.unwrap(), Some(agent_dir.clone()))
            .await
            .unwrap();

        let args2 = CliArgs::try_parse_from([
            "smart-rss",
            "queries",
            "weight",
            "--name",
            "Uses fMRI or EEG (Penalty)",
            "--weight",
            "-1.5",
        ])
        .unwrap();
        execute_cli_resource(args2.resource.unwrap(), Some(agent_dir.clone()))
            .await
            .unwrap();

        let q_updated = api
            .db
            .get_query_by_name("Uses fMRI or EEG (Penalty)")
            .unwrap()
            .unwrap();
        assert!((q_updated["weight"].as_f64().unwrap() - (-1.5)).abs() < 1e-6);

        let _ = std::fs::remove_dir_all(&base_path);
    }

    #[test]
    fn test_historical_query_snapshot_preserved_when_query_modified_or_deleted() {
        let base_path = create_temp_dir("snapshot");
        let db = Database::new(base_path.join("snapshot.db"), false).unwrap();
        let q1 = db
            .create_query(
                "Original Query Name",
                "noul",
                "Original instructions at test time",
                &json!({"true": "Original True", "false": "Original False"}),
                2.0,
                true,
            )
            .unwrap();
        let q1_id = q1["id"].as_i64().unwrap();

        let entries = db
            .insert_new_entries(&[json!({
                "entry_uid": "snap_entry_1",
                "feed_id": Value::Null,
                "journal_title": "Neuron",
                "title": "Synaptic Plasticity in Cortical Circuits",
                "link": "",
                "authors": "",
                "published_at": "",
                "summary": "",
            })])
            .unwrap();
        let eid = entries[0]["id"].as_i64().unwrap();

        let norm_res = normalize_jev_answer(&q1, &json!({"noul": 0.92, "confidence": 0.95})).unwrap();
        let mut score_map = HashMap::new();
        score_map.insert(q1_id, norm_res);
        db.save_entry_scores(eid, &score_map).unwrap();

        // Modify query's name, instructions, and weight
        db.update_query(
            q1_id,
            Some("Completely Different Query Name"),
            None,
            Some("Modified instructions after evaluation"),
            None,
            Some(-5.0),
            None,
        )
        .unwrap();

        let listed_after_edit = db.list_entries("active", None, None).unwrap();
        let qs_edit = &listed_after_edit[0]["query_scores"][0];
        assert_eq!(qs_edit["query_name"], "Original Query Name");
        assert_eq!(qs_edit["query_modified_since_run"], true);
        assert!((qs_edit["confidence"].as_f64().unwrap() - 0.95).abs() < 1e-6);
        assert!(qs_edit["raw_outcome_text"]
            .as_str()
            .unwrap()
            .contains("95.0% conf"));
        assert!((qs_edit["weighted_contribution"].as_f64().unwrap() - 1.84).abs() < 1e-6);

        // Delete the query altogether — entry must STILL keep its historical score breakdown!
        db.delete_query(q1_id).unwrap();
        let listed_after_delete = db.list_entries("active", None, None).unwrap();
        let qs_del = &listed_after_delete[0]["query_scores"][0];
        assert_eq!(qs_del["query_name"], "Original Query Name");
        assert_eq!(qs_del["query_deleted_since_run"], true);
        assert!((qs_del["weighted_contribution"].as_f64().unwrap() - 1.84).abs() < 1e-6);

        let _ = std::fs::remove_dir_all(&base_path);
    }

    #[test]
    fn test_auto_prune_archived_entries_older_than_one_month() {
        let base_path = create_temp_dir("prune");
        let db_file = base_path.join("prune.db");
        let db = Database::new(&db_file, false).unwrap();

        let entries = db
            .insert_new_entries(&[
                json!({
                    "entry_uid": "old_archived_1",
                    "feed_id": Value::Null,
                    "journal_title": "Nature",
                    "title": "Archived 35 Days Ago",
                    "link": "",
                    "authors": "",
                    "published_at": "",
                    "summary": "",
                }),
                json!({
                    "entry_uid": "recent_archived_2",
                    "feed_id": Value::Null,
                    "journal_title": "Nature",
                    "title": "Archived 10 Days Ago",
                    "link": "",
                    "authors": "",
                    "published_at": "",
                    "summary": "",
                }),
                json!({
                    "entry_uid": "old_active_3",
                    "feed_id": Value::Null,
                    "journal_title": "Nature",
                    "title": "Active Entry 45 Days Old (Must Not Be Removed)",
                    "link": "",
                    "authors": "",
                    "published_at": "",
                    "summary": "",
                }),
            ])
            .unwrap();

        let old_arch_id = entries[0]["id"].as_i64().unwrap();
        let recent_arch_id = entries[1]["id"].as_i64().unwrap();
        let old_active_id = entries[2]["id"].as_i64().unwrap();

        db.set_entry_status(old_arch_id, "archived").unwrap();
        db.set_entry_status(recent_arch_id, "archived").unwrap();

        let now = Utc::now();
        let ts_35_days_ago = (now - ChronoDuration::days(35)).format("%Y-%m-%dT%H:%M:%S+00:00").to_string();
        let ts_10_days_ago = (now - ChronoDuration::days(10)).format("%Y-%m-%dT%H:%M:%S+00:00").to_string();
        let ts_45_days_ago = (now - ChronoDuration::days(45)).format("%Y-%m-%dT%H:%M:%S+00:00").to_string();

        db.update_entry_timestamps_for_test(old_arch_id, Some(&ts_35_days_ago), &ts_35_days_ago)
            .unwrap();
        db.update_entry_timestamps_for_test(recent_arch_id, Some(&ts_10_days_ago), &ts_10_days_ago)
            .unwrap();
        db.update_entry_timestamps_for_test(old_active_id, None, &ts_45_days_ago)
            .unwrap();

        let db_reopened = Database::new(&db_file, false).unwrap();
        let counts = db_reopened.get_counts().unwrap();
        assert_eq!(counts["archived_entries"], 1);
        assert_eq!(counts["active_entries"], 1);

        let remaining_archived = db_reopened.list_entries("archived", None, None).unwrap();
        assert_eq!(remaining_archived.len(), 1);
        assert_eq!(remaining_archived[0]["id"].as_i64().unwrap(), recent_arch_id);

        let _ = std::fs::remove_dir_all(&base_path);
    }

    #[test]
    fn test_reset_all_entries_clears_active_and_archived_memory() {
        let base_path = create_temp_dir("reset");
        let db = Database::new(base_path.join("reset.db"), false).unwrap();
        let feed = db
            .create_feed("Quantum Bio", "https://example.org/rss.xml", "Biology", true)
            .unwrap();
        let fid = feed["id"].as_i64().unwrap();

        let parsed = parse_feed_content(
            SAMPLE_RSS_XML.as_bytes(),
            feed["url"].as_str().unwrap(),
            Some(fid),
            feed["title"].as_str(),
            50,
        )
        .unwrap();
        let entries_arr = parsed["entries"].as_array().unwrap();

        let inserted_1 = db.insert_new_entries(entries_arr).unwrap();
        assert_eq!(inserted_1.len(), 2);
        db.set_entry_status(inserted_1[1]["id"].as_i64().unwrap(), "archived")
            .unwrap();
        db.mark_feed_fetched(fid, None).unwrap();

        let counts_before = db.get_counts().unwrap();
        assert_eq!(counts_before["active_entries"], 1);
        assert_eq!(counts_before["archived_entries"], 1);
        assert_eq!(counts_before["total_remembered_entries"], 2);

        let deleted_count = db.reset_all_entries().unwrap();
        assert_eq!(deleted_count, 2);

        let counts_after = db.get_counts().unwrap();
        assert_eq!(counts_after["active_entries"], 0);
        assert_eq!(counts_after["archived_entries"], 0);
        assert_eq!(counts_after["total_remembered_entries"], 0);
        assert_eq!(db.get_known_entry_uids().unwrap().len(), 0);

        let updated_feed = db.get_feed(fid).unwrap().unwrap();
        assert!(updated_feed["last_fetched_at"].is_null());

        let inserted_again = db.insert_new_entries(entries_arr).unwrap();
        assert_eq!(inserted_again.len(), 2);

        let _ = std::fs::remove_dir_all(&base_path);
    }

    #[tokio::test]
    async fn test_workspace_skills_provisioning_and_folder_switch() {
        let base_path = create_temp_dir("workspace_skills");
        let folder_a = base_path.join("folder_a");
        let folder_b = base_path.join("folder_b");

        let state = AppState::new(Some(folder_a.clone()), true).unwrap();
        let ws_status = dispatch_api_request(&state, "GET", "/api/workspace", None)
            .await
            .unwrap();
        assert_eq!(ws_status["data_dir"], folder_a.to_string_lossy().as_ref());
        assert!(folder_a
            .join(".agents/skills/manage-rss-feeds/SKILL.md")
            .exists());
        assert!(folder_a
            .join(".agents/skills/manage-jev-queries/SKILL.md")
            .exists());
        assert!(folder_a.join("AGENTS.md").exists());
        assert!(folder_a.join("GEMINI.md").exists());
        assert!(folder_a.join("CLAUDE.md").exists());
        assert!(folder_a.join("smart-rss").exists());
        assert!(folder_a.join("smart_rss.db").exists());

        // Switch folder to folder_b via PUT /api/workspace
        let switched = dispatch_api_request(
            &state,
            "PUT",
            "/api/workspace",
            Some(json!({ "data_dir": folder_b.to_string_lossy() })),
        )
        .await
        .unwrap();
        assert_eq!(switched["data_dir"], folder_b.to_string_lossy().as_ref());
        assert!(folder_b
            .join(".agents/skills/manage-rss-feeds/SKILL.md")
            .exists());
        assert!(folder_b
            .join(".agents/skills/manage-jev-queries/SKILL.md")
            .exists());
        assert!(folder_b.join("smart-rss").exists());
        assert!(folder_b.join("smart_rss.db").exists());

        let _ = std::fs::remove_dir_all(&base_path);
    }

    #[tokio::test]
    async fn test_multi_provider_catalog_nanogpt_and_custom_provider() {
        let base_path = create_temp_dir("providers_test");
        let state = AppState::new(Some(base_path.join("prov_data")), true).unwrap();

        let r_provs = dispatch_api_request(&state, "GET", "/api/providers", None)
            .await
            .unwrap();
        let prov_list = r_provs["providers"].as_array().unwrap();
        assert!(prov_list.len() >= 25);
        assert!(prov_list.iter().any(|p| p["id"] == "nanogpt"));
        assert!(prov_list.iter().any(|p| p["id"] == "opencode"));
        assert!(prov_list.iter().any(|p| p["id"] == "custom"));

        // Save OpenRouter key and NanoGPT key under the same master password
        dispatch_api_request(
            &state,
            "POST",
            "/api/auth/setup",
            Some(json!({
                "provider_id": "openrouter",
                "api_key": "sk-or-v1-openrouter-key-1111",
                "password": "master-password"
            })),
        )
        .await
        .unwrap();

        dispatch_api_request(
            &state,
            "POST",
            "/api/auth/setup",
            Some(json!({
                "provider_id": "nanogpt",
                "api_key": "nk-nanogpt-secret-key-2222",
                "password": "master-password",
                "jev_model": "deepseek-chat"
            })),
        )
        .await
        .unwrap();

        // Lock & unlock and verify both provider keys are preserved
        dispatch_api_request(&state, "POST", "/api/auth/lock", None)
            .await
            .unwrap();
        dispatch_api_request(
            &state,
            "POST",
            "/api/auth/unlock",
            Some(json!({ "password": "master-password" })),
        )
        .await
        .unwrap();

        assert_eq!(
            state.crypto.get_api_key_for_provider("openrouter").as_deref(),
            Some("sk-or-v1-openrouter-key-1111")
        );
        assert_eq!(
            state.crypto.get_api_key_for_provider("nanogpt").as_deref(),
            Some("nk-nanogpt-secret-key-2222")
        );

        // Configure custom provider with general link and custom base_url
        let r_custom = dispatch_api_request(
            &state,
            "PUT",
            "/api/settings",
            Some(json!({
                "ai_provider": "custom",
                "jev_model": "my-custom-llm-v1",
                "custom_provider_name": "My Lab vLLM Cluster",
                "custom_base_url": "https://llm.mylab.org/v1",
                "custom_portal_url": "https://portal.mylab.org/keys"
            })),
        )
        .await
        .unwrap();

        assert_eq!(r_custom["active_provider"]["provider_id"], "custom");
        assert_eq!(
            r_custom["active_provider"]["provider_name"],
            "My Lab vLLM Cluster"
        );
        assert_eq!(
            r_custom["active_provider"]["base_url"],
            "https://llm.mylab.org/v1"
        );
        assert_eq!(
            r_custom["active_provider"]["portal_url"],
            "https://portal.mylab.org/keys"
        );
        assert_eq!(r_custom["active_provider"]["model"], "my-custom-llm-v1");

        let _ = std::fs::remove_dir_all(&base_path);
    }

    #[tokio::test]
    async fn test_initial_seeded_feeds_enabled_on_first_refresh() {
        let base_path = create_temp_dir("first_refresh_test");
        let state = AppState::new(Some(base_path.join("fresh_data")), true).unwrap();

        // Verify newly seeded starter feeds have `enabled == true`
        let r_feeds = dispatch_api_request(&state, "GET", "/api/feeds", None)
            .await
            .unwrap();
        let feeds_arr = r_feeds["feeds"].as_array().unwrap();
        assert_eq!(feeds_arr.len(), 4);
        for f in feeds_arr {
            assert_eq!(f["enabled"], true);
        }

        // Write a local RSS XML file and point an enabled feed at it to test /api/feeds/refresh end-to-end
        let rss_file = base_path.join("sample_feed.xml");
        std::fs::write(&rss_file, SAMPLE_RSS_XML).unwrap();

        // Disable the 4 internet feeds and add our local enabled feed (both bool `true` and int `1` supported)
        for f in feeds_arr {
            let fid = f["id"].as_i64().unwrap();
            dispatch_api_request(
                &state,
                "PUT",
                &format!("/api/feeds/{}", fid),
                Some(json!({ "enabled": false })),
            )
            .await
            .unwrap();
        }

        let created = dispatch_api_request(
            &state,
            "POST",
            "/api/feeds",
            Some(json!({
                "title": "Local Test Journal",
                "url": format!("file://{}", rss_file.to_string_lossy()),
                "category": "Test",
                "enabled": 1
            })),
        )
        .await
        .unwrap();
        assert_eq!(created["feed"]["enabled"], true);

        let refresh_res = dispatch_api_request(
            &state,
            "POST",
            "/api/feeds/refresh",
            Some(json!({ "evaluate_unscored_active": false })),
        )
        .await
        .unwrap();

        assert_eq!(refresh_res["feeds_fetched"], 1);
        assert_eq!(refresh_res["newly_inserted"], 2);

        let _ = std::fs::remove_dir_all(&base_path);
    }

    #[tokio::test]
    async fn test_multi_feed_ids_filtering_and_select_none() {
        let base_path = create_temp_dir("multi_feed_filter");
        let state = AppState::new(Some(base_path.join("data")), true).unwrap();

        let f1 = state
            .db
            .create_feed("Journal Alpha", "https://example.org/a.rss", "Neuro", true)
            .unwrap()["id"]
            .as_i64()
            .unwrap();
        let f2 = state
            .db
            .create_feed("Journal Beta", "https://example.org/b.rss", "Physics", true)
            .unwrap()["id"]
            .as_i64()
            .unwrap();
        let f3 = state
            .db
            .create_feed("Journal Gamma", "https://example.org/c.rss", "AI", true)
            .unwrap()["id"]
            .as_i64()
            .unwrap();

        let entries_payload = json!([
            {
                "entry_uid": "uid-alpha-1",
                "feed_id": f1,
                "journal_title": "Journal Alpha",
                "title": "Alpha Paper 1",
                "link": "https://example.org/a1",
                "authors": "Alice",
                "published_at": "2026-11-18",
                "summary": "Alpha abstract"
            },
            {
                "entry_uid": "uid-beta-1",
                "feed_id": f2,
                "journal_title": "Journal Beta",
                "title": "Beta Paper 1",
                "link": "https://example.org/b1",
                "authors": "Bob",
                "published_at": "2026-11-18",
                "summary": "Beta abstract"
            },
            {
                "entry_uid": "uid-gamma-1",
                "feed_id": f3,
                "journal_title": "Journal Gamma",
                "title": "Gamma Paper 1",
                "link": "https://example.org/c1",
                "authors": "Carol",
                "published_at": "2026-11-18",
                "summary": "Gamma abstract"
            }
        ]);
        let inserted = state
            .db
            .insert_new_entries(entries_payload.as_array().unwrap())
            .unwrap();
        let entry_a1_id = inserted[0]["id"].as_i64().unwrap();
        let q = state
            .db
            .create_query("Test Q", "noul", "Is it good?", &json!({}), 1.0, true)
            .unwrap();
        let qid = q["id"].as_i64().unwrap();
        let mut sc = HashMap::new();
        sc.insert(
            qid,
            json!({"normalized_score": 0.85, "raw_response": {"noul": 0.85}}),
        );
        state.db.save_entry_scores(entry_a1_id, &sc).unwrap();

        // 1. All entries (no feed_ids param) + feed_stats
        let res_all = dispatch_api_request(&state, "GET", "/api/entries?status=active", None)
            .await
            .unwrap();
        assert_eq!(res_all["entries"].as_array().unwrap().len(), 3);
        let f1_key = f1.to_string();
        assert_eq!(res_all["feed_stats"][&f1_key]["total"], 1);
        assert_eq!(
            res_all["feed_stats"][&f1_key]["scored_pcts"]
                .as_array()
                .unwrap()[0]
                .as_f64()
                .unwrap(),
            85.0
        );

        // 2. Multiple selected journals (f1 and f3)
        let res_subset = dispatch_api_request(
            &state,
            "GET",
            &format!("/api/entries?status=active&feed_ids={f1},{f3}"),
            None,
        )
        .await
        .unwrap();
        let subset_entries = res_subset["entries"].as_array().unwrap();
        assert_eq!(subset_entries.len(), 2);
        let titles: Vec<&str> = subset_entries
            .iter()
            .filter_map(|e| e["title"].as_str())
            .collect();
        assert!(titles.contains(&"Alpha Paper 1"));
        assert!(titles.contains(&"Gamma Paper 1"));
        assert!(!titles.contains(&"Beta Paper 1"));

        // 3. Select none (feed_ids=none)
        let res_none = dispatch_api_request(
            &state,
            "GET",
            "/api/entries?status=active&feed_ids=none",
            None,
        )
        .await
        .unwrap();
        assert_eq!(res_none["entries"].as_array().unwrap().len(), 0);

        // 4. Backwards compatibility with single feed_id
        let res_single = dispatch_api_request(
            &state,
            "GET",
            &format!("/api/entries?status=active&feed_id={f2}"),
            None,
        )
        .await
        .unwrap();
        let single_entries = res_single["entries"].as_array().unwrap();
        assert_eq!(single_entries.len(), 1);
        assert_eq!(single_entries[0]["title"], "Beta Paper 1");

        let _ = std::fs::remove_dir_all(&base_path);
    }
}
