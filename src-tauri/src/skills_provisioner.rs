use crate::version_manager::{list_version_backups, sanitize_version_dir_name, APP_VERSION};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

const EMBEDDED_AGENTS_MD: &str = include_str!("../../AGENTS.md");
const EMBEDDED_MANAGE_FEEDS_SKILL: &str =
    include_str!("../../.agents/skills/manage-rss-feeds/SKILL.md");
const EMBEDDED_MANAGE_QUERIES_SKILL: &str =
    include_str!("../../.agents/skills/manage-jev-queries/SKILL.md");
const EMBEDDED_IMPORT_EXPORT_SKILL: &str =
    include_str!("../../.agents/skills/import-export-config/SKILL.md");

#[derive(Clone)]
pub struct WorkspaceManager {
    data_dir: Arc<RwLock<PathBuf>>,
    config_file: PathBuf,
    is_first_run: Arc<RwLock<bool>>,
}

impl WorkspaceManager {
    pub fn new(cli_override: Option<PathBuf>) -> Self {
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("smart-rss-reader");
        let config_file = config_dir.join("workspace.json");

        let mut first_run = !config_file.exists();

        let resolved_dir = if let Some(ov) = cli_override {
            expand_tilde(&ov.to_string_lossy())
        } else if let Ok(env_dir) = std::env::var("SMART_RSS_DATA_DIR") {
            if !env_dir.trim().is_empty() {
                expand_tilde(env_dir.trim())
            } else {
                load_or_default_data_dir(&config_file, &mut first_run)
            }
        } else {
            load_or_default_data_dir(&config_file, &mut first_run)
        };

        if !resolved_dir.exists() {
            first_run = true;
        }

        let mgr = Self {
            data_dir: Arc::new(RwLock::new(resolved_dir)),
            config_file,
            is_first_run: Arc::new(RwLock::new(first_run)),
        };

        let _ = mgr.provision_workspace();
        let _ = mgr.save_workspace_config();
        mgr
    }

    pub fn new_for_dir(dir: impl AsRef<Path>) -> Self {
        let path_buf = dir.as_ref().to_path_buf();
        let config_file = path_buf.join(".workspace_meta.json");
        let first_run = !path_buf.join(".agents/skills/manage-rss-feeds/SKILL.md").exists();
        let mgr = Self {
            data_dir: Arc::new(RwLock::new(path_buf)),
            config_file,
            is_first_run: Arc::new(RwLock::new(first_run)),
        };
        let _ = mgr.provision_workspace();
        mgr
    }

    pub fn get_data_dir(&self) -> PathBuf {
        self.data_dir.read().unwrap().clone()
    }

    pub fn get_db_path(&self) -> PathBuf {
        self.get_data_dir().join("smart_rss.db")
    }

    pub fn get_secrets_path(&self) -> PathBuf {
        self.get_data_dir().join("secrets.enc")
    }

