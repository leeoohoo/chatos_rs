use crate::types::McpServerConfig;
use crate::utils::generate_id;
use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    #[serde(default, alias = "apiKey")]
    pub api_key: String,
    #[serde(default, alias = "baseUrl")]
    pub base_url: String,
    #[serde(default)]
    pub model: String,
    #[serde(default = "default_reasoning_enabled", alias = "reasoningEnabled")]
    pub reasoning_enabled: bool,
    #[serde(default = "default_responses_enabled", alias = "responsesEnabled")]
    pub responses_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfigRecord {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default, alias = "apiKey")]
    pub api_key: String,
    #[serde(default, alias = "baseUrl")]
    pub base_url: String,
    #[serde(default)]
    pub model: String,
    #[serde(default = "default_reasoning_enabled", alias = "reasoningEnabled")]
    pub reasoning_enabled: bool,
    #[serde(default = "default_responses_enabled", alias = "responsesEnabled")]
    pub responses_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    #[serde(alias = "aiTimeoutMs")]
    pub ai_timeout_ms: Option<i64>,
    #[serde(alias = "aiMaxOutputBytes")]
    pub ai_max_output_bytes: Option<i64>,
    #[serde(alias = "aiToolMaxTurns")]
    pub ai_tool_max_turns: Option<i64>,
    #[serde(alias = "aiMaxRetries")]
    pub ai_max_retries: Option<i64>,
    #[serde(alias = "commandTimeoutMs")]
    pub command_timeout_ms: Option<i64>,
    #[serde(alias = "commandMaxOutputBytes")]
    pub command_max_output_bytes: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplaceRecord {
    pub id: String,
    pub name: String,
    pub plugin_count: i64,
    pub active: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceEntry {
    pub id: String,
    pub name: String,
    pub json: String,
}

pub struct ConfigStore {
    conn: Connection,
    marketplace_path: Option<PathBuf>,
}

impl ConfigStore {
    pub fn new(db_path: &str, marketplace_path: Option<PathBuf>) -> Result<Self, String> {
        let conn = Connection::open(db_path).map_err(|err| err.to_string())?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|err| err.to_string())?;
        conn.pragma_update(None, "synchronous", "NORMAL")
            .map_err(|err| err.to_string())?;
        conn.execute_batch(
            r#"
      CREATE TABLE IF NOT EXISTS subagent_settings (
        key TEXT PRIMARY KEY,
        value_json TEXT,
        updated_at TEXT NOT NULL
      );

      CREATE TABLE IF NOT EXISTS subagent_marketplaces (
        id TEXT PRIMARY KEY,
        name TEXT,
        json TEXT NOT NULL,
        plugin_count INTEGER NOT NULL DEFAULT 0,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        active INTEGER NOT NULL DEFAULT 0
      );
      CREATE INDEX IF NOT EXISTS subagent_marketplaces_active_idx ON subagent_marketplaces(active);

      CREATE TABLE IF NOT EXISTS subagent_mcp_servers (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        transport TEXT NOT NULL,
        command TEXT NOT NULL,
        args_json TEXT,
        endpoint_url TEXT,
        headers_json TEXT,
        enabled INTEGER NOT NULL DEFAULT 1,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
      );
      CREATE INDEX IF NOT EXISTS subagent_mcp_servers_enabled_idx ON subagent_mcp_servers(enabled);
      "#,
        )
        .map_err(|err| err.to_string())?;
        Ok(Self { conn, marketplace_path })
    }

    pub fn get_setting<T: serde::de::DeserializeOwned>(&self, key: &str, fallback: T) -> T {
        let mut stmt = match self
            .conn
            .prepare("SELECT value_json FROM subagent_settings WHERE key = ?1")
        {
            Ok(stmt) => stmt,
            Err(_) => return fallback,
        };
        let value: Option<String> = stmt
            .query_row(params![key], |row| row.get(0))
            .unwrap_or(None);
        match value {
            Some(text) => serde_json::from_str(&text).unwrap_or(fallback),
            None => fallback,
        }
    }

    pub fn set_setting(&self, key: &str, value: serde_json::Value) -> Result<(), String> {
        let now = Utc::now().to_rfc3339();
        let text = serde_json::to_string(&value).map_err(|err| err.to_string())?;
        self.conn
            .execute(
                "INSERT INTO subagent_settings (key, value_json, updated_at) VALUES (?1, ?2, ?3) ON CONFLICT(key) DO UPDATE SET value_json = ?2, updated_at = ?3",
                params![key, text, now],
            )
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub fn get_allow_prefixes(&self) -> Vec<String> {
        let parsed: Vec<String> = self.get_setting("mcp_allow_prefixes", Vec::new());
        parsed
            .into_iter()
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect()
    }

    pub fn set_allow_prefixes(&self, prefixes: Vec<String>) -> Result<(), String> {
        let cleaned: Vec<String> = prefixes
            .into_iter()
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect();
        self.set_setting("mcp_allow_prefixes", serde_json::json!(cleaned))
    }

    pub fn get_effective_allow_prefixes(&self) -> Vec<String> {
        let manual = self.get_allow_prefixes();
        if !manual.is_empty() {
            return manual;
        }
        let servers = self
            .list_mcp_servers()
            .into_iter()
            .filter(|s| s.enabled)
            .collect::<Vec<_>>();
        let mut prefixes = std::collections::HashSet::new();
        for server in servers {
            let name = normalize_mcp_name(&server.name);
            if !name.is_empty() {
                prefixes.insert(format!("mcp_{name}_"));
            }
        }
        prefixes.into_iter().collect()
    }

    pub fn get_plugins_root(&self) -> String {
        self.get_setting("plugins_root", String::new())
    }

    pub fn set_plugins_root(&self, value: &str) -> Result<(), String> {
        self.set_setting("plugins_root", serde_json::json!(value))
    }

    pub fn get_plugins_source_root(&self) -> String {
        self.get_setting("plugins_source_root", String::new())
    }

    pub fn set_plugins_source_root(&self, value: &str) -> Result<(), String> {
        self.set_setting("plugins_source_root", serde_json::json!(value))
    }

    pub fn set_marketplace_path(&self, value: &str) -> Result<(), String> {
        self.set_setting("marketplace_path", serde_json::json!(value))
    }

    pub fn get_registry_path(&self) -> String {
        self.get_setting("registry_path", String::new())
    }

    pub fn set_registry_path(&self, value: &str) -> Result<(), String> {
        self.set_setting("registry_path", serde_json::json!(value))
    }

    pub fn get_db_path(&self) -> String {
        self.get_setting("db_path", String::new())
    }

    pub fn set_db_path(&self, value: &str) -> Result<(), String> {
        self.set_setting("db_path", serde_json::json!(value))
    }

    pub fn get_model_config(&self) -> ModelConfig {
        let active = self.get_active_model_config();
        ModelConfig {
            api_key: active.api_key,
            base_url: active.base_url,
            model: active.model,
            reasoning_enabled: active.reasoning_enabled,
            responses_enabled: active.responses_enabled,
        }
    }

    pub fn set_model_config(&self, config: ModelConfig) -> Result<(), String> {
        let entry = ModelConfigRecord {
            id: "default".to_string(),
            name: "Default".to_string(),
            api_key: config.api_key.clone(),
            base_url: config.base_url.clone(),
            model: config.model.clone(),
            reasoning_enabled: config.reasoning_enabled,
            responses_enabled: config.responses_enabled,
        };
        self.set_model_configs(vec![entry])?;
        self.set_active_model_id("default")?;
        self.set_setting("model_config", serde_json::json!(config))
    }

    pub fn get_model_configs(&self) -> Vec<ModelConfigRecord> {
        let parsed: Vec<ModelConfigRecord> = self.get_setting("model_configs", Vec::new());
        let mut cleaned = Vec::new();
        for entry in parsed {
            let normalized = normalize_model_config(entry);
            if !normalized.api_key.is_empty()
                || !normalized.base_url.is_empty()
                || !normalized.model.is_empty()
            {
                cleaned.push(normalized);
            }
        }
        if !cleaned.is_empty() {
            return cleaned;
        }
        let legacy: Option<ModelConfig> = self.get_setting("model_config", None);
        let legacy = match legacy {
            Some(value) => value,
            None => return Vec::new(),
        };
        if legacy.api_key.trim().is_empty()
            && legacy.base_url.trim().is_empty()
            && legacy.model.trim().is_empty()
        {
            return Vec::new();
        }
        vec![ModelConfigRecord {
            id: "default".to_string(),
            name: "Default".to_string(),
            api_key: legacy.api_key,
            base_url: legacy.base_url,
            model: legacy.model,
            reasoning_enabled: legacy.reasoning_enabled,
            responses_enabled: legacy.responses_enabled,
        }]
    }

    pub fn set_model_configs(&self, list: Vec<ModelConfigRecord>) -> Result<(), String> {
        let mut cleaned = Vec::new();
        for entry in list {
            let normalized = normalize_model_config(entry);
            if !normalized.id.trim().is_empty() {
                cleaned.push(normalized);
            }
        }
        self.set_setting("model_configs", serde_json::json!(cleaned))
    }

    pub fn get_active_model_id(&self) -> String {
        self.get_setting("active_model_id", String::new())
            .trim()
            .to_string()
    }

    pub fn set_active_model_id(&self, id: &str) -> Result<(), String> {
        self.set_setting("active_model_id", serde_json::json!(id.trim()))
    }

    pub fn get_runtime_config(&self) -> RuntimeConfig {
        self.get_setting(
            "runtime_config",
            RuntimeConfig {
                ai_timeout_ms: None,
                ai_max_output_bytes: None,
                ai_tool_max_turns: None,
                ai_max_retries: None,
                command_timeout_ms: None,
                command_max_output_bytes: None,
            },
        )
    }

    pub fn set_runtime_config(&self, config: RuntimeConfig) -> Result<(), String> {
        self.set_setting("runtime_config", serde_json::json!(config))
    }

    pub fn list_mcp_servers(&self) -> Vec<McpServerConfig> {
        let mut stmt = match self.conn.prepare(
            "SELECT id, name, transport, command, args_json, endpoint_url, headers_json, enabled, created_at, updated_at FROM subagent_mcp_servers ORDER BY created_at DESC",
        ) {
            Ok(stmt) => stmt,
            Err(_) => return Vec::new(),
        };
        let rows = match stmt.query_map([], |row| {
            let args_json: Option<String> = row.get(4)?;
            let args = args_json
                .and_then(|raw| serde_json::from_str::<Vec<String>>(&raw).ok())
                .unwrap_or_default();
            Ok(McpServerConfig {
                id: row.get(0)?,
                name: row.get(1)?,
                transport: row.get(2)?,
                command: row.get(3)?,
                args,
                endpoint_url: row.get::<_, Option<String>>(5)?.unwrap_or_default(),
                headers_json: row.get::<_, Option<String>>(6)?.unwrap_or_default(),
                enabled: row.get::<_, i64>(7)? == 1,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            })
        }) {
            Ok(rows) => rows,
            Err(_) => return Vec::new(),
        };
        rows.filter_map(Result::ok).collect()
    }

    pub fn save_mcp_server(&self, input: McpServerConfig) -> Result<String, String> {
        let now = Utc::now().to_rfc3339();
        let id = if input.id.is_empty() {
            generate_id("mcp")
        } else {
            input.id.clone()
        };
        let args_json = serde_json::to_string(&input.args).unwrap_or_else(|_| "[]".to_string());
        self.conn
            .execute(
                r#"
        INSERT INTO subagent_mcp_servers (id, name, transport, command, args_json, endpoint_url, headers_json, enabled, created_at, updated_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        ON CONFLICT(id) DO UPDATE SET
          name = excluded.name,
          transport = excluded.transport,
          command = excluded.command,
          args_json = excluded.args_json,
          endpoint_url = excluded.endpoint_url,
          headers_json = excluded.headers_json,
          enabled = excluded.enabled,
          updated_at = excluded.updated_at
        "#,
                params![
                    id,
                    input.name,
                    input.transport,
                    input.command,
                    args_json,
                    input.endpoint_url,
                    input.headers_json,
                    if input.enabled { 1 } else { 0 },
                    now,
                    now
                ],
            )
            .map_err(|err| err.to_string())?;
        Ok(id)
    }

    pub fn delete_mcp_server(&self, id: &str) -> Result<(), String> {
        self.conn
            .execute("DELETE FROM subagent_mcp_servers WHERE id = ?1", params![id])
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub fn list_marketplaces(&self) -> Vec<MarketplaceRecord> {
        let mut stmt = match self.conn.prepare(
            "SELECT id, name, plugin_count, active, created_at, updated_at FROM subagent_marketplaces ORDER BY created_at DESC",
        ) {
            Ok(stmt) => stmt,
            Err(_) => return Vec::new(),
        };
        let rows = match stmt.query_map([], |row| {
            Ok(MarketplaceRecord {
                id: row.get(0)?,
                name: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                plugin_count: row.get(2)?,
                active: row.get::<_, i64>(3)? == 1,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        }) {
            Ok(rows) => rows,
            Err(_) => return Vec::new(),
        };
        rows.filter_map(Result::ok).collect()
    }

    pub fn get_active_marketplaces(&self) -> Vec<MarketplaceEntry> {
        let mut stmt = match self.conn.prepare(
            "SELECT id, name, json FROM subagent_marketplaces WHERE active = 1 ORDER BY created_at DESC",
        ) {
            Ok(stmt) => stmt,
            Err(_) => return Vec::new(),
        };
        let rows = match stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let name: Option<String> = row.get(1)?;
            let json: String = row.get(2)?;
            Ok(MarketplaceEntry {
                id: id.clone(),
                name: name.unwrap_or(id),
                json,
            })
        }) {
            Ok(rows) => rows,
            Err(_) => return Vec::new(),
        };
        rows.filter_map(Result::ok).collect()
    }

    pub fn save_marketplace(&self, name: Option<&str>, json_text: &str, activate: bool) -> Result<String, String> {
        let parsed = parse_marketplace_json(json_text)?;
        let plugin_count = parsed.plugin_count;
        let now = Utc::now().to_rfc3339();
        let id = generate_id("marketplace");
        let normalized_name = name
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| format!("marketplace-{plugin_count}"));
        self.conn
            .execute(
                "INSERT INTO subagent_marketplaces (id, name, json, plugin_count, created_at, updated_at, active) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![id, normalized_name, json_text, plugin_count, now, now, if activate { 1 } else { 0 }],
            )
            .map_err(|err| err.to_string())?;
        if activate {
            let _ = self.set_marketplace_active(&id, true);
        }
        Ok(id)
    }

    pub fn set_marketplace_active(&self, id: &str, active: bool) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE subagent_marketplaces SET active = ?1 WHERE id = ?2",
                params![if active { 1 } else { 0 }, id],
            )
            .map_err(|err| err.to_string())?;
        self.ensure_marketplace_file()
    }

    pub fn delete_marketplace(&self, id: &str) -> Result<(), String> {
        self.conn
            .execute("DELETE FROM subagent_marketplaces WHERE id = ?1", params![id])
            .map_err(|err| err.to_string())?;
        self.ensure_marketplace_file()
    }

    pub fn ensure_marketplace_file(&self) -> Result<(), String> {
        let path = match self.marketplace_path.as_ref() {
            Some(path) => path,
            None => return Ok(()),
        };
        let active = self.get_active_marketplaces();
        if active.is_empty() {
            let _ = std::fs::remove_file(path);
            return Ok(());
        }
        let merged = merge_marketplaces(&active);
        let text = serde_json::to_string_pretty(&merged).map_err(|err| err.to_string())?;
        self.write_marketplace_file(&text)
    }

    pub fn write_marketplace_file(&self, _json_text: &str) -> Result<(), String> {
        if let Some(path) = &self.marketplace_path {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
            }
            std::fs::write(path, _json_text).map_err(|err| err.to_string())?;
        }
        Ok(())
    }

    fn get_active_model_config(&self) -> ModelConfigRecord {
        let configs = self.get_model_configs();
        let active_id = self.get_active_model_id();
        let found = configs
            .iter()
            .find(|entry| entry.id == active_id)
            .cloned()
            .or_else(|| configs.first().cloned());
        found.unwrap_or_else(|| ModelConfigRecord {
            id: "default".to_string(),
            name: "Default".to_string(),
            api_key: String::new(),
            base_url: String::new(),
            model: String::new(),
            reasoning_enabled: true,
            responses_enabled: false,
        })
    }
}

