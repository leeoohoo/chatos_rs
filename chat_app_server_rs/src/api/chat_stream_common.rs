use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::warn;

use crate::core::chat_context::{resolve_effective_user_id, resolve_system_prompt};
use crate::core::chat_runtime::{
    compose_contact_command_system_prompt, compose_contact_system_prompt, normalize_id,
    parse_contact_command_invocation, parse_implicit_command_selections_from_tools_end,
    resolve_project_runtime, ChatRuntimeMetadata,
};
use crate::core::mcp_runtime::{
    contact_agent_command_reader_server, contact_agent_plugin_reader_server,
    contact_agent_skill_reader_server, empty_mcp_server_bundle, has_any_mcp_server,
    load_mcp_servers_by_selection, normalize_mcp_ids, McpServerBundle,
};
use crate::core::mcp_tools::ToolInfo;
use crate::core::turn_runtime_snapshot::{
    build_turn_runtime_snapshot_payload, BuildTurnRuntimeSnapshotInput,
};
use crate::services::ai_client_common::AiClientCallbacks;
use crate::services::memory_server_client::{self, TurnRuntimeSnapshotSelectedCommandDto};

#[derive(Debug, Deserialize, Clone)]
pub(crate) struct ChatStreamRequest {
    #[serde(rename = "conversation_id", alias = "conversationId")]
    pub conversation_id: Option<String>,
    pub content: Option<String>,
    pub ai_model_config: Option<Value>,
    pub user_id: Option<String>,
    pub attachments: Option<Vec<Value>>,
    pub reasoning_enabled: Option<bool>,
    pub turn_id: Option<String>,
    pub contact_agent_id: Option<String>,
    pub project_id: Option<String>,
    pub project_root: Option<String>,
    pub remote_connection_id: Option<String>,
    pub mcp_enabled: Option<bool>,
    pub enabled_mcp_ids: Option<Vec<String>>,
}

pub(crate) fn validate_chat_stream_request(
    req: &ChatStreamRequest,
    require_responses: bool,
) -> Result<(), (StatusCode, Json<Value>)> {
    let conversation_id = req.conversation_id.as_deref().unwrap_or_default().trim();
    let content = req.content.as_deref().unwrap_or_default();
    let has_text_content = !content.trim().is_empty();
    let has_attachments = req
        .attachments
        .as_ref()
        .map(|items| !items.is_empty())
        .unwrap_or(false);
    if conversation_id.is_empty() || (!has_text_content && !has_attachments) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                json!({"error": "conversation_id 不能为空，且 content 与 attachments 不能同时为空"}),
            ),
        ));
    }
    if require_responses
        && req
            .ai_model_config
            .as_ref()
            .and_then(|cfg| cfg.get("supports_responses").and_then(|v| v.as_bool()))
            != Some(true)
    {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "当前模型未启用 Responses API"})),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedChatStreamContext {
    pub effective_user_id: Option<String>,
    pub contact_agent_id: Option<String>,
    pub base_system_prompt: Option<String>,
    pub contact_system_prompt: Option<String>,
    pub command_system_prompt: Option<String>,
    pub selected_commands_for_snapshot: Arc<Mutex<Vec<TurnRuntimeSnapshotSelectedCommandDto>>>,
    pub resolved_project_id: Option<String>,
    pub resolved_project_root: Option<String>,
    pub default_remote_connection_id: Option<String>,
    pub workspace_root: Option<String>,
    pub mcp_enabled: bool,
    pub enabled_mcp_ids_for_snapshot: Vec<String>,
    pub mcp_server_bundle: McpServerBundle,
    pub use_tools: bool,
    pub memory_summary_prompt: Option<String>,
}

