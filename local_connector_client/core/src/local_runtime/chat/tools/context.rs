// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::path::PathBuf;

use chatos_mcp_runtime::BuiltinMcpKind;
use serde_json::json;

use crate::history::CommandHistoryRecorder;
use crate::local_runtime::capabilities::resolve_local_chat_capabilities;
use crate::local_runtime::storage::{
    LocalDatabase, LocalProjectRecord, LocalRuntimeSettingsRecord,
};
use crate::local_runtime::LocalAskUserPromptRegistry;
use crate::mcp::manifest::LocalMcpManifestRecord;
use crate::mcp::tools::request_project_root;
use crate::relay::RelayRequest;
use crate::skills::PreparedLocalSkill;
use crate::{LocalRuntime, LocalState, LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER};

use super::context_selection::{
    manifest_is_selected, parse_selected_ids, selected_chat_builtin_kinds,
};

#[derive(Clone)]
pub(super) struct LocalChatToolContext {
    pub(super) request: RelayRequest,
    pub(super) state: LocalState,
    pub(super) history_recorder: CommandHistoryRecorder,
    pub(super) project_root: PathBuf,
    pub(super) builtin_kinds: Vec<BuiltinMcpKind>,
    pub(super) user_manifests: Vec<LocalMcpManifestRecord>,
    pub(super) skills: Vec<PreparedLocalSkill>,
    pub(super) capability_prompt: Option<String>,
    pub(super) database: LocalDatabase,
    pub(super) ask_user_prompts: LocalAskUserPromptRegistry,
    pub(super) auto_create_task: bool,
    pub(super) enabled: bool,
}

pub(super) async fn resolve_local_chat_tool_context(
    runtime: &LocalRuntime,
    owner_user_id: &str,
    request_id: &str,
    project: &LocalProjectRecord,
    settings: &LocalRuntimeSettingsRecord,
) -> Result<LocalChatToolContext, String> {
    let selected_ids = parse_selected_ids(settings.enabled_mcp_ids_json.as_str());
    let builtin_kinds = selected_chat_builtin_kinds(
        settings.mcp_enabled,
        settings.plan_mode_enabled,
        selected_ids.as_slice(),
    );
    let state = runtime.state.read().await.clone();
    if state.device_id.as_deref() != Some(project.device_id.as_str()) {
        return Err("Local project belongs to a different connector device".to_string());
    }

    let mut headers = BTreeMap::new();
    headers.insert(
        LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER.to_string(),
        builtin_kinds
            .iter()
            .map(|kind| kind.kind_name())
            .collect::<Vec<_>>()
            .join(","),
    );
    headers.insert(
        "x-task-runner-task-id".to_string(),
        project.project_id.clone(),
    );
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

    let request = RelayRequest {
        _message_type: "local_runtime_chat".to_string(),
        request_id: request_id.to_string(),
        owner_user_id: Some(owner_user_id.to_string()),
        device_id: Some(project.device_id.clone()),
        workspace_id: project.workspace_id.clone(),
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
    let manifest_candidates = if settings.mcp_enabled && !settings.plan_mode_enabled {
        database
            .list_mcp_manifests(owner_user_id, project.device_id.as_str())
            .await
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|manifest| {
                manifest.is_locally_executable()
                    && manifest_is_selected(manifest, selected_ids.as_slice())
            })
            .collect()
    } else {
        Vec::new()
    };
    let capabilities = resolve_local_chat_capabilities(
        &database,
        owner_user_id,
        settings,
        &state,
        &request,
        builtin_kinds,
        manifest_candidates,
    )
    .await?;
    let enabled = !capabilities.builtin_kinds.is_empty()
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
        user_manifests: capabilities.user_manifests,
        skills: capabilities.skills,
        capability_prompt: capabilities.prompt,
        database,
        ask_user_prompts: runtime.ask_user_prompts.clone(),
        auto_create_task: settings.auto_create_task,
        enabled,
    })
}
