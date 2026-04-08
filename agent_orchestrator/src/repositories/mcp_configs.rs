use mongodb::bson::Document;
use serde_json::Value;

use crate::models::mcp_config::McpConfig;

mod app_links;
mod read_ops;
mod write_ops;

pub use self::app_links::{get_app_ids_for_mcp_config, set_app_ids_for_mcp_config};
pub use self::read_ops::{
    get_mcp_config_by_id, list_enabled_mcp_configs, list_enabled_mcp_configs_by_ids,
    list_mcp_configs,
};
pub use self::write_ops::{create_mcp_config, delete_mcp_config, update_mcp_config};

pub(super) fn normalize_doc(doc: &Document) -> Option<McpConfig> {
    Some(McpConfig {
        id: doc.get_str("id").ok()?.to_string(),
        name: doc.get_str("name").ok()?.to_string(),
        command: doc.get_str("command").ok()?.to_string(),
        r#type: doc.get_str("type").unwrap_or("stdio").to_string(),
        args: doc
            .get_str("args")
            .ok()
            .and_then(|s| serde_json::from_str::<Value>(s).ok()),
        env: doc
            .get_str("env")
            .ok()
            .and_then(|s| serde_json::from_str::<Value>(s).ok()),
        cwd: doc.get_str("cwd").ok().map(|s| s.to_string()),
        user_id: doc.get_str("user_id").ok().map(|s| s.to_string()),
        enabled: doc.get_bool("enabled").unwrap_or(true),
        created_at: doc.get_str("created_at").unwrap_or("").to_string(),
        updated_at: doc.get_str("updated_at").unwrap_or("").to_string(),
    })
}
