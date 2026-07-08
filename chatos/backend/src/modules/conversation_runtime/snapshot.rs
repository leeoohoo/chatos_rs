// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use crate::core::builtin_mcp_prompt::{
    builtin_mcp_prompt_section_ids, builtin_mcp_prompt_source_path,
    inspect_builtin_mcp_system_prompt, inspect_effective_builtin_mcp_system_prompt,
    BuiltinMcpPromptBuildResult,
};
use crate::core::messages::join_text_lines_or_json;
use crate::core::turn_runtime_snapshot::{
    build_turn_runtime_snapshot_payload, BuildTurnRuntimeSnapshotInput,
};
use crate::models::memory_runtime_types::{
    TurnRuntimeSnapshotContextItemDto, TurnRuntimeSnapshotLookupResponseDto,
};
use crate::services::chatos_sessions;

use super::runtime_context::{ResolvedConversationRuntimeContext, ToolMetadataMap};

#[derive(Debug, Clone, Default)]
pub struct ActualTurnRequestContext {
    pub context_mode: Option<String>,
    pub items: Vec<TurnRuntimeSnapshotContextItemDto>,
    pub model_request_payload: Option<Value>,
}

#[derive(Debug, Clone)]
pub struct LiveRequestSnapshotContext {
    pub session_id: String,
    pub turn_id: String,
    pub user_message_id: String,
    pub model: String,
    pub provider: String,
    pub tool_metadata: ToolMetadataMap,
    pub unavailable_builtin_tools: Vec<Value>,
    pub runtime_context: ResolvedConversationRuntimeContext,
}

pub fn actual_context_items_from_v3_input(input: &Value) -> Vec<TurnRuntimeSnapshotContextItemDto> {
    input
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(actual_context_item_from_v3_input_item)
                .collect()
        })
        .unwrap_or_default()
}

pub async fn sync_chat_turn_snapshot(
    session_id: &str,
    turn_id: &str,
    status: &str,
    user_message_id: Option<String>,
    model: &str,
    provider: &str,
    tool_metadata: &ToolMetadataMap,
    unavailable_builtin_tools: &[Value],
    context: &ResolvedConversationRuntimeContext,
    actual_request: Option<&ActualTurnRequestContext>,
) -> Result<(), String> {
    let should_load_preserved = actual_request.is_none()
        || actual_request
            .map(|value| value.context_mode.is_none())
            .unwrap_or(false)
        || actual_request
            .map(|value| value.items.is_empty())
            .unwrap_or(false)
        || actual_request
            .map(|value| value.model_request_payload.is_none())
            .unwrap_or(false);
    let preserved_actual = if should_load_preserved {
        load_existing_actual_request_context(session_id, turn_id).await
    } else {
        None
    };
    let effective_actual_context_mode = actual_request
        .and_then(|value| value.context_mode.as_deref())
        .or_else(|| {
            preserved_actual
                .as_ref()
                .and_then(|value| value.context_mode.as_deref())
        });
    let effective_actual_items = actual_request
        .filter(|value| !value.items.is_empty())
        .map(|value| value.items.as_slice())
        .or_else(|| {
            preserved_actual
                .as_ref()
                .map(|value| value.items.as_slice())
        })
        .unwrap_or(&[]);
    let effective_model_request_payload = actual_request
        .and_then(|value| value.model_request_payload.as_ref())
        .or_else(|| {
            preserved_actual
                .as_ref()
                .and_then(|value| value.model_request_payload.as_ref())
        });
    let selected_commands = context
        .selected_commands_for_snapshot
        .lock()
        .map(|items| items.clone())
        .unwrap_or_default();
    let task_board_prompt: Option<String> = None;
    let builtin_prompt_debug = inspect_builtin_mcp_prompt_for_runtime(
        context.mcp_server_bundle.2.as_slice(),
        tool_metadata,
        unavailable_builtin_tools,
        context.internal_context_locale,
    );
    let payload = build_turn_runtime_snapshot_payload(BuildTurnRuntimeSnapshotInput {
        user_message_id,
        status,
        base_system_prompt: context.base_system_prompt.as_deref(),
        contact_system_prompt: context.contact_system_prompt.as_deref(),
        task_board_prompt: task_board_prompt.as_deref(),
        builtin_mcp_system_prompt: context.builtin_mcp_system_prompt.as_deref(),
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
        unavailable_builtin_tools,
        builtin_mcp_prompt_debug: Some(&builtin_prompt_debug),
        actual_context_mode: effective_actual_context_mode,
        actual_context_items: effective_actual_items,
        last_model_request_payload: effective_model_request_payload,
    });
    chatos_sessions::sync_turn_runtime_snapshot(session_id, turn_id, &payload)
        .await
        .map(|_| ())
}