fn normalize_mcp_name(value: &str) -> String {
    let mut out = String::new();
    let mut prev_underscore = false;
    for ch in value.trim().to_lowercase().chars() {
        let valid = ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-';
        if valid {
            out.push(ch);
            prev_underscore = false;
        } else if !prev_underscore {
            out.push('_');
            prev_underscore = true;
        }
    }
    out.trim_matches('_').to_string()
}

fn normalize_model_config(input: ModelConfigRecord) -> ModelConfigRecord {
    let id = input.id.trim().to_string();
    let id = if id.is_empty() { generate_id("model") } else { id };
    let name = input.name.trim().to_string();
    let name = if name.is_empty() { id.clone() } else { name };
    let api_key = input.api_key.trim().to_string();
    let base_url = input.base_url.trim().to_string();
    let model = input.model.trim().to_string();
    ModelConfigRecord {
        id,
        name,
        api_key,
        base_url,
        model,
        reasoning_enabled: input.reasoning_enabled,
        responses_enabled: input.responses_enabled,
    }
}

fn parse_marketplace_json(text: &str) -> Result<ParsedMarketplace, String> {
    let parsed: Value = serde_json::from_str(text).map_err(|_| "Invalid marketplace.json (must be valid JSON with plugins array)".to_string())?;
    let plugins = parsed.get("plugins").and_then(|v| v.as_array()).map(|list| list.len()).unwrap_or(0);
    Ok(ParsedMarketplace { plugin_count: plugins as i64 })
}

