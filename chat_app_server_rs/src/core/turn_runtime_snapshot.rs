use std::collections::HashMap;

use serde_json::Value;

use crate::core::builtin_mcp_prompt::BuiltinMcpPromptBuildResult;
use crate::core::mcp_tools::ToolInfo;
use crate::services::memory_server_client::{
    SyncTurnRuntimeSnapshotRequestDto, TurnRuntimeSnapshotBuiltinMcpPromptDto,
    TurnRuntimeSnapshotRuntimeDto, TurnRuntimeSnapshotSelectedCommandDto,
    TurnRuntimeSnapshotSystemMessageDto, TurnRuntimeSnapshotToolDto,
    TurnRuntimeSnapshotUnavailableToolDto,
};

pub struct BuildTurnRuntimeSnapshotInput<'a> {
    pub user_message_id: Option<String>,
    pub status: &'a str,
    pub base_system_prompt: Option<&'a str>,
    pub contact_system_prompt: Option<&'a str>,
    pub task_board_prompt: Option<&'a str>,
    pub builtin_mcp_system_prompt: Option<&'a str>,
    pub memory_summary_prompt: Option<&'a str>,
    pub tools: &'a HashMap<String, ToolInfo>,
    pub model: Option<&'a str>,
    pub provider: Option<&'a str>,
    pub contact_agent_id: Option<&'a str>,
    pub remote_connection_id: Option<&'a str>,
    pub project_id: Option<&'a str>,
    pub project_root: Option<&'a str>,
    pub workspace_root: Option<&'a str>,
    pub mcp_enabled: bool,
    pub enabled_mcp_ids: &'a [String],
    pub selected_commands: &'a [TurnRuntimeSnapshotSelectedCommandDto],
    pub unavailable_builtin_tools: &'a [Value],
    pub builtin_mcp_prompt_debug: Option<&'a BuiltinMcpPromptBuildResult>,
}

pub fn build_turn_runtime_snapshot_payload(
    input: BuildTurnRuntimeSnapshotInput<'_>,
) -> SyncTurnRuntimeSnapshotRequestDto {
    let mut system_messages = Vec::new();
    if let Some(content) = normalize_optional_text(input.base_system_prompt) {
        system_messages.push(TurnRuntimeSnapshotSystemMessageDto {
            id: "base_system".to_string(),
            source: "active_system_context".to_string(),
            content,
        });
    }
    if let Some(content) = normalize_optional_text(input.contact_system_prompt) {
        system_messages.push(TurnRuntimeSnapshotSystemMessageDto {
            id: "contact_system".to_string(),
            source: "contact_runtime_context".to_string(),
            content,
        });
    }
    if let Some(content) = normalize_optional_text(input.task_board_prompt) {
        system_messages.push(TurnRuntimeSnapshotSystemMessageDto {
            id: "task_board".to_string(),
            source: "task_runtime_board".to_string(),
            content,
        });
    }
    if let Some(content) = normalize_optional_text(input.builtin_mcp_system_prompt) {
        system_messages.push(TurnRuntimeSnapshotSystemMessageDto {
            id: "builtin_mcp".to_string(),
            source: "builtin_mcp_policy".to_string(),
            content,
        });
    }
    if let Some(content) = normalize_optional_text(input.memory_summary_prompt) {
        system_messages.push(TurnRuntimeSnapshotSystemMessageDto {
            id: "memory_summary".to_string(),
            source: "memory_context_summary".to_string(),
            content,
        });
    }

    let mut tool_entries: Vec<(&String, &ToolInfo)> = input.tools.iter().collect();
    tool_entries.sort_by(|(left, _), (right, _)| left.cmp(right));
    let tools = tool_entries
        .into_iter()
        .map(|(name, info)| TurnRuntimeSnapshotToolDto {
            name: name.to_string(),
            server_name: info.server_name.clone(),
            server_type: info.server_type.clone(),
            description: extract_tool_description(&info.tool_info),
        })
        .collect::<Vec<_>>();

    SyncTurnRuntimeSnapshotRequestDto {
        user_message_id: input.user_message_id,
        status: Some(normalize_status(input.status)),
        snapshot_source: Some("captured".to_string()),
        snapshot_version: Some(1),
        captured_at: None,
        system_messages: Some(system_messages),
        tools: Some(tools),
        runtime: Some(TurnRuntimeSnapshotRuntimeDto {
            model: normalize_optional_text(input.model),
            provider: normalize_optional_text(input.provider),
            contact_agent_id: normalize_optional_text(input.contact_agent_id),
            remote_connection_id: normalize_optional_text(input.remote_connection_id),
            project_id: normalize_optional_text(input.project_id),
            project_root: normalize_optional_text(input.project_root),
            workspace_root: normalize_optional_text(input.workspace_root),
            mcp_enabled: Some(input.mcp_enabled),
            enabled_mcp_ids: normalize_string_list(input.enabled_mcp_ids),
            selected_commands: normalize_selected_commands(input.selected_commands),
            unavailable_builtin_tools: normalize_unavailable_builtin_tools(
                input.unavailable_builtin_tools,
            ),
            builtin_mcp_prompt: normalize_builtin_mcp_prompt(input.builtin_mcp_prompt_debug),
        }),
    }
}

