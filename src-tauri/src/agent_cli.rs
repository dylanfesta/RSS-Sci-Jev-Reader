use crate::database::Database;
use crate::jev_client::{get_provider_spec, list_providers_json, resolve_active_provider_config};
use crate::rss_service::fetch_and_parse_feed;
use crate::skills_provisioner::WorkspaceManager;
use clap::{Parser, Subcommand};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;

pub struct AgentAPI {
    pub db: Database,
    pub workspace: WorkspaceManager,
}

impl AgentAPI {
    pub fn new(data_dir: Option<PathBuf>, seed_defaults: bool) -> Result<Self, String> {
        let workspace = match data_dir {
            Some(d) => WorkspaceManager::new_for_dir(d),
            None => WorkspaceManager::new(None),
        };
        let db = Database::new(workspace.get_db_path(), seed_defaults)?;
        Ok(Self { db, workspace })
    }

    pub fn list_feeds(&self) -> Result<Vec<Value>, String> {
        self.db.list_feeds()
    }

    pub async fn test_feed(&self, url: &str, max_entries: usize) -> Result<Value, String> {
        let mut parsed = fetch_and_parse_feed(url, None, None, max_entries).await?;
        let known_uids = self.db.get_known_entry_uids()?;
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
        Ok(parsed)
    }

    pub async fn upsert_feed(
        &self,
        url: &str,
        title: Option<&str>,
        category: Option<&str>,
        enabled: Option<bool>,
        validate_first: bool,
    ) -> Result<Value, String> {
        let mut resolved_title = title.unwrap_or("").trim().to_string();
        let mut preview_info: Option<Value> = None;

        if validate_first || resolved_title.is_empty() {
            let p = self.test_feed(url, 5).await?;
            if resolved_title.is_empty() {
                resolved_title = p
                    .get("feed_title")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or(url.trim())
                    .to_string();
            }
            preview_info = Some(p);
        }

        let feed = self
            .db
            .upsert_feed(&resolved_title, url, category, enabled)?;

        let mut res = json!({ "feed": feed });
        if let Some(p) = preview_info {
            let sample_titles: Vec<String> = p
                .get("entries")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .take(3)
                        .filter_map(|e| {
                            e.get("title")
                                .and_then(|t| t.as_str())
                                .map(|s| s.to_string())
                        })
                        .collect()
                })
                .unwrap_or_default();
            res["validation"] = json!({
                "detected_title": p.get("feed_title").cloned().unwrap_or(Value::Null),
                "entry_count": p.get("entry_count").cloned().unwrap_or(Value::Null),
                "sample_titles": sample_titles,
            });
        }
        Ok(res)
    }

    pub fn remove_feed(&self, feed_id: Option<i64>, url: Option<&str>) -> Result<Value, String> {
        if let Some(fid) = feed_id {
            let deleted = self.db.delete_feed(fid)?;
            return Ok(json!({ "deleted": deleted, "feed_id": fid }));
        }
        if let Some(u) = url {
            if !u.trim().is_empty() {
                let deleted = self.db.delete_feed_by_url(u)?;
                return Ok(json!({ "deleted": deleted, "url": u }));
            }
        }
        Err("Provide either feed_id or url to remove a feed.".to_string())
    }

    pub fn list_queries(&self) -> Result<Vec<Value>, String> {
        self.db.list_queries(false)
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
        let query = self.db.upsert_query(
            name,
            query_type,
            instructions,
            criteria,
            weight,
            enabled,
            query_id,
        )?;
        let top_entries = self.list_entries("active", None, None, 5)?;
        let preview: Vec<Value> = top_entries
            .into_iter()
            .map(|e| {
                json!({
                    "id": e.get("id").cloned().unwrap_or(Value::Null),
                    "title": e.get("title").cloned().unwrap_or(Value::Null),
                    "weighted_score": e.get("weighted_score").cloned().unwrap_or(Value::Null),
                    "relevance_pct": e.get("relevance_pct").cloned().unwrap_or(Value::Null),
                })
            })
            .collect();
        Ok(json!({
            "query": query,
            "top_active_entries_preview": preview,
        }))
    }

    pub fn remove_query(&self, query_id: Option<i64>, name: Option<&str>) -> Result<Value, String> {
        if let Some(qid) = query_id {
            let deleted = self.db.delete_query(qid)?;
            return Ok(json!({ "deleted": deleted, "query_id": qid }));
        }
        if let Some(n) = name {
            if !n.trim().is_empty() {
                let deleted = self.db.delete_query_by_name(n)?;
                return Ok(json!({ "deleted": deleted, "name": n }));
            }
        }
        Err("Provide either query_id or name to remove a query.".to_string())
    }

    pub fn list_entries(
        &self,
        status: &str,
        feed_id: Option<i64>,
        search: Option<&str>,
        limit: usize,
    ) -> Result<Vec<Value>, String> {
        let entries = self.db.list_entries(status, feed_id, search)?;
        let cap = limit.max(1);
        Ok(entries.into_iter().take(cap).collect())
    }

    pub fn reset_all_entries(&self) -> Result<Value, String> {
        let deleted_count = self.db.reset_all_entries()?;
        let counts = self.db.get_counts()?;
        Ok(json!({
            "deleted_count": deleted_count,
            "counts": counts,
        }))
    }
}

