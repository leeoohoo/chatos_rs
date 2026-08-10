// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[path = "runtime_context/mcp_management_gateway.rs"]
mod mcp_management_gateway;
#[path = "runtime_context/policy.rs"]
mod policy;
#[path = "runtime_context/support.rs"]
mod support;
#[path = "runtime_context/task_runner.rs"]
mod task_runner;
#[path = "runtime_context/workspace.rs"]
mod workspace;

use std::sync::{Arc, Mutex};

use chatos_agent::ChatosAgentProfile;
use chatos_mcp_management_sdk::McpManagementRuntimeSessionHandle;
use chatos_plugin_management_sdk::PluginCommandInvocation;
use sha2::{Digest, Sha256};
use tracing::warn;

use self::mcp_management_gateway::{resolve_mcp_management_gateway, McpManagementGatewayRequest};
use self::policy::merge_optional_system_prompts;
use self::support::{is_concrete_project_id, normalize_optional_text};
use self::task_runner::{normalize_plugin_command_invocations, normalize_selected_plugin_ids};
use self::workspace::{authorize_runtime_workspace_dir, resolve_runtime_project_root};
use crate::core::builtin_mcp_prompt::compose_builtin_mcp_system_prompt;
use crate::core::chat_context::resolve_system_prompt;
use crate::core::chat_runtime::{
    compose_contact_system_prompt, normalize_id, resolve_project_runtime_context,
    ChatRuntimeMetadata, ContactSkillPromptMode,
};
use crate::core::internal_context_locale::InternalContextLocale;
use crate::core::mcp_runtime::{empty_mcp_server_bundle, McpServerBundle};
use crate::core::mcp_tools::ToolInfo;
use crate::models::memory_runtime_types::{
    TurnRuntimeSnapshotPluginCommandInvocationDto, TurnRuntimeSnapshotSelectedCommandDto,
};
use crate::models::project::PUBLIC_PROJECT_ID;
use crate::services::{
    chatos_agents, chatos_memory_engine, chatos_memory_mappings, chatos_sessions,
    plugin_management_prompts,
};
#[derive(Debug, Clone)]
pub struct ConversationRuntimeRequest {
    pub effective_user_id: Option<String>,
    pub contact_agent_id: Option<String>,
    pub project_id: Option<String>,
    pub project_root: Option<String>,
    pub workspace_root: Option<String>,
    pub remote_connection_id: Option<String>,
    pub selected_plugin_ids: Vec<String>,
    pub plugin_command_invocations: Vec<PluginCommandInvocation>,
    pub plan_mode: bool,
    pub project_requirement_execution_planner: bool,
    pub project_requirement_execution_task_ids: Vec<String>,
    pub model_config_id: Option<String>,
    pub model_provider: String,
    pub prompt_vendor: Option<String>,
    pub conversation_turn_id: Option<String>,
    pub source_user_message_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedConversationRuntimeContext {
    pub agent_profile: ChatosAgentProfile,
    pub internal_context_locale: InternalContextLocale,
    pub user_output_locale: InternalContextLocale,
    pub contact_agent_id: Option<String>,
    pub base_system_prompt: Option<String>,
    pub agent_system_prompt: Option<String>,
    pub contact_system_prompt: Option<String>,
    pub builtin_mcp_system_prompt: Option<String>,
    pub selected_commands_for_snapshot: Arc<Mutex<Vec<TurnRuntimeSnapshotSelectedCommandDto>>>,
    pub plugin_command_invocations_for_snapshot: Vec<TurnRuntimeSnapshotPluginCommandInvocationDto>,
    pub resolved_project_id: Option<String>,
    pub resolved_project_name: Option<String>,
    pub resolved_project_root: Option<String>,
    pub default_remote_connection_id: Option<String>,
    pub workspace_root: Option<String>,
    pub mcp_enabled: bool,
    pub enabled_mcp_ids_for_snapshot: Vec<String>,
    pub mcp_server_bundle: McpServerBundle,
    pub mcp_management_runtime_session: Option<McpManagementRuntimeSessionHandle>,
    pub use_tools: bool,
    pub memory_summary_prompt: Option<String>,
    pub runtime_error: Option<String>,
    pub project_requirement_execution_planner: bool,
}

pub type ToolMetadataMap = std::collections::HashMap<String, ToolInfo>;

pub async fn resolve_runtime_context(
    session_id: &str,
    _content: &str,
    req: &ConversationRuntimeRequest,
    default_system_prompt: Option<String>,
    use_active_system_context: bool,
    internal_context_locale: InternalContextLocale,
    user_output_locale: InternalContextLocale,
) -> ResolvedConversationRuntimeContext {
    let memory_session = chatos_sessions::get_session_by_id(session_id)
        .await
        .ok()
        .flatten();
    let session_metadata = memory_session
        .as_ref()
        .and_then(|session| session.metadata.as_ref());
    let runtime_metadata = ChatRuntimeMetadata::from_metadata(session_metadata);

    let effective_user_id = req.effective_user_id.clone();
    let mut contact_agent_id = normalize_id(req.contact_agent_id.clone())
        .or_else(|| runtime_metadata.contact_agent_id.clone())
        .or_else(|| {
            memory_session
                .as_ref()
                .and_then(|session| normalize_id(session.selected_agent_id.clone()))
        });

    if contact_agent_id.is_none() {
        if let Some(contact_id) = runtime_metadata.contact_id.as_deref() {
            if let Ok(contacts) = chatos_memory_mappings::list_memory_contacts(
                effective_user_id.as_deref(),
                Some(500),
                0,
            )
            .await
            {
                if let Some(contact) = contacts.iter().find(|item| item.id.trim() == contact_id) {
                    contact_agent_id = normalize_id(Some(contact.agent_id.clone()));
                    if let Some(agent_id) = contact_agent_id.as_deref() {
                        warn!(
                            "resolved contact_agent_id from contact_id: session_id={} contact_id={} contact_agent_id={}",
                            session_id, contact_id, agent_id
                        );
                    }
                }
            }
        }
    }

    let contact_runtime_context = match contact_agent_id.as_deref() {
        Some(agent_id) => match chatos_agents::get_agent_runtime_context(agent_id).await {
            Ok(value) => value,
            Err(err) => {
                warn!(
                    "load contact runtime context failed: session_id={} contact_agent_id={} detail={}",
                    session_id, agent_id, err
                );
                None
            }
        },
        None => None,
    };
    if contact_agent_id.is_some() && contact_runtime_context.is_none() {
        warn!(
            "contact runtime context missing: session_id={} contact_agent_id={}",
            session_id,
            contact_agent_id.as_deref().unwrap_or_default()
        );
    }

    let base_system_prompt = resolve_system_prompt(
        default_system_prompt,
        use_active_system_context,
        effective_user_id.clone(),
    )
    .await;
    let mut contact_system_prompt = compose_contact_system_prompt(
        contact_runtime_context.as_ref(),
        &ContactSkillPromptMode::Disabled,
        internal_context_locale,
    );
    let selected_commands_for_snapshot = Arc::new(Mutex::new(Vec::new()));

    let requested_project_id = normalize_id(req.project_id.clone())
        .or_else(|| runtime_metadata.project_id.clone())
        .or_else(|| {
            memory_session
                .as_ref()
                .and_then(|session| normalize_id(session.project_id.clone()))
        });
    let requested_project_root =
        normalize_id(req.project_root.clone()).or_else(|| runtime_metadata.project_root.clone());
    let resolved_project_runtime = resolve_project_runtime_context(
        effective_user_id.as_deref(),
        requested_project_id,
        requested_project_root,
    )
    .await;
    let is_local_project = resolved_project_runtime.is_local_project;
    let resolved_project_id = resolved_project_runtime.project_id;
    let resolved_project_name = resolved_project_runtime.project_name;
    let resolved_project_root =
        resolve_runtime_project_root(resolved_project_runtime.project_root).await;
    let resolved_project_root = resolved_project_root.logical_root;

    let default_remote_connection_id = normalize_id(req.remote_connection_id.clone())
        .or_else(|| runtime_metadata.remote_connection_id.clone());
    let workspace_root = normalize_id(req.workspace_root.clone())
        .or_else(|| runtime_metadata.workspace_root.clone());
    let workspace_root =
        authorize_runtime_workspace_dir(effective_user_id.as_deref(), workspace_root).await;

    let (mut http_servers, stdio_servers, builtin_servers) = empty_mcp_server_bundle();
    let mut runtime_error = None;
    let mut effective_mcp_resource_ids = Vec::new();
    let mut gateway_provider_skills_prompt = None;
    let mut mcp_management_runtime_session = None;
    let agent_profile = ChatosAgentProfile::from_project_locality(
        req.plan_mode,
        req.project_requirement_execution_planner,
        is_local_project,
    );
    let normalized_selected_plugin_ids =
        normalize_selected_plugin_ids(req.selected_plugin_ids.as_slice());
    let normalized_plugin_command_invocations = normalize_plugin_command_invocations(
        normalized_selected_plugin_ids.as_slice(),
        req.plugin_command_invocations.as_slice(),
    );
    let plugin_command_invocations_for_snapshot = normalized_plugin_command_invocations
        .iter()
        .map(|invocation| TurnRuntimeSnapshotPluginCommandInvocationDto {
            plugin_id: invocation.plugin_id.clone(),
            command_id: invocation.command_id.clone(),
            arguments_present: invocation.arguments.is_some(),
            arguments_sha256: invocation
                .arguments
                .as_deref()
                .map(|arguments| hex::encode(Sha256::digest(arguments.as_bytes()))),
        })
        .collect::<Vec<_>>();

    let agent_system_prompt = match plugin_management_prompts::resolve_for_model(
        agent_profile.key(),
        req.prompt_vendor.as_deref(),
        req.model_provider.as_str(),
    )
    .await
    {
        Ok(prompt) => Some(prompt.content),
        Err(err) => {
            runtime_error = Some(format!("系统智能体 Prompt 不可用：{err}"));
            None
        }
    };

    let requires_concrete_project = agent_profile.requires_concrete_project();
    let task_runner_project_id = if requires_concrete_project {
        resolved_project_id
            .as_deref()
            .filter(|project_id| is_concrete_project_id(project_id))
    } else {
        resolved_project_id.as_deref().or(Some(PUBLIC_PROJECT_ID))
    };
    if requires_concrete_project && task_runner_project_id.is_none() {
        runtime_error = Some("当前智能体运行需要先选择一个有效项目。".to_string());
    }

    let mcp_management_gateway = if runtime_error.is_none() {
        match resolve_mcp_management_gateway(McpManagementGatewayRequest {
            tenant_id: effective_user_id.as_deref(),
            owner_user_id: effective_user_id.as_deref(),
            agent_profile,
            project_id: task_runner_project_id,
            source_session_id: Some(session_id),
            turn_id: req.conversation_turn_id.as_deref(),
            source_user_message_id: req.source_user_message_id.as_deref(),
            contact_agent_id: contact_agent_id.as_deref(),
            default_model_config_id: req.model_config_id.as_deref(),
            expected_project_task_ids: req.project_requirement_execution_task_ids.as_slice(),
            locale: Some(if user_output_locale.is_english() {
                InternalContextLocale::ENGLISH_KEY
            } else {
                InternalContextLocale::DEFAULT_KEY
            }),
        })
        .await
        {
            Ok(gateway) => Some(gateway),
            Err(err) => {
                runtime_error = Some(format!("MCP Management 运行会话不可用：{err}"));
                None
            }
        }
    } else {
        None
    };

    if let Some(gateway) = mcp_management_gateway {
        let (server, effective_mcp_ids, provider_skills_prompt, runtime_session) =
            gateway.into_parts();
        http_servers.push(server);
        effective_mcp_resource_ids = effective_mcp_ids;
        gateway_provider_skills_prompt = provider_skills_prompt;
        mcp_management_runtime_session = Some(runtime_session);
    }

    if runtime_error.is_none() {
        contact_system_prompt =
            merge_optional_system_prompts(contact_system_prompt, gateway_provider_skills_prompt);
    }

    let enabled_mcp_ids_for_snapshot = effective_mcp_resource_ids;
    let builtin_mcp_system_prompt =
        compose_builtin_mcp_system_prompt(builtin_servers.as_slice(), internal_context_locale);
    let use_tools =
        !http_servers.is_empty() || !stdio_servers.is_empty() || !builtin_servers.is_empty();
    let memory_summary_prompt = match memory_session.as_ref() {
        Some(session) => chatos_memory_engine::compose_chatos_context(session, true)
            .await
            .ok()
            .and_then(|payload| payload.merged_summary)
            .and_then(|value| normalize_optional_text(Some(value.as_str()))),
        None => None,
    };

    ResolvedConversationRuntimeContext {
        agent_profile,
        internal_context_locale,
        user_output_locale,
        contact_agent_id,
        base_system_prompt,
        agent_system_prompt,
        contact_system_prompt,
        builtin_mcp_system_prompt,
        selected_commands_for_snapshot,
        plugin_command_invocations_for_snapshot,
        resolved_project_id,
        resolved_project_name,
        resolved_project_root,
        default_remote_connection_id,
        workspace_root,
        mcp_enabled: true,
        enabled_mcp_ids_for_snapshot,
        mcp_server_bundle: (http_servers, stdio_servers, builtin_servers),
        mcp_management_runtime_session,
        use_tools,
        memory_summary_prompt,
        runtime_error,
        project_requirement_execution_planner: req.project_requirement_execution_planner,
    }
}

#[cfg(test)]
#[path = "runtime_context/plugin_policy_tests.rs"]
mod plugin_policy_tests;
