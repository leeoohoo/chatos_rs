// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_agent::CHATOS_PLAN_TASK_PROFILE;
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
mod internal_skills;
mod plugins;
mod system_mcps;

use agent_bindings::seed_agent_bindings;
#[cfg(test)]
use agent_bindings::{
    binding_matches_seed_variant, task_runner_cloud_plan_phase_builtin_kinds,
    task_runner_cloud_plan_phase_required, task_runner_cloud_run_phase_optional_builtin_kinds,
};
use agent_prompts::{backfill_agent_prompt_versions, seed_agent_prompts};
#[cfg(test)]
use agents::system_agent_specs;
use agents::{remove_retired_system_agents, seed_agents};
use internal_skills::{internal_skill_catalog, seed_internal_skills};
use plugins::{seed_bundled_plugins, BUNDLED_PONYTAIL_AGENT_KEYS, BUNDLED_PONYTAIL_PLUGIN_ID};
#[cfg(test)]
use system_mcps::{
    builtin_kinds, provider_skills_for_builtin_mcp, provider_skills_for_system_mcp,
    system_mcp_record,
};
use system_mcps::{builtin_resource_id, remove_retired_system_mcps, seed_system_mcps};

pub use chatos_plugin_management_sdk::{
    CHATOS_TASK_RUNNER_MCP_RESOURCE_ID, LOCAL_CONNECTOR_APPROVAL_MCP_RESOURCE_ID,
    PROJECT_ENVIRONMENT_MCP_RESOURCE_ID, PROJECT_RUNTIME_ENVIRONMENT_MCP_RESOURCE_ID,
    SANDBOX_IMAGES_MCP_RESOURCE_ID, TASK_PROCESS_LOG_MCP_RESOURCE_ID,
};
const CHATOS_TASK_RUNNER_DEFAULT_TOOL_ALLOWLIST: &[&str] = &[
    "list_tasks",
    "get_task",
    "get_task_stats",
    "create_task",
    "wait_for_task_completion",
];
const CHATOS_TASK_RUNNER_PLAN_TOOL_ALLOWLIST: &[&str] = &[
    "list_tasks",
    "get_task",
    "get_task_stats",
    "create_task",
    "create_tasks_with_prerequisites",
    "wait_for_task_completion",
];
const PROJECT_MANAGEMENT_AGENT_SANDBOX_TOOL_ALLOWLIST: &[&str] =
    &["get_image_catalog", "search_images"];
const CHATOS_CONVERSATION_AGENT_KEY: &str = SystemAgentKey::ChatosConversationAgent.as_str();
const CHATOS_LOCAL_CONVERSATION_AGENT_KEY: &str =
    SystemAgentKey::ChatosLocalConversationAgent.as_str();
const PROJECT_REQUIREMENT_EXECUTION_PLANNER_AGENT_KEY: &str =
    SystemAgentKey::ProjectRequirementExecutionPlannerAgent.as_str();
const PROJECT_REQUIREMENT_EXECUTION_LOCAL_PLANNER_AGENT_KEY: &str =
    SystemAgentKey::ProjectRequirementExecutionLocalPlannerAgent.as_str();
const TASK_RUNNER_PLAN_AGENT_KEY: &str = SystemAgentKey::TaskRunnerPlanPhase.as_str();
const TASK_RUNNER_RUN_AGENT_KEY: &str = SystemAgentKey::TaskRunnerRunPhase.as_str();
const PROJECT_MANAGEMENT_AGENT_KEY: &str = SystemAgentKey::ProjectManagementAgent.as_str();
const PROJECT_MANAGEMENT_LOCAL_AGENT_KEY: &str =
    SystemAgentKey::ProjectManagementLocalAgent.as_str();
const LOCAL_CONNECTOR_COMMAND_APPROVAL_AGENT_KEY: &str =
    SystemAgentKey::LocalConnectorCommandApprovalAgent.as_str();
const RETIRED_SYSTEM_AGENT_KEYS: &[&str] = &[
    "chatos_plan_agent",
    "chatos_planning_agent",
    "chatos_async_planner",
    "chatos_chat_runtime",
    "project_environment_agent",
    "local_connector_client_agent",
    "memory_engine_context_agent",
];
const CHATOS_NOTEPAD_AGENT_KEYS: &[&str] = &[
    CHATOS_CONVERSATION_AGENT_KEY,
    CHATOS_LOCAL_CONVERSATION_AGENT_KEY,
    PROJECT_REQUIREMENT_EXECUTION_PLANNER_AGENT_KEY,
    PROJECT_REQUIREMENT_EXECUTION_LOCAL_PLANNER_AGENT_KEY,
];
const CHATOS_TASK_RUNNER_AGENT_KEYS: &[&str] = &[
    CHATOS_CONVERSATION_AGENT_KEY,
    CHATOS_LOCAL_CONVERSATION_AGENT_KEY,
];
const TASK_RUNNER_PHASE_AGENT_KEYS: &[&str] =
    &[TASK_RUNNER_PLAN_AGENT_KEY, TASK_RUNNER_RUN_AGENT_KEY];
const PROJECT_MANAGEMENT_AGENT_REQUIRED_MCPS: &[(&str, i64)] = &[
    (PROJECT_ENVIRONMENT_MCP_RESOURCE_ID, 20),
    (SANDBOX_IMAGES_MCP_RESOURCE_ID, 30),
];

pub async fn seed_system_resources(store: &AppStore, admin_user_id: &str) -> Result<(), String> {
    remove_retired_system_agents(store).await?;
    remove_retired_system_mcps(store).await?;
    seed_system_mcps(store, admin_user_id).await?;
    seed_internal_skills(store, admin_user_id).await?;
    seed_bundled_plugins(store, admin_user_id).await?;
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