#[derive(Parser, Debug)]
#[command(
    name = "smart-rss",
    about = "SmartRSSReader Agent CLI — Manage RSS feeds, Jev queries, and ranked entries"
)]
pub struct CliArgs {
    #[arg(long = "data-dir")]
    pub data_dir: Option<PathBuf>,

    #[command(subcommand)]
    pub resource: Option<CliResource>,
}

#[derive(Subcommand, Debug)]
pub enum CliResource {
    /// List, add/upsert, test, or remove RSS feeds
    Feeds {
        #[command(subcommand)]
        action: FeedsAction,
    },
    /// List, create/update, re-weight, or remove Jev queries
    Queries {
        #[command(subcommand)]
        action: QueriesAction,
    },
    /// Inspect ranked active or archived entries
    Entries {
        #[command(subcommand)]
        action: EntriesAction,
    },
    /// Inspect or provision the Data & AI Skills Folder
    Workspace {
        #[command(subcommand)]
        action: WorkspaceAction,
    },
    /// List or configure AI providers (OpenRouter, NanoGPT, OpenCode, OpenAI, Anthropic, Gemini, Custom, etc.)
    Providers {
        #[command(subcommand)]
        action: ProvidersAction,
    },
}

#[derive(Subcommand, Debug)]
pub enum FeedsAction {
    /// List all configured RSS feeds as JSON
    List,
    /// Add or update an RSS feed by URL
    Add {
        #[arg(long)]
        url: String,
        #[arg(long)]
        title: Option<String>,
        #[arg(long, default_value = "General")]
        category: String,
        #[arg(long)]
        disabled: bool,
        #[arg(long)]
        test: bool,
    },
    /// Live-fetch and preview an RSS feed URL without saving
    Test {
        #[arg(long)]
        url: String,
        #[arg(long = "max-entries", default_value_t = 5)]
        max_entries: usize,
    },
    /// Remove an RSS feed by --id or --url
    Remove {
        #[arg(long)]
        id: Option<i64>,
        #[arg(long)]
        url: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum QueriesAction {
    /// List all configured Jev queries and weights as JSON
    List,
    /// Create or update a Jev query (by --name or --id) with positive or negative weight
    Set {
        #[arg(long)]
        id: Option<i64>,
        #[arg(long)]
        name: String,
        #[arg(long = "type")]
        query_type: Option<String>,
        #[arg(long, allow_hyphen_values = true)]
        weight: Option<f64>,
        #[arg(long)]
        instructions: Option<String>,
        #[arg(long = "true-desc")]
        true_desc: Option<String>,
        #[arg(long = "false-desc")]
        false_desc: Option<String>,
        #[arg(long, num_args = 1..)]
        levels: Option<Vec<String>>,
        #[arg(long)]
        disabled: bool,
        #[arg(long)]
        enabled: bool,
    },
    /// Quickly set a query's positive or negative weight
    Weight {
        #[arg(long)]
        id: Option<i64>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long, allow_hyphen_values = true)]
        weight: f64,
    },
    /// Delete a Jev query by --id or --name
    Remove {
        #[arg(long)]
        id: Option<i64>,
        #[arg(long)]
        name: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum EntriesAction {
    /// List ranked entries as JSON
    List {
        #[arg(long, default_value = "active")]
        status: String,
        #[arg(long = "feed-id")]
        feed_id: Option<i64>,
        #[arg(long)]
        search: Option<String>,
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
    /// Delete all active and archived RSS entries to start over blank
    Reset,
}

#[derive(Subcommand, Debug)]
pub enum WorkspaceAction {
    /// Show Data & AI Skills Folder paths and provisioned files
    Status,
    /// Re-sync/provision skills and wrapper scripts into the folder
    Resync,
}

#[derive(Subcommand, Debug)]
pub enum ProvidersAction {
    /// List all available AI providers and the active provider configuration
    List,
    /// Switch active AI provider or configure custom provider Base URL, Portal link, and Model
    Set {
        #[arg(long)]
        provider: String,
        #[arg(long)]
        model: Option<String>,
        #[arg(long = "base-url")]
        base_url: Option<String>,
        #[arg(long = "portal-url")]
        portal_url: Option<String>,
        #[arg(long = "custom-name")]
        custom_name: Option<String>,
    },
}

pub async fn execute_cli_resource(
    resource: CliResource,
    data_dir: Option<PathBuf>,
) -> Result<Value, String> {
    let api = AgentAPI::new(data_dir, true)?;

    match resource {
        CliResource::Feeds { action } => match action {
            FeedsAction::List => Ok(json!({ "feeds": api.list_feeds()? })),
            FeedsAction::Add {
                url,
                title,
                category,
                disabled,
                test,
            } => {
                api.upsert_feed(
                    &url,
                    title.as_deref(),
                    Some(&category),
                    Some(!disabled),
                    test,
                )
                .await
            }
            FeedsAction::Test { url, max_entries } => api.test_feed(&url, max_entries).await,
            FeedsAction::Remove { id, url } => api.remove_feed(id, url.as_deref()),
        },
        CliResource::Queries { action } => match action {
            QueriesAction::List => Ok(json!({ "queries": api.list_queries()? })),
            QueriesAction::Set {
                id,
                name,
                query_type,
                weight,
                instructions,
                true_desc,
                false_desc,
                levels,
                disabled,
                enabled,
            } => {
                let criteria: Option<Value> = if let Some(lvls) = levels {
                    Some(json!(lvls))
                } else if true_desc.is_some() || false_desc.is_some() {
                    Some(json!({
                        "true": true_desc.unwrap_or_default(),
                        "false": false_desc.unwrap_or_default(),
                    }))
                } else {
                    None
                };

                let enabled_val = if disabled {
                    Some(false)
                } else if enabled {
                    Some(true)
                } else {
                    None
                };

                api.upsert_query(
                    Some(&name),
                    query_type.as_deref(),
                    instructions.as_deref(),
                    criteria.as_ref(),
                    weight,
                    enabled_val,
                    id,
                )
            }
            QueriesAction::Weight { id, name, weight } => {
                if id.is_none() && name.as_deref().unwrap_or("").trim().is_empty() {
                    return Err("Provide either --id or --name to set query weight.".to_string());
                }
                api.upsert_query(name.as_deref(), None, None, None, Some(weight), None, id)
            }
            QueriesAction::Remove { id, name } => api.remove_query(id, name.as_deref()),
        },
        CliResource::Entries { action } => match action {
            EntriesAction::List {
                status,
                feed_id,
                search,
                limit,
            } => {
                let entries = api.list_entries(&status, feed_id, search.as_deref(), limit)?;
                let counts = api.db.get_counts()?;
                Ok(json!({
                    "entries": entries,
                    "counts": counts,
                }))
            }
            EntriesAction::Reset => api.reset_all_entries(),
        },
        CliResource::Workspace { action } => match action {
            WorkspaceAction::Status => Ok(api.workspace.get_status()),
            WorkspaceAction::Resync => api.workspace.provision_workspace(),
        },
        CliResource::Providers { action } => match action {
            ProvidersAction::List => {
                let settings = api.db.get_settings()?;
                let active = resolve_active_provider_config(&settings);
                Ok(json!({
                    "active_provider": active,
                    "providers": list_providers_json(),
                }))
            }
            ProvidersAction::Set {
                provider,
                model,
                base_url,
                portal_url,
                custom_name,
            } => {
                let pid = provider.trim().to_lowercase();
                let spec = get_provider_spec(&pid);
                let mut updates = HashMap::new();
                updates.insert("ai_provider".to_string(), spec.id.to_string());
                let resolved_model = model
                    .map(|m| m.trim().to_string())
                    .filter(|m| !m.is_empty())
                    .unwrap_or_else(|| spec.default_model.to_string());
                updates.insert("jev_model".to_string(), resolved_model);
                if let Some(bu) = base_url {
                    updates.insert("custom_base_url".to_string(), bu.trim().to_string());
                } else if spec.id != "custom" {
                    updates.insert("custom_base_url".to_string(), String::new());
                }
                if let Some(pu) = portal_url {
                    updates.insert("custom_portal_url".to_string(), pu.trim().to_string());
                } else if spec.id != "custom" {
                    updates.insert("custom_portal_url".to_string(), String::new());
                }
                if let Some(cn) = custom_name {
                    updates.insert("custom_provider_name".to_string(), cn.trim().to_string());
                } else if spec.id != "custom" {
                    updates.insert("custom_provider_name".to_string(), String::new());
                }
                let updated_settings = api.db.update_settings(&updates)?;
                let active = resolve_active_provider_config(&updated_settings);
                Ok(json!({
                    "active_provider": active,
                    "settings": updated_settings,
                }))
            }
        },
    }
}

pub fn run_agent_cli_from_args(args: CliArgs) -> i32 {
    let Some(resource) = args.resource else {
        return 0;
    };
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Failed to initialize Tokio runtime");
    match rt.block_on(execute_cli_resource(resource, args.data_dir)) {
        Ok(val) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&val).unwrap_or_else(|_| "{}".to_string())
            );
            0
        }
        Err(err) => {
            eprintln!(
                "{}",
                serde_json::to_string_pretty(&json!({ "error": err }))
                    .unwrap_or_else(|_| format!("{{\"error\": {:?}}}", err))
            );
            1
        }
    }
}
