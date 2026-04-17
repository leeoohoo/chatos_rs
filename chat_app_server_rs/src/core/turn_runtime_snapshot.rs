use std::collections::HashMap;

use serde_json::Value;

use crate::core::mcp_tools::ToolInfo;
use crate::services::memory_server_client::{
    SyncTurnRuntimeSnapshotRequestDto, TurnRuntimeSnapshotRuntimeDto,
    TurnRuntimeSnapshotSelectedCommandDto, TurnRuntimeSnapshotSystemMessageDto,
    TurnRuntimeSnapshotToolDto,
};

pub struct BuildTurnRuntimeSnapshotInput<'a> {
    pub user_message_id: Option<String>,
    pub status: &'a str,
    pub base_system_prompt: Option<&'a str>,
    pub contact_system_prompt: Option<&'a str>,
    pub tool_routing_system_prompt: Option<&'a str>,
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
    if let Some(content) = normalize_optional_text(input.tool_routing_system_prompt) {
        system_messages.push(TurnRuntimeSnapshotSystemMessageDto {
            id: "tool_routing".to_string(),
            source: "tool_routing_policy".to_string(),
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{build_turn_runtime_snapshot_payload, BuildTurnRuntimeSnapshotInput};

    #[test]
    fn snapshot_payload_includes_tool_routing_system_prompt() {
        let payload = build_turn_runtime_snapshot_payload(BuildTurnRuntimeSnapshotInput {
            user_message_id: None,
            status: "running",
            base_system_prompt: Some("base prompt"),
            contact_system_prompt: Some("contact prompt"),
            tool_routing_system_prompt: Some("routing prompt"),
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
        });

        let system_messages = payload.system_messages.expect("system messages");
        assert_eq!(system_messages.len(), 4);
        assert_eq!(system_messages[2].id, "tool_routing");
        assert_eq!(system_messages[2].source, "tool_routing_policy");
        assert_eq!(system_messages[2].content, "routing prompt");
    }
}
