// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp::{
    system_mcp_catalog, system_mcp_provider_skills, system_mcp_tool_catalog, SystemMcpDescriptor,
    SystemMcpToolCatalog,
};
use chatos_mcp_runtime::BuiltinMcpKind;
use chatos_plugin_management_sdk::SystemAgentKey;
use serde_json::Value;

use crate::models::*;
use crate::store::{now_rfc3339, AppStore};

mod agent_bindings;
mod agent_prompts;
mod agents;
mod system_mcps;

#[cfg(test)]
use agent_bindings::local_agent_execution_optional_builtin_kinds;
use agent_bindings::seed_agent_bindings;
pub(crate) use agent_prompts::agent_prompt_profiles_for_agent;
use agent_prompts::{backfill_agent_prompt_versions, seed_agent_prompts};
use agents::seed_agents;
#[cfg(test)]
use agents::system_agent_specs;
#[cfg(test)]
use system_mcps::{
    active_system_mcp_resource_ids, builtin_kinds, provider_skills_for_builtin_mcp,
    provider_skills_for_system_mcp, system_mcp_record,
};
use system_mcps::{builtin_resource_id, seed_system_mcps};

pub use chatos_plugin_management_sdk::LOCAL_CONNECTOR_APPROVAL_MCP_RESOURCE_ID;
const CHATOS_CONVERSATION_AGENT_KEY: &str = SystemAgentKey::ChatosConversationAgent.as_str();
const LOCAL_AGENT_EXECUTION_AGENT_KEY: &str = SystemAgentKey::LocalAgentExecutionAgent.as_str();
const LOCAL_CONNECTOR_COMMAND_APPROVAL_AGENT_KEY: &str =
    SystemAgentKey::LocalConnectorCommandApprovalAgent.as_str();
pub async fn seed_system_resources(store: &AppStore, admin_user_id: &str) -> Result<(), String> {
    seed_system_mcps(store, admin_user_id).await?;
    seed_agents(store).await?;
    seed_agent_prompts(store, admin_user_id).await?;
    seed_agent_bindings(store, admin_user_id).await?;
    Ok(())
}

pub async fn ensure_agent_prompt_version_history(store: &AppStore) -> Result<(), String> {
    backfill_agent_prompt_versions(store).await
}

#[cfg(test)]
mod tests;
