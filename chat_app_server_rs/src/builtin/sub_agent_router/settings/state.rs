use std::fs;
use std::path::PathBuf;

use serde_json::json;

use crate::builtin::sub_agent_router::types::{AgentSpec, RegistryData};
use crate::builtin::sub_agent_router::utils::ensure_dir;

use super::types::{
    SubAgentRouterMcpPermissions, SubAgentRouterStatePaths, SUB_AGENT_ROUTER_STATE_ROOT_ENV,
};

pub(super) fn resolve_state_paths() -> SubAgentRouterStatePaths {
    let root = resolve_state_root();
    SubAgentRouterStatePaths {
        registry_path: root.join("subagents.json"),
        marketplace_path: root.join("marketplace.json"),
        plugins_root: root.join("plugins"),
        mcp_permissions_path: root.join("mcp_permissions.json"),
        root,
    }
}

pub(super) fn ensure_state_files() -> Result<SubAgentRouterStatePaths, String> {
    let paths = resolve_state_paths();
    ensure_dir(paths.root.as_path())?;
    ensure_dir(paths.plugins_root.as_path())?;

    if !paths.registry_path.exists() {
        let initial = RegistryData { agents: Vec::new() };
        let text = serde_json::to_string_pretty(&initial).map_err(|err| err.to_string())?;
        fs::write(paths.registry_path.as_path(), text).map_err(|err| err.to_string())?;
    }

    if !paths.marketplace_path.exists() {
        let initial = json!({
            "name": "sub-agent-router-marketplace",
            "plugins": []
        });
        let text = serde_json::to_string_pretty(&initial).map_err(|err| err.to_string())?;
        fs::write(paths.marketplace_path.as_path(), text).map_err(|err| err.to_string())?;
    }

    if !paths.mcp_permissions_path.exists() {
        let text = serde_json::to_string_pretty(&SubAgentRouterMcpPermissions::default())
            .map_err(|err| err.to_string())?;
        fs::write(paths.mcp_permissions_path.as_path(), text).map_err(|err| err.to_string())?;
    }

    Ok(paths)
}

pub(super) fn parse_registry_agents(raw: &str) -> (Vec<AgentSpec>, bool) {
    if raw.trim().is_empty() {
        return (Vec::new(), false);
    }
    if let Ok(registry) = serde_json::from_str::<RegistryData>(raw) {
        return (registry.agents, true);
    }
    if let Ok(agents) = serde_json::from_str::<Vec<AgentSpec>>(raw) {
        return (agents, true);
    }
    (Vec::new(), false)
}

fn resolve_state_root() -> PathBuf {
    if let Ok(raw) = std::env::var(SUB_AGENT_ROUTER_STATE_ROOT_ENV) {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }

    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".chatos").join("builtin_sub_agent_router")
}
