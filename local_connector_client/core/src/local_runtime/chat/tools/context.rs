// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::path::PathBuf;

use chatos_mcp_runtime::BuiltinMcpKind;
use chatos_plugin_management_sdk::{SystemAgentKey, SystemMcpKey};
use serde_json::json;

use crate::history::CommandHistoryRecorder;
use crate::local_runtime::capabilities::resolve_local_chat_capabilities;
use crate::local_runtime::storage::{
    LocalDatabase, LocalProjectRecord, LocalRuntimeSettingsRecord,
};
use crate::local_runtime::LocalAskUserPromptRegistry;
use crate::local_runtime::{
    local_unscoped_workspace_root, LOCAL_UNSCOPED_PROJECT_ID, LOCAL_UNSCOPED_WORKSPACE_ID,
};
use crate::mcp::manifest::LocalMcpManifestRecord;
use crate::mcp::tools::request_project_root;
use crate::relay::RelayRequest;
use crate::skills::PreparedLocalSkill;
use crate::{
    LocalRuntime, LocalState, WorkspaceState, LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER,
};

#[derive(Clone)]
pub(super) struct LocalChatToolContext {
    pub(super) request: RelayRequest,
    pub(super) state: LocalState,
    pub(super) history_recorder: CommandHistoryRecorder,
    pub(super) project_root: PathBuf,
    pub(super) builtin_kinds: Vec<BuiltinMcpKind>,
    pub(super) host_system_mcps: Vec<SystemMcpKey>,
    pub(super) user_manifests: Vec<LocalMcpManifestRecord>,
    pub(super) skills: Vec<PreparedLocalSkill>,
    pub(super) capability_prompt: Option<String>,
    pub(super) database: LocalDatabase,
    pub(super) ask_user_prompts: LocalAskUserPromptRegistry,
    pub(super) auto_create_task: bool,
    pub(super) session_id: String,
    pub(super) source_turn_id: String,
    pub(super) default_model_config_id: Option<String>,
    pub(super) enabled: bool,
}

pub(super) async fn resolve_local_chat_tool_context(
    runtime: &LocalRuntime,
    owner_user_id: &str,
    request_id: &str,
    project: &LocalProjectRecord,
    settings: &LocalRuntimeSettingsRecord,
    agent_key: SystemAgentKey,
    include_all_configured: bool,
) -> Result<LocalChatToolContext, String> {
    let mut state = runtime.state.read().await.clone();
    if state.device_id.as_deref() != Some(project.device_id.as_str()) {
        return Err("Local project belongs to a different connector device".to_string());
    }
    let unscoped = project.project_id == LOCAL_UNSCOPED_PROJECT_ID;
    if unscoped {
        let root = local_unscoped_workspace_root(runtime.state_path.as_path());
        std::fs::create_dir_all(root.as_path()).map_err(|error| {
            format!(
                "Create local unscoped workspace failed ({}): {error}",
                root.display()
            )
        })?;
        state
            .workspaces
            .retain(|workspace| workspace.id != LOCAL_UNSCOPED_WORKSPACE_ID);
        state.workspaces.push(WorkspaceState {
            id: LOCAL_UNSCOPED_WORKSPACE_ID.to_string(),
            absolute_root: root,
            alias: "ChatOS Local Contacts".to_string(),
            fingerprint: LOCAL_UNSCOPED_WORKSPACE_ID.to_string(),
            project_config_trust: None,
        });
    }

    let mut headers = BTreeMap::new();
    headers.insert(
        "x-task-runner-task-id".to_string(),
        project.project_id.clone(),
    );
    headers.insert("x-task-runner-run-id".to_string(), request_id.to_string());
    if let Some(relative_path) = project
        .root_relative_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != ".")
    {
        headers.insert(
            "x-local-connector-cwd".to_string(),
            relative_path.to_string(),
        );
    }

    let mut request = RelayRequest {
        _message_type: "local_runtime_chat".to_string(),
        request_id: request_id.to_string(),
        owner_user_id: Some(owner_user_id.to_string()),
        device_id: Some(project.device_id.clone()),
        workspace_id: if unscoped {
            LOCAL_UNSCOPED_WORKSPACE_ID.to_string()
        } else {
            project.workspace_id.clone()
        },
        method: None,
        path: None,
        headers,
        body: json!({}),
    };
    let workspace = state
        .workspace_by_id(project.workspace_id.as_str())
        .ok_or_else(|| {
            format!(
                "Local workspace is not registered on this device: {}",
                project.workspace_id
            )
        })?;
    let project_root = request_project_root(workspace, &request)
        .map_err(|error| format!("Resolve local project root failed: {error}"))?;
    let database = runtime
        .local_database()
        .map_err(|error| error.to_string())?
        .clone();
    let manifest_candidates = database
        .list_mcp_manifests(owner_user_id, project.device_id.as_str())
        .await
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter(LocalMcpManifestRecord::is_locally_executable)
        .collect();
    let capabilities = resolve_local_chat_capabilities(
        &database,
        owner_user_id,
        settings,
        &state,
        &request,
        agent_key,
        include_all_configured,
        manifest_candidates,
    )
    .await?;
    request.headers.insert(
        LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER.to_string(),
        capabilities
            .builtin_kinds
            .iter()
            .map(|kind| kind.kind_name())
            .collect::<Vec<_>>()
            .join(","),
    );
    let enabled = !capabilities.builtin_kinds.is_empty()
        || !capabilities.host_system_mcps.is_empty()
        || !capabilities.user_manifests.is_empty()
        || capabilities
            .skills
            .iter()
            .any(|skill| skill.server.is_some());

    Ok(LocalChatToolContext {
        request,
        state,
        history_recorder: CommandHistoryRecorder {
            state_path: runtime.state_path.clone(),
            state: runtime.state.clone(),
        },
        project_root,
        builtin_kinds: capabilities.builtin_kinds,
        host_system_mcps: capabilities.host_system_mcps,
        user_manifests: capabilities.user_manifests,
        skills: capabilities.skills,
        capability_prompt: capabilities.prompt,
        database,
        ask_user_prompts: runtime.ask_user_prompts.clone(),
        auto_create_task: settings.auto_create_task,
        session_id: settings.session_id.clone(),
        source_turn_id: request_id.to_string(),
        default_model_config_id: settings.selected_model_id.clone(),
        enabled,
    })
}