fn extract_tool_description(tool_info: &Value) -> Option<String> {
    let direct = tool_info
        .get("description")
        .and_then(Value::as_str)
        .and_then(|value| normalize_optional_text(Some(value)));
    if direct.is_some() {
        return direct;
    }

    tool_info
        .get("function")
        .and_then(Value::as_object)
        .and_then(|function| function.get("description"))
        .and_then(Value::as_str)
        .and_then(|value| normalize_optional_text(Some(value)))
}

fn normalize_status(status: &str) -> String {
    match status.trim().to_ascii_lowercase().as_str() {
        "running" => "running".to_string(),
        "completed" => "completed".to_string(),
        "failed" => "failed".to_string(),
        _ => "unknown".to_string(),
    }
}

fn normalize_string_list(values: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for value in values {
        let normalized = value.trim();
        if normalized.is_empty() {
            continue;
        }
        if out.iter().any(|item| item == normalized) {
            continue;
        }
        out.push(normalized.to_string());
    }
    out
}

fn normalize_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|raw| !raw.is_empty())
        .map(ToOwned::to_owned)
}

fn normalize_selected_commands(
    selected_commands: &[TurnRuntimeSnapshotSelectedCommandDto],
) -> Vec<TurnRuntimeSnapshotSelectedCommandDto> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for item in selected_commands {
        let plugin_source = item.plugin_source.trim();
        let source_path = item.source_path.trim();
        if plugin_source.is_empty() || source_path.is_empty() {
            continue;
        }
        let dedup_key = format!(
            "{}::{}::{}",
            item.command_ref
                .as_deref()
                .map(str::trim)
                .unwrap_or_default(),
            plugin_source,
            source_path
        );
        if !seen.insert(dedup_key) {
            continue;
        }
        out.push(TurnRuntimeSnapshotSelectedCommandDto {
            command_ref: normalize_optional_text(item.command_ref.as_deref()),
            name: normalize_optional_text(item.name.as_deref()),
            plugin_source: plugin_source.to_string(),
            source_path: source_path.to_string(),
            trigger: normalize_optional_text(item.trigger.as_deref()),
            arguments: normalize_optional_text(item.arguments.as_deref()),
        });
    }
    out
}

fn normalize_unavailable_builtin_tools(
    unavailable_tools: &[Value],
) -> Vec<TurnRuntimeSnapshotUnavailableToolDto> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for item in unavailable_tools {
        let Some(server_name) = item
            .get("server_name")
            .and_then(Value::as_str)
            .and_then(|value| normalize_optional_text(Some(value)))
        else {
            continue;
        };
        let Some(tool_name) = item
            .get("tool_name")
            .and_then(Value::as_str)
            .and_then(|value| normalize_optional_text(Some(value)))
        else {
            continue;
        };
        let reason = item
            .get("reason")
            .and_then(Value::as_str)
            .and_then(|value| normalize_optional_text(Some(value)));
        let dedup_key = format!(
            "{}::{}::{}",
            server_name,
            tool_name,
            reason.as_deref().unwrap_or_default()
        );
        if !seen.insert(dedup_key) {
            continue;
        }
        out.push(TurnRuntimeSnapshotUnavailableToolDto {
            server_name,
            tool_name,
            reason,
        });
    }

    out
}

