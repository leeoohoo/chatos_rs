use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::{info, warn};

use crate::core::chat_context::{resolve_effective_user_id, resolve_system_prompt};
use crate::core::chat_runtime::{
    compose_contact_command_system_prompt, compose_contact_system_prompt,
    compose_contact_task_planning_prompt, normalize_id, parse_contact_command_invocation,
    parse_implicit_command_selections_from_tools_end, resolve_project_runtime,
    ChatRuntimeMetadata,
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
use crate::services::builtin_mcp::contact_chat_default_mcp_ids;
use crate::services::memory_server_client::{self, TurnRuntimeSnapshotSelectedCommandDto};
use crate::services::task_manager::list_tasks_for_context;

const CONTACT_CHAT_TASK_PLANNING_RULES: &str =
    include_str!("../../config/prompts/contact_chat_task_planning.md");

#[derive(Debug, Deserialize, Clone)]
pub(crate) struct ChatStreamRequest {
    pub session_id: Option<String>,
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
    pub execution_context: Option<bool>,
}

pub(crate) fn validate_chat_stream_request(
    req: &ChatStreamRequest,
    require_responses: bool,
) -> Result<(), (StatusCode, Json<Value>)> {
    let session_id = req.session_id.as_deref().unwrap_or_default().trim();
    let content = req.content.as_deref().unwrap_or_default();
    let has_text_content = !content.trim().is_empty();
    let has_attachments = req
        .attachments
        .as_ref()
        .map(|items| !items.is_empty())
        .unwrap_or(false);
    if session_id.is_empty() || (!has_text_content && !has_attachments) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "session_id 不能为空，且 content 与 attachments 不能同时为空"})),
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
    pub is_im_session: bool,
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
    let memory_session = if session_id.trim().is_empty() {
        None
    } else {
        memory_server_client::get_session_by_id(session_id)
            .await
            .ok()
            .flatten()
    };
    let session_metadata = memory_session
        .as_ref()
        .and_then(|session| session.metadata.as_ref());
    let is_im_session = session_metadata
        .and_then(|metadata| metadata.get("im"))
        .map(|node| {
            node.get("conversation_id")
                .or_else(|| node.get("conversationId"))
                .and_then(|value| value.as_str())
                .map(str::trim)
                .map(|value| !value.is_empty())
                .unwrap_or(false)
        })
        .unwrap_or(false);
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

    let is_contact_chat_context = req.execution_context != Some(true) && contact_agent_id.is_some();
    let contact_authorized_builtin_mcp_ids = if is_contact_chat_context {
        resolve_contact_authorized_builtin_mcp_ids(
            effective_user_id.as_deref(),
            runtime_metadata.contact_id.as_deref(),
            contact_agent_id.as_deref(),
            session_id,
        )
        .await
    } else {
        Vec::new()
    };
    let contact_task_planning_prompt = if is_contact_chat_context {
        compose_contact_task_planning_prompt(
            contact_runtime_context.as_ref(),
            contact_authorized_builtin_mcp_ids.as_slice(),
        )
    } else {
        None
    };
    let merged_task_planning_prompt = if is_contact_chat_context {
        let existing_tasks_prompt = match list_tasks_for_context(session_id, None, false, 12).await {
            Ok(tasks) => format_existing_unfinished_tasks_prompt(tasks.as_slice()),
            Err(err) => {
                warn!(
                    "load unfinished tasks for contact planning prompt failed: session_id={} detail={}",
                    session_id, err
                );
                None
            }
        };
        let runtime_task_planning_prompt = merge_prompt_sections(
            contact_task_planning_prompt.as_deref(),
            existing_tasks_prompt.as_deref(),
        );
        merge_prompt_sections(
            Some(CONTACT_CHAT_TASK_PLANNING_RULES),
            runtime_task_planning_prompt.as_deref(),
        )
    } else {
        None
    };
    let contact_system_prompt = if is_contact_chat_context {
        merge_prompt_sections(
            contact_system_prompt.as_deref(),
            merged_task_planning_prompt.as_deref(),
        )
    } else {
        contact_system_prompt
    };

    let requested_mcp_ids = req
        .enabled_mcp_ids
        .clone()
        .unwrap_or_else(|| runtime_metadata.enabled_mcp_ids.clone());
    let mcp_selection_configured = if is_contact_chat_context {
        true
    } else {
        req.enabled_mcp_ids.is_some() || !runtime_metadata.enabled_mcp_ids.is_empty()
    };
    let normalized_mcp_ids = if is_contact_chat_context {
        contact_chat_default_mcp_ids()
    } else {
        normalize_mcp_ids(&requested_mcp_ids)
    };
    let enabled_mcp_ids_for_snapshot = normalized_mcp_ids.clone();
    let default_remote_connection_id = normalize_id(req.remote_connection_id.clone())
        .or_else(|| runtime_metadata.remote_connection_id.clone());
    let workspace_root = runtime_metadata.workspace_root.clone();
    let mcp_enabled = req
        .mcp_enabled
        .or(runtime_metadata.mcp_enabled)
        .unwrap_or(true);
    let mcp_enabled = if is_contact_chat_context {
        true
    } else {
        mcp_enabled
    };

    let (http_servers, stdio_servers, mut builtin_servers) = if mcp_enabled {
        load_mcp_servers_by_selection(
            effective_user_id.clone(),
            mcp_selection_configured,
            normalized_mcp_ids,
            resolved_project_root.as_deref(),
            resolved_project_id.as_deref(),
        )
        .await
    } else {
        empty_mcp_server_bundle()
    };

    if req.execution_context != Some(true) {
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
        is_im_session,
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

pub(crate) fn build_chat_system_prompt(
    base_system_prompt: Option<&str>,
    contact_system_prompt: Option<&str>,
    command_system_prompt: Option<&str>,
) -> Option<String> {
    let merged_contact_prompt =
        merge_prompt_sections(base_system_prompt, contact_system_prompt);
    merge_prompt_sections(
        merged_contact_prompt.as_deref(),
        command_system_prompt,
    )
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

async fn resolve_contact_authorized_builtin_mcp_ids(
    effective_user_id: Option<&str>,
    contact_id: Option<&str>,
    contact_agent_id: Option<&str>,
    session_id: &str,
) -> Vec<String> {
    let has_contact_id = contact_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_some();
    let has_contact_agent_id = contact_agent_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_some();
    if !has_contact_id && !has_contact_agent_id {
        return Vec::new();
    }

    let Ok(contact) = memory_server_client::resolve_memory_contact(
        effective_user_id,
        contact_id,
        contact_agent_id,
    )
    .await
    else {
        return Vec::new();
    };

    let grants = contact
        .as_ref()
        .map(|item| item.authorized_builtin_mcp_ids.clone())
        .unwrap_or_default();

    info!(
        "resolved contact builtin MCP grants for chat context: session_id={} contact_id={} contact_agent_id={} grants={}",
        session_id,
        contact_id.unwrap_or_default(),
        contact_agent_id.unwrap_or_default(),
        grants.join(", ")
    );

    grants
}

fn merge_prompt_sections(base: Option<&str>, extra: Option<&str>) -> Option<String> {
    match (
        base.map(str::trim).filter(|value| !value.is_empty()),
        extra.map(str::trim).filter(|value| !value.is_empty()),
    ) {
        (Some(base), Some(extra)) => Some(format!("{}\n\n{}", base, extra)),
        (Some(base), None) => Some(base.to_string()),
        (None, Some(extra)) => Some(extra.to_string()),
        (None, None) => None,
    }
}

fn format_existing_unfinished_tasks_prompt(
    tasks: &[crate::services::task_manager::TaskRecord],
) -> Option<String> {
    if tasks.is_empty() {
        return None;
    }

    let mut lines = Vec::new();
    lines.push("当前上下文里已经存在未完成任务，先阅读并结合它们决定是否需要重排，而不是默认重新创建：".to_string());
    lines.push("规则：如果已有任务可以覆盖用户新增要求，优先调整、确认、暂停、恢复或停止现有任务；只有当现有任务无法承接时，才新建任务。".to_string());
    for (index, task) in tasks.iter().take(8).enumerate() {
        let mut parts = vec![
            format!("{}.", index + 1),
            format!("id={}", task.id.trim()),
            format!("status={}", task.status.trim()),
            format!("title={}", task.title.trim()),
        ];
        if let Some(task_ref) = task.task_ref.as_deref().map(str::trim).filter(|value| !value.is_empty()) {
            parts.push(format!("task_ref={}", task_ref));
        }
        if let Some(task_kind) = task
            .task_kind
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            parts.push(format!("kind={}", task_kind));
        }
        if !task.depends_on_task_ids.is_empty() {
            parts.push(format!("depends_on={}", task.depends_on_task_ids.join(",")));
        }
        if !task.verification_of_task_ids.is_empty() {
            parts.push(format!(
                "verification_of={}",
                task.verification_of_task_ids.join(",")
            ));
        }
        lines.push(parts.join(" | "));

        let details = task.details.trim();
        if !details.is_empty() {
            lines.push(format!("- details: {}", details));
        }
        if let Some(blocked_reason) = task
            .blocked_reason
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            lines.push(format!("- blocked_reason: {}", blocked_reason));
        }
        if let Some(result_summary) = task
            .result_summary
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            lines.push(format!("- latest_result_summary: {}", result_summary));
        }
    }

    Some(lines.join("\n"))
}

fn normalize_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::build_chat_system_prompt;

    #[test]
    fn build_chat_system_prompt_merges_stable_sections_in_order() {
        let prompt = build_chat_system_prompt(
            Some("base rules"),
            Some("contact rules"),
            Some("command rules"),
        );

        assert_eq!(
            prompt.as_deref(),
            Some("base rules\n\ncontact rules\n\ncommand rules")
        );
    }

    #[test]
    fn build_chat_system_prompt_skips_empty_sections() {
        let prompt = build_chat_system_prompt(Some("base rules"), Some(""), None);

        assert_eq!(prompt.as_deref(), Some("base rules"));
    }
}