    fn save_workspace_config(&self) -> Result<(), String> {
        if let Some(parent) = self.config_file.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create config dir: {}", e))?;
        }
        let dir = self.get_data_dir();
        let payload = json!({
            "data_dir": dir.to_string_lossy(),
            "app_version": APP_VERSION,
        });
        let pretty = serde_json::to_string_pretty(&payload).unwrap_or_default();
        std::fs::write(&self.config_file, &pretty)
            .map_err(|e| format!("Failed to write workspace config: {}", e))?;
        if let Some(parent) = self.config_file.parent() {
            let ver_tag = sanitize_version_dir_name(APP_VERSION);
            let _ = std::fs::write(parent.join(format!("workspace_{ver_tag}.json")), &pretty);
        }
        Ok(())
    }

    pub fn set_data_dir(&self, raw_path: &str) -> Result<PathBuf, String> {
        let trimmed = raw_path.trim();
        if trimmed.is_empty() {
            return Err("Data & AI Skills Folder path cannot be empty.".to_string());
        }
        let new_dir = expand_tilde(trimmed);
        std::fs::create_dir_all(&new_dir)
            .map_err(|e| format!("Failed to create folder '{}': {}", new_dir.display(), e))?;

        let old_dir = self.get_data_dir();
        if old_dir != new_dir {
            // If new_dir does not have smart_rss.db or secrets.enc yet, copy existing ones over so data isn't lost
            let old_db = old_dir.join("smart_rss.db");
            let new_db = new_dir.join("smart_rss.db");
            if old_db.exists() && !new_db.exists() {
                let _ = std::fs::copy(&old_db, &new_db);
            }
            let old_sec = old_dir.join("secrets.enc");
            let new_sec = new_dir.join("secrets.enc");
            if old_sec.exists() && !new_sec.exists() {
                let _ = std::fs::copy(&old_sec, &new_sec);
            }
        }

        if let Ok(mut g) = self.data_dir.write() {
            *g = new_dir.clone();
        }
        if let Ok(mut fr) = self.is_first_run.write() {
            *fr = false;
        }
        self.provision_workspace()?;
        self.save_workspace_config()?;
        Ok(new_dir)
    }

    pub fn provision_workspace(&self) -> Result<Value, String> {
        let dir = self.get_data_dir();
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create workspace folder '{}': {}", dir.display(), e))?;

        let feeds_skill_dir = dir.join(".agents").join("skills").join("manage-rss-feeds");
        let queries_skill_dir = dir.join(".agents").join("skills").join("manage-jev-queries");
        let import_export_skill_dir = dir
            .join(".agents")
            .join("skills")
            .join("import-export-config");
        std::fs::create_dir_all(&feeds_skill_dir)
            .map_err(|e| format!("Failed to create skill dir: {}", e))?;
        std::fs::create_dir_all(&queries_skill_dir)
            .map_err(|e| format!("Failed to create skill dir: {}", e))?;
        std::fs::create_dir_all(&import_export_skill_dir)
            .map_err(|e| format!("Failed to create skill dir: {}", e))?;

        // Write AGENTS.md, GEMINI.md, CLAUDE.md
        for doc_name in &["AGENTS.md", "GEMINI.md", "CLAUDE.md"] {
            let p = dir.join(doc_name);
            std::fs::write(&p, EMBEDDED_AGENTS_MD)
                .map_err(|e| format!("Failed to write {}: {}", doc_name, e))?;
        }

        // Write SKILL.md files
        std::fs::write(
            feeds_skill_dir.join("SKILL.md"),
            EMBEDDED_MANAGE_FEEDS_SKILL,
        )
        .map_err(|e| format!("Failed to write manage-rss-feeds/SKILL.md: {}", e))?;
        std::fs::write(
            queries_skill_dir.join("SKILL.md"),
            EMBEDDED_MANAGE_QUERIES_SKILL,
        )
        .map_err(|e| format!("Failed to write manage-jev-queries/SKILL.md: {}", e))?;
        std::fs::write(
            import_export_skill_dir.join("SKILL.md"),
            EMBEDDED_IMPORT_EXPORT_SKILL,
        )
        .map_err(|e| format!("Failed to write import-export-config/SKILL.md: {}", e))?;

        // Write local CLI wrapper scripts (`./smart-rss` and `smart-rss.cmd`)
        let exe_path = std::env::current_exe()
            .unwrap_or_else(|_| PathBuf::from("smart-rss-reader"))
            .to_string_lossy()
            .to_string();
        let data_dir_str = dir.to_string_lossy().to_string();

        let sh_script = format!(
            "#!/usr/bin/env sh\n# Auto-generated by Smart RSS Reader (Tauri)\n# Invokes the local Smart RSS Reader agent CLI against this Data & AI Skills Folder.\nAPP_BIN=\"{exe}\"\nDATA_DIR=\"{data}\"\nif [ -x \"$APP_BIN\" ]; then\n  exec \"$APP_BIN\" --data-dir \"$DATA_DIR\" \"$@\"\nelse\n  exec smart-rss-reader --data-dir \"$DATA_DIR\" \"$@\"\nfi\n",
            exe = exe_path,
            data = data_dir_str
        );
        let sh_path = dir.join("smart-rss");
        std::fs::write(&sh_path, sh_script)
            .map_err(|e| format!("Failed to write smart-rss wrapper: {}", e))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = std::fs::metadata(&sh_path) {
                let mut perms = meta.permissions();
                perms.set_mode(0o755);
                let _ = std::fs::set_permissions(&sh_path, perms);
            }
        }

        let cmd_script = format!(
            "@echo off\r\nREM Auto-generated by Smart RSS Reader (Tauri)\r\nSET \"APP_BIN={exe}\"\r\nSET \"DATA_DIR={data}\"\r\nIF EXIST \"%APP_BIN%\" (\r\n  \"%APP_BIN%\" --data-dir \"%DATA_DIR%\" %*\r\n) ELSE (\r\n  smart-rss-reader --data-dir \"%DATA_DIR%\" %*\r\n)\r\n",
            exe = exe_path,
            data = data_dir_str
        );
        let cmd_path = dir.join("smart-rss.cmd");
        std::fs::write(&cmd_path, cmd_script)
            .map_err(|e| format!("Failed to write smart-rss.cmd wrapper: {}", e))?;

        Ok(self.get_status())
    }

    pub fn get_status(&self) -> Value {
        let dir = self.get_data_dir();
        let tracked = [
            "smart_rss.db",
            "secrets.enc",
            "smart-rss",
            "AGENTS.md",
            "GEMINI.md",
            "CLAUDE.md",
            ".agents/skills/manage-rss-feeds/SKILL.md",
            ".agents/skills/manage-jev-queries/SKILL.md",
            ".agents/skills/import-export-config/SKILL.md",
        ];

        let files: Vec<Value> = tracked
            .iter()
            .map(|rel| {
                let abs = dir.join(rel);
                json!({
                    "rel_path": rel,
                    "abs_path": abs.to_string_lossy(),
                    "exists": abs.exists(),
                })
            })
            .collect();

        let first_run = self.is_first_run.read().map(|g| *g).unwrap_or(false);
        let backups = list_version_backups(&dir);

        json!({
            "app_version": APP_VERSION,
            "data_dir": dir.to_string_lossy(),
            "backups_dir": dir.join("backups").to_string_lossy(),
            "version_backups": backups,
            "db_path": self.get_db_path().to_string_lossy(),
            "secrets_path": self.get_secrets_path().to_string_lossy(),
            "cli_wrapper_path": dir.join("smart-rss").to_string_lossy(),
            "is_first_run": first_run,
            "provisioned_files": files,
        })
    }

    pub fn open_in_file_manager(&self) -> Result<(), String> {
        let dir = self.get_data_dir();
        open::that(&dir).map_err(|e| format!("Failed to open folder '{}': {}", dir.display(), e))
    }
}

pub fn expand_tilde(raw: &str) -> PathBuf {
    if raw == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    }
    if let Some(rest) = raw.strip_prefix("~/").or_else(|| raw.strip_prefix("~\\")) {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(raw)
}

fn load_or_default_data_dir(config_file: &Path, first_run: &mut bool) -> PathBuf {
    if config_file.exists() {
        if let Ok(raw) = std::fs::read_to_string(config_file) {
            if let Ok(v) = serde_json::from_str::<Value>(&raw) {
                if let Some(d) = v.get("data_dir").and_then(|x| x.as_str()) {
                    if !d.trim().is_empty() {
                        return expand_tilde(d.trim());
                    }
                }
            }
        }
    }
    *first_run = true;
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("SmartRSSReader")
}
