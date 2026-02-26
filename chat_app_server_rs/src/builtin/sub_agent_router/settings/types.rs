use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub(super) const SUB_AGENT_ROUTER_STATE_ROOT_ENV: &str = "SUB_AGENT_ROUTER_STATE_ROOT";
pub(super) const RECOMMENDER_REFERENCE_DOCS_DIR: &str = "reference_docs";
pub(super) const RECOMMENDER_AGENTS_DOC_FILE: &str = "agents.md";
pub(super) const RECOMMENDER_SKILLS_DOC_FILE: &str = "agent-skills.md";

#[derive(Debug, Clone)]
pub struct SubAgentRouterStatePaths {
    pub root: PathBuf,
    pub registry_path: PathBuf,
    pub marketplace_path: PathBuf,
    pub plugins_root: PathBuf,
    pub mcp_permissions_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct GitImportOptions {
    pub repository: String,
    pub branch: Option<String>,
    pub agents_path: Option<String>,
    pub skills_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct InstallPluginOptions {
    pub source: Option<String>,
    pub install_all: bool,
}

#[derive(Debug, Default, Clone)]
pub(super) struct DiscoveredPluginEntries {
    pub exists: bool,
    pub agents: Vec<String>,
    pub skills: Vec<String>,
    pub commands: Vec<String>,
}

#[derive(Debug, Default)]
pub(super) struct ParsedMarketplaceItems {
    pub plugins: Vec<Value>,
    pub agents: Vec<Value>,
    pub skills: Vec<Value>,
    pub discovered_agents: usize,
    pub discovered_skills: usize,
    pub discovered_commands: usize,
    pub installable_plugins: usize,
    pub valid: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct SubAgentRouterMcpPermissions {
    pub configured: bool,
    pub enabled_mcp_ids: Vec<String>,
    pub enabled_tool_prefixes: Vec<String>,
    pub selected_system_context_id: Option<String>,
    pub updated_at: String,
}

impl Default for SubAgentRouterMcpPermissions {
    fn default() -> Self {
        Self {
            configured: false,
            enabled_mcp_ids: Vec::new(),
            enabled_tool_prefixes: Vec::new(),
            selected_system_context_id: None,
            updated_at: String::new(),
        }
    }
}
