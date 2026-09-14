// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_plugin_management_sdk::{SystemAgentKey, SystemMcpKey};
use serde_json::Value;

use crate::models::*;
use crate::store::{now_rfc3339, AppStore};
use crate::system_mcp_catalog::{
    system_mcp_catalog, system_mcp_descriptor, system_mcp_provider_skills, system_mcp_tool_catalog,
    SystemMcpDescriptor, SystemMcpToolCatalog,
};

mod agent_bindings;
mod agent_prompts;
mod agents;
mod system_mcps;

use agent_bindings::seed_agent_bindings;
#[cfg(test)]
use agent_bindings::task_runner_run_phase_optional_builtin_kinds;
pub(crate) use agent_prompts::agent_prompt_profiles_for_agent;
use agent_prompts::{backfill_agent_prompt_versions, seed_agent_prompts};
#[cfg(test)]
use agents::system_agent_specs;
use agents::{remove_retired_system_agents, seed_agents};
#[cfg(test)]
use system_mcps::{
    active_system_mcp_keys, provider_skills_for_system_key, provider_skills_for_system_mcp,
    system_mcp_record,
};
use system_mcps::{remove_retired_system_mcps, seed_system_mcps, system_mcp_resource_id};

pub use chatos_plugin_management_sdk::{
    LOCAL_CONNECTOR_APPROVAL_MCP_RESOURCE_ID, TASK_PROCESS_LOG_MCP_RESOURCE_ID,
};
const CHATOS_CONVERSATION_AGENT_KEY: &str = SystemAgentKey::ChatosConversationAgent.as_str();
const TASK_RUNNER_RUN_AGENT_KEY: &str = SystemAgentKey::TaskRunnerRunPhase.as_str();
const LOCAL_CONNECTOR_COMMAND_APPROVAL_AGENT_KEY: &str =
    SystemAgentKey::LocalConnectorCommandApprovalAgent.as_str();
const RETIRED_SYSTEM_AGENT_KEYS: &[&str] = &[
    "task_runner_plan_phase",
    "chatos_plan_agent",
    "chatos_planning_agent",
    "chatos_async_planner",
    "chatos_chat_runtime",
    "chatos_local_conversation_agent",
    "project_requirement_execution_local_planner_agent",
    "project_requirement_execution_planner_agent",
    "task_runner_local_plan_phase",
    "task_runner_local_run_phase",
    "project_environment_agent",
    "local_connector_client_agent",
    "memory_engine_context_agent",
];
const TASK_RUNNER_PHASE_AGENT_KEYS: &[&str] = &[TASK_RUNNER_RUN_AGENT_KEY];
pub async fn seed_system_resources(store: &AppStore, admin_user_id: &str) -> Result<(), String> {
    remove_retired_system_agents(store).await?;
    remove_retired_system_mcps(store).await?;
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
