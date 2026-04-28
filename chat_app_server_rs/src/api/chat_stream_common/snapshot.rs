use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use crate::core::builtin_mcp_prompt::{
    builtin_mcp_prompt_section_ids, builtin_mcp_prompt_source_path,
    inspect_builtin_mcp_system_prompt, inspect_effective_builtin_mcp_system_prompt,
    BuiltinMcpPromptBuildResult,
};
use crate::core::chat_runtime::parse_implicit_command_selections_from_tools_end;
use crate::core::turn_runtime_snapshot::{
    build_turn_runtime_snapshot_payload, BuildTurnRuntimeSnapshotInput,
};
use crate::services::ai_client_common::AiClientCallbacks;
use crate::services::memory_server_client::{self, TurnRuntimeSnapshotSelectedCommandDto};
use crate::services::task_board_prompt::build_task_board_prompt;

use super::types::{ResolvedChatStreamContext, ToolMetadataMap};

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
    tool_metadata: &ToolMetadataMap,
    unavailable_builtin_tools: &[Value],
    context: &ResolvedChatStreamContext,
) -> Result<(), String> {
    let selected_commands = context
        .selected_commands_for_snapshot
        .lock()
        .map(|items| items.clone())
        .unwrap_or_default();
    let task_board_prompt = build_task_board_prompt(session_id, Some(turn_id)).await;
    let builtin_prompt_debug = inspect_builtin_mcp_prompt_for_runtime(
        context.mcp_server_bundle.2.as_slice(),
        tool_metadata,
        unavailable_builtin_tools,
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
    });
    memory_server_client::sync_turn_runtime_snapshot(session_id, turn_id, &payload)
        .await
        .map(|_| ())
}

pub(crate) fn inspect_builtin_mcp_prompt_for_runtime(
    builtin_servers: &[crate::services::mcp_loader::McpBuiltinServer],
    tool_metadata: &ToolMetadataMap,
    unavailable_builtin_tools: &[Value],
) -> BuiltinMcpPromptBuildResult {
    if tool_metadata.is_empty() && unavailable_builtin_tools.is_empty() {
        inspect_builtin_mcp_system_prompt(builtin_servers)
    } else {
        inspect_effective_builtin_mcp_system_prompt(
            builtin_servers,
            tool_metadata,
            unavailable_builtin_tools,
        )
    }
}

pub(crate) fn build_builtin_mcp_debug_payload(
    builtin_servers: &[crate::services::mcp_loader::McpBuiltinServer],
    tool_metadata: &ToolMetadataMap,
    unavailable_builtin_tools: &[Value],
    builtin_mcp_system_prompt: Option<&str>,
) -> Value {
    let inspected = inspect_builtin_mcp_prompt_for_runtime(
        builtin_servers,
        tool_metadata,
        unavailable_builtin_tools,
    );

    json!({
        "prompt_source_path": builtin_mcp_prompt_source_path(),
        "all_section_ids": builtin_mcp_prompt_section_ids(),
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