fn normalize_builtin_mcp_prompt(
    prompt: Option<&BuiltinMcpPromptBuildResult>,
) -> Option<TurnRuntimeSnapshotBuiltinMcpPromptDto> {
    let Some(prompt) = prompt else {
        return None;
    };

    Some(TurnRuntimeSnapshotBuiltinMcpPromptDto {
        prompt_source_path: normalize_optional_text(Some(
            crate::core::builtin_mcp_prompt::builtin_mcp_prompt_source_path(),
        )),
        all_section_ids: normalize_string_list(
            crate::core::builtin_mcp_prompt::builtin_mcp_prompt_section_ids().as_slice(),
        ),
        selected_section_ids: normalize_string_list(prompt.selected_section_ids.as_slice()),
        omitted_section_ids: normalize_string_list(prompt.omitted_section_ids.as_slice()),
        requested_builtin_server_names: normalize_string_list(
            prompt.requested_builtin_server_names.as_slice(),
        ),
        active_builtin_server_names: normalize_string_list(
            prompt.active_builtin_server_names.as_slice(),
        ),
        omitted_builtin_server_names: normalize_string_list(
            prompt.omitted_builtin_server_names.as_slice(),
        ),
        runtime_limitations: normalize_optional_text(prompt.runtime_limitations.as_deref()),
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use serde_json::json;

    use super::{build_turn_runtime_snapshot_payload, BuildTurnRuntimeSnapshotInput};
    use crate::core::builtin_mcp_prompt::BuiltinMcpPromptBuildResult;

    #[test]
    fn snapshot_payload_includes_builtin_mcp_system_prompt() {
        let payload = build_turn_runtime_snapshot_payload(BuildTurnRuntimeSnapshotInput {
            user_message_id: None,
            status: "running",
            base_system_prompt: Some("base prompt"),
            contact_system_prompt: Some("contact prompt"),
            task_board_prompt: Some("task board prompt"),
            builtin_mcp_system_prompt: Some("builtin mcp prompt"),
            memory_summary_prompt: Some("memory prompt"),
            tools: &HashMap::new(),
            model: Some("gpt-5"),
            provider: Some("openai"),
            contact_agent_id: None,
            remote_connection_id: None,
            project_id: None,
            project_root: None,
            workspace_root: None,
            mcp_enabled: true,
            enabled_mcp_ids: &[],
            selected_commands: &[],
            unavailable_builtin_tools: &[],
            builtin_mcp_prompt_debug: None,
        });

        let system_messages = payload.system_messages.expect("system messages");
        assert_eq!(system_messages.len(), 5);
        assert_eq!(system_messages[2].id, "task_board");
        assert_eq!(system_messages[2].source, "task_runtime_board");
        assert_eq!(system_messages[2].content, "task board prompt");
        assert_eq!(system_messages[3].id, "builtin_mcp");
        assert_eq!(system_messages[3].source, "builtin_mcp_policy");
        assert_eq!(system_messages[3].content, "builtin mcp prompt");
    }

    #[test]
    fn snapshot_payload_includes_unavailable_builtin_tools() {
        let payload = build_turn_runtime_snapshot_payload(BuildTurnRuntimeSnapshotInput {
            user_message_id: None,
            status: "running",
            base_system_prompt: None,
            contact_system_prompt: None,
            task_board_prompt: None,
            builtin_mcp_system_prompt: None,
            memory_summary_prompt: None,
            tools: &HashMap::new(),
            model: None,
            provider: None,
            contact_agent_id: None,
            remote_connection_id: None,
            project_id: None,
            project_root: None,
            workspace_root: None,
            mcp_enabled: true,
            enabled_mcp_ids: &[],
            selected_commands: &[],
            unavailable_builtin_tools: &[json!({
                "server_name": "browser_tools",
                "tool_name": "browser_inspect",
                "reason": "agent-browser unavailable"
            })],
            builtin_mcp_prompt_debug: None,
        });

        let runtime = payload.runtime.expect("runtime");
        assert_eq!(runtime.unavailable_builtin_tools.len(), 1);
        assert_eq!(runtime.unavailable_builtin_tools[0].server_name, "browser_tools");
        assert_eq!(runtime.unavailable_builtin_tools[0].tool_name, "browser_inspect");
        assert_eq!(
            runtime.unavailable_builtin_tools[0].reason.as_deref(),
            Some("agent-browser unavailable")
        );
    }

    #[test]
    fn snapshot_payload_includes_builtin_mcp_prompt_debug() {
        let payload = build_turn_runtime_snapshot_payload(BuildTurnRuntimeSnapshotInput {
            user_message_id: None,
            status: "running",
            base_system_prompt: None,
            contact_system_prompt: None,
            task_board_prompt: None,
            builtin_mcp_system_prompt: Some("builtin mcp prompt"),
            memory_summary_prompt: None,
            tools: &HashMap::new(),
            model: None,
            provider: None,
            contact_agent_id: None,
            remote_connection_id: None,
            project_id: None,
            project_root: None,
            workspace_root: None,
            mcp_enabled: true,
            enabled_mcp_ids: &[],
            selected_commands: &[],
            unavailable_builtin_tools: &[],
            builtin_mcp_prompt_debug: Some(&BuiltinMcpPromptBuildResult {
                prompt: Some("builtin mcp prompt".to_string()),
                selected_section_ids: vec!["global".to_string(), "builtin_task_manager".to_string()],
                omitted_section_ids: vec!["builtin_browser_tools".to_string()],
                requested_builtin_server_names: vec!["task_manager".to_string(), "browser_tools".to_string()],
                active_builtin_server_names: vec!["task_manager".to_string()],
                omitted_builtin_server_names: vec!["browser_tools".to_string()],
                runtime_limitations: Some("当前运行时限制：\n- 当前不要依赖以下内置 MCP 工具：`browser_tools_browser_inspect`。".to_string()),
            }),
        });

        let runtime = payload.runtime.expect("runtime");
        let builtin = runtime.builtin_mcp_prompt.expect("builtin prompt debug");
        assert_eq!(builtin.prompt_source_path.as_deref(), Some("BUILTIN_MCP_PROMPT.md"));
        assert!(builtin.all_section_ids.iter().any(|item| item == "global"));
        assert_eq!(builtin.selected_section_ids, vec!["global", "builtin_task_manager"]);
        assert_eq!(builtin.omitted_section_ids, vec!["builtin_browser_tools"]);
        assert_eq!(builtin.active_builtin_server_names, vec!["task_manager"]);
        assert_eq!(builtin.omitted_builtin_server_names, vec!["browser_tools"]);
    }
}