pub async fn sync_live_request_snapshot(
    context: &LiveRequestSnapshotContext,
    actual_request: &ActualTurnRequestContext,
) -> Result<(), String> {
    sync_chat_turn_snapshot(
        context.session_id.as_str(),
        context.turn_id.as_str(),
        "running",
        Some(context.user_message_id.clone()),
        context.model.as_str(),
        context.provider.as_str(),
        &context.tool_metadata,
        context.unavailable_builtin_tools.as_slice(),
        &context.runtime_context,
        Some(actual_request),
    )
    .await
}

pub fn inspect_builtin_mcp_prompt_for_runtime(
    builtin_servers: &[crate::services::mcp_loader::McpBuiltinServer],
    tool_metadata: &ToolMetadataMap,
    unavailable_builtin_tools: &[Value],
    locale: crate::core::internal_context_locale::InternalContextLocale,
) -> BuiltinMcpPromptBuildResult {
    if tool_metadata.is_empty() && unavailable_builtin_tools.is_empty() {
        inspect_builtin_mcp_system_prompt(builtin_servers, locale)
    } else {
        inspect_effective_builtin_mcp_system_prompt(
            builtin_servers,
            tool_metadata,
            unavailable_builtin_tools,
            locale,
        )
    }
}

pub fn build_builtin_mcp_debug_payload(
    builtin_servers: &[crate::services::mcp_loader::McpBuiltinServer],
    tool_metadata: &ToolMetadataMap,
    unavailable_builtin_tools: &[Value],
    builtin_mcp_system_prompt: Option<&str>,
    locale: crate::core::internal_context_locale::InternalContextLocale,
) -> Value {
    let inspected = inspect_builtin_mcp_prompt_for_runtime(
        builtin_servers,
        tool_metadata,
        unavailable_builtin_tools,
        locale,
    );

    json!({
        "prompt_source_path": builtin_mcp_prompt_source_path(locale),
        "all_section_ids": builtin_mcp_prompt_section_ids(locale),
        "selected_section_ids": inspected.selected_section_ids,
        "omitted_section_ids": inspected.omitted_section_ids,
        "requested_builtin_server_names": inspected.requested_builtin_server_names,
        "active_builtin_server_names": inspected.active_builtin_server_names,
        "omitted_builtin_server_names": inspected.omitted_builtin_server_names,
        "runtime_limitations": inspected.runtime_limitations,
        "composed_prompt": builtin_mcp_system_prompt
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .or(inspected.prompt),
    })
}

async fn load_existing_actual_request_context(
    session_id: &str,
    turn_id: &str,
) -> Option<ActualTurnRequestContext> {
    let lookup = chatos_sessions::get_turn_runtime_snapshot_by_turn(session_id, turn_id)
        .await
        .ok()?;
    extract_actual_request_context(lookup)
}

fn extract_actual_request_context(
    lookup: TurnRuntimeSnapshotLookupResponseDto,
) -> Option<ActualTurnRequestContext> {
    let runtime = lookup.snapshot?.runtime?;
    if runtime.actual_context_items.is_empty() && runtime.last_model_request_payload.is_none() {
        return None;
    }

    Some(ActualTurnRequestContext {
        context_mode: runtime.actual_context_mode,
        items: runtime.actual_context_items,
        model_request_payload: runtime.last_model_request_payload,
    })
}

fn actual_context_item_from_v3_input_item(
    item: &Value,
) -> Option<TurnRuntimeSnapshotContextItemDto> {
    let item_type = item
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if item_type.is_empty() {
        return None;
    }

    if item_type == "message" {
        let role = item
            .get("role")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("message");
        let content = item
            .get("content")
            .map(|value| {
                join_text_lines_or_json(value, &["text", "value", "content", "delta", "output"])
            })
            .unwrap_or_default();
        let trimmed = content.trim();
        if trimmed.is_empty() {
            return None;
        }
        return Some(TurnRuntimeSnapshotContextItemDto {
            role: Some(role.to_string()),
            item_type: Some("message".to_string()),
            source: Some("request".to_string()),
            content: trimmed.to_string(),
        });
    }

    let content = match item_type {
        "function_call" => {
            let name = item
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            let arguments = item
                .get("arguments")
                .map(|value| join_text_lines_or_json(value, &["text", "value", "content", "delta"]))
                .unwrap_or_default();
            format!("name={name}\narguments={arguments}")
        }
        "function_call_output" => item
            .get("output")
            .map(|value| {
                join_text_lines_or_json(value, &["text", "value", "content", "delta", "output"])
            })
            .unwrap_or_default(),
        _ => serde_json::to_string_pretty(item).ok()?,
    };
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(TurnRuntimeSnapshotContextItemDto {
        role: None,
        item_type: Some(if item_type.starts_with("function_call") {
            "tool".to_string()
        } else {
            item_type.to_string()
        }),
        source: Some("request".to_string()),
        content: trimmed.to_string(),
    })
}