pub(crate) async fn resolve_chat_stream_context(
    session_id: &str,
    content: &str,
    req: &ChatStreamRequest,
    default_system_prompt: Option<String>,
    use_active_system_context: bool,
) -> ResolvedChatStreamContext {
    let memory_session = memory_server_client::get_session_by_id(session_id)
        .await
        .ok()
        .flatten();
    let session_metadata = memory_session
        .as_ref()
        .and_then(|session| session.metadata.as_ref());
    let runtime_metadata = ChatRuntimeMetadata::from_metadata(session_metadata);

    let effective_user_id = resolve_effective_user_id(req.user_id.clone(), session_id).await;
    let mut contact_agent_id = normalize_id(req.contact_agent_id.clone())
        .or_else(|| runtime_metadata.contact_agent_id.clone())
        .or_else(|| {
            memory_session
                .as_ref()
                .and_then(|session| normalize_id(session.selected_agent_id.clone()))
        });

    if contact_agent_id.is_none() {
        if let Some(contact_id) = runtime_metadata.contact_id.as_deref() {
            if let Ok(contacts) = memory_server_client::list_memory_contacts(
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
        Some(agent_id) => {
            match memory_server_client::get_memory_agent_runtime_context(agent_id).await {
                Ok(value) => value,
                Err(err) => {
                    warn!(
                        "load contact runtime context failed: session_id={} contact_agent_id={} detail={}",
                        session_id, agent_id, err
                    );
                    None
                }
            }
        }
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
    let contact_system_prompt = compose_contact_system_prompt(contact_runtime_context.as_ref());
    let selected_command =
        parse_contact_command_invocation(content, contact_runtime_context.as_ref());
    let command_system_prompt = compose_contact_command_system_prompt(selected_command.as_ref());
    let selected_commands_for_snapshot = Arc::new(Mutex::new(seed_selected_commands(
        selected_command.as_ref(),
    )));

    let requested_project_id = normalize_id(req.project_id.clone())
        .or_else(|| runtime_metadata.project_id.clone())
        .or_else(|| {
            memory_session
                .as_ref()
                .and_then(|session| normalize_id(session.project_id.clone()))
        });
    let requested_project_root =
        normalize_id(req.project_root.clone()).or_else(|| runtime_metadata.project_root.clone());
    let (resolved_project_id, resolved_project_root) = resolve_project_runtime(
        effective_user_id.as_deref(),
        requested_project_id,
        requested_project_root,
    )
    .await;

    let requested_mcp_ids = req
        .enabled_mcp_ids
        .clone()
        .unwrap_or_else(|| runtime_metadata.enabled_mcp_ids.clone());
    let normalized_mcp_ids = normalize_mcp_ids(&requested_mcp_ids);
    let enabled_mcp_ids_for_snapshot = normalized_mcp_ids.clone();
    let default_remote_connection_id = normalize_id(req.remote_connection_id.clone())
        .or_else(|| runtime_metadata.remote_connection_id.clone());
    let workspace_root = runtime_metadata.workspace_root.clone();
    let mcp_enabled = req
        .mcp_enabled
        .or(runtime_metadata.mcp_enabled)
        .unwrap_or(true);

    let (http_servers, stdio_servers, mut builtin_servers) = if mcp_enabled {
        load_mcp_servers_by_selection(
            effective_user_id.clone(),
            !normalized_mcp_ids.is_empty(),
            normalized_mcp_ids,
            resolved_project_root.as_deref(),
            resolved_project_id.as_deref(),
        )
        .await
    } else {
        empty_mcp_server_bundle()
    };

    if let Some(agent_id) = contact_runtime_context
        .as_ref()
        .map(|context| context.agent_id.as_str())
    {
        if let Some(server) = contact_agent_skill_reader_server(
            effective_user_id.clone(),
            resolved_project_id.clone(),
            agent_id,
        ) {
            builtin_servers.push(server);
        }
        if let Some(server) = contact_agent_command_reader_server(
            effective_user_id.clone(),
            resolved_project_id.clone(),
            agent_id,
        ) {
            builtin_servers.push(server);
        }
        if let Some(server) = contact_agent_plugin_reader_server(
            effective_user_id.clone(),
            resolved_project_id.clone(),
            agent_id,
        ) {
            builtin_servers.push(server);
        }
    }
    for server in &mut builtin_servers {
        server.remote_connection_id = default_remote_connection_id.clone();
    }

    let use_tools = has_any_mcp_server(&http_servers, &stdio_servers, &builtin_servers);
    let memory_summary_prompt = memory_server_client::compose_context(session_id, 2)
        .await
        .ok()
        .and_then(|payload| payload.0)
        .and_then(|value| normalize_optional_text(Some(value.as_str())));

    ResolvedChatStreamContext {
        effective_user_id,
        contact_agent_id,
        base_system_prompt,
        contact_system_prompt,
        command_system_prompt,
        selected_commands_for_snapshot,
        resolved_project_id,
        resolved_project_root,
        default_remote_connection_id,
        workspace_root,
        mcp_enabled,
        enabled_mcp_ids_for_snapshot,
        mcp_server_bundle: (http_servers, stdio_servers, builtin_servers),
        use_tools,
        memory_summary_prompt,
    }
}

pub(crate) fn build_prefixed_messages(
    contact_system_prompt: Option<&str>,
    command_system_prompt: Option<&str>,
) -> Option<Vec<Value>> {
    let mut prefixed_messages_items = Vec::new();
    if let Some(prompt) = normalize_optional_text(contact_system_prompt) {
        prefixed_messages_items.push(json!({
            "role": "system",
            "content": prompt,
        }));
    }
    if let Some(prompt) = normalize_optional_text(command_system_prompt) {
        prefixed_messages_items.push(json!({
            "role": "system",
            "content": prompt,
        }));
    }
    if prefixed_messages_items.is_empty() {
        None
    } else {
        Some(prefixed_messages_items)
    }
}

pub(crate) fn build_prefixed_input_items(
    contact_system_prompt: Option<&str>,
    command_system_prompt: Option<&str>,
) -> Option<Vec<Value>> {
    let mut prefixed_input_items = Vec::new();
    if let Some(prompt) = normalize_optional_text(contact_system_prompt) {
        prefixed_input_items.push(json!({
            "type": "message",
            "role": "system",
            "content": [{ "type": "input_text", "text": prompt }],
        }));
    }
    if let Some(prompt) = normalize_optional_text(command_system_prompt) {
        prefixed_input_items.push(json!({
            "type": "message",
            "role": "system",
            "content": [{ "type": "input_text", "text": prompt }],
        }));
    }
    if prefixed_input_items.is_empty() {
        None
    } else {
        Some(prefixed_input_items)
    }
}

pub(crate) fn wire_implicit_command_tracking(
    callbacks: &mut AiClientCallbacks,
    selected_commands_for_snapshot: Arc<Mutex<Vec<TurnRuntimeSnapshotSelectedCommandDto>>>,
) {
    let original_on_tools_end = callbacks.on_tools_end.clone();
    callbacks.on_tools_end = Some(Arc::new(move |result: Value| {
        let implicit_items = parse_implicit_command_selections_from_tools_end(&result);
        if !implicit_items.is_empty() {
            if let Ok(mut snapshot_items) = selected_commands_for_snapshot.lock() {
                for item in implicit_items {
                    snapshot_items.push(TurnRuntimeSnapshotSelectedCommandDto {
                        command_ref: item.command_ref,
                        name: item.name,
                        plugin_source: item.plugin_source,
                        source_path: item.source_path,
                        trigger: Some("implicit".to_string()),
                        arguments: None,
                    });
                }
            }
        }
        if let Some(callback) = original_on_tools_end.as_ref() {
            callback(result);
        }
    }));
}

pub(crate) async fn sync_chat_turn_snapshot(
    session_id: &str,
    turn_id: &str,
    status: &str,
    user_message_id: Option<String>,
    model: &str,
    provider: &str,
    tool_metadata: &HashMap<String, ToolInfo>,
    context: &ResolvedChatStreamContext,
) -> Result<(), String> {
    let selected_commands = context
        .selected_commands_for_snapshot
        .lock()
        .map(|items| items.clone())
        .unwrap_or_default();
    let payload = build_turn_runtime_snapshot_payload(BuildTurnRuntimeSnapshotInput {
        user_message_id,
        status,
        base_system_prompt: context.base_system_prompt.as_deref(),
        contact_system_prompt: context.contact_system_prompt.as_deref(),
        memory_summary_prompt: context.memory_summary_prompt.as_deref(),
        tools: tool_metadata,
        model: Some(model),
        provider: Some(provider),
        contact_agent_id: context.contact_agent_id.as_deref(),
        remote_connection_id: context.default_remote_connection_id.as_deref(),
        project_id: context.resolved_project_id.as_deref(),
        project_root: context.resolved_project_root.as_deref(),
        workspace_root: context.workspace_root.as_deref(),
        mcp_enabled: context.mcp_enabled,
        enabled_mcp_ids: &context.enabled_mcp_ids_for_snapshot,
        selected_commands: selected_commands.as_slice(),
    });
    memory_server_client::sync_turn_runtime_snapshot(session_id, turn_id, &payload)
        .await
        .map(|_| ())
}

fn seed_selected_commands(
    selected_command: Option<&crate::core::chat_runtime::ParsedContactCommandInvocation>,
) -> Vec<TurnRuntimeSnapshotSelectedCommandDto> {
    selected_command
        .map(|command| {
            vec![TurnRuntimeSnapshotSelectedCommandDto {
                command_ref: Some(command.command_ref.clone()),
                name: Some(command.name.clone()),
                plugin_source: command.plugin_source.clone(),
                source_path: command.source_path.clone(),
                trigger: Some("explicit".to_string()),
                arguments: command.arguments.clone(),
            }]
        })
        .unwrap_or_default()
}

fn normalize_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}