fn merge_marketplaces(entries: &[MarketplaceEntry]) -> Value {
    let mut plugins: Vec<Value> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut sources = Vec::new();
    for entry in entries {
        let parsed: Value = serde_json::from_str(&entry.json).unwrap_or_else(|_| json!({}));
        let list = parsed
            .get("plugins")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        sources.push(json!({
            "id": entry.id,
            "name": if entry.name.trim().is_empty() { entry.id.clone() } else { entry.name.clone() },
            "plugins": list.len()
        }));
        for plugin in list {
            let key = build_plugin_key(&plugin);
            if seen.insert(key) {
                plugins.push(plugin);
            }
        }
    }
    json!({
        "name": "merged-marketplace",
        "metadata": {
            "merged": true,
            "sources": sources
        },
        "plugins": plugins
    })
}

fn build_plugin_key(plugin: &Value) -> String {
    let source = plugin.get("source").and_then(|v| v.as_str()).unwrap_or("").trim();
    if !source.is_empty() {
        return format!("source:{source}");
    }
    let name = plugin.get("name").and_then(|v| v.as_str()).unwrap_or("").trim();
    if !name.is_empty() {
        return format!("name:{name}");
    }
    plugin.to_string()
}

struct ParsedMarketplace {
    plugin_count: i64,
}

fn default_reasoning_enabled() -> bool {
    true
}

fn default_responses_enabled() -> bool {
    false
}
