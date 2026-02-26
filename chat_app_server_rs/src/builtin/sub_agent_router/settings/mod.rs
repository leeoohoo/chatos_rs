mod git_import;
mod mcp_permissions;
mod plugins;
mod state;
mod types;

use serde_json::Value;

pub use types::{GitImportOptions, InstallPluginOptions, SubAgentRouterStatePaths};

#[allow(dead_code)]
pub fn resolve_state_paths() -> SubAgentRouterStatePaths {
    state::resolve_state_paths()
}

pub fn ensure_state_files() -> Result<SubAgentRouterStatePaths, String> {
    state::ensure_state_files()
}

pub fn load_settings_summary() -> Result<Value, String> {
    plugins::load_settings_summary()
}

pub fn load_mcp_permissions() -> Result<Value, String> {
    mcp_permissions::load_mcp_permissions()
}

pub fn save_mcp_permissions(
    enabled_mcp_ids: &[String],
    enabled_tool_prefixes: &[String],
    selected_system_context_id: Option<&str>,
) -> Result<Value, String> {
    mcp_permissions::save_mcp_permissions(
        enabled_mcp_ids,
        enabled_tool_prefixes,
        selected_system_context_id,
    )
}

pub fn import_agents_json(raw: &str) -> Result<Value, String> {
    git_import::import_agents_json(raw)
}

pub fn import_marketplace_json(raw: &str) -> Result<Value, String> {
    git_import::import_marketplace_json(raw)
}

pub fn import_from_git(opts: GitImportOptions) -> Result<Value, String> {
    git_import::import_from_git(opts)
}

pub fn install_plugins(opts: InstallPluginOptions) -> Result<Value, String> {
    plugins::install_plugins(opts)
}
