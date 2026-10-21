// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;
use std::collections::HashSet;

pub(super) fn task_scoped_tools(tools: Vec<Value>, input: &Value) -> Result<Vec<Value>, String> {
    let Some(options) = input.get("tool_options").and_then(Value::as_object) else {
        return Err("Task tool_options must be an object".to_string());
    };
    let requires_execution = options
        .get("requires_execution")
        .and_then(Value::as_bool)
        .ok_or_else(|| "Task tool_options.requires_execution must be a boolean".to_string())?;
    let enabled = options
        .get("enabled_builtin_kinds")
        .and_then(Value::as_array)
        .ok_or_else(|| "Task tool_options.enabled_builtin_kinds must be an array".to_string())?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| "Task builtin capability must be a string".to_string())
        })
        .collect::<Result<HashSet<_>, _>>()?;
    let plugin_hints = options
        .get("plugin_hints")
        .and_then(Value::as_array)
        .ok_or_else(|| "Task tool_options.plugin_hints must be an array".to_string())?;
    let mut plugin_components = HashSet::new();
    for hint in plugin_hints {
        let plugin_key = hint
            .get("plugin_key")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "Task Plugin hint must contain plugin_key".to_string())?;
        let component_key = hint
            .get("component_key")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "Task Plugin hint must contain component_key".to_string())?;
        if !plugin_components.insert((plugin_key.to_string(), component_key.to_string())) {
            return Err("Task Plugin hints must contain unique plugin components".to_string());
        }
    }
    let plugins_enabled = !plugin_components.is_empty();
    let external_mcps = match options.get("external_mcp_config_ids") {
        None => HashSet::new(),
        Some(value) => value
            .as_array()
            .ok_or_else(|| {
                "Task tool_options.external_mcp_config_ids must be an array".to_string()
            })?
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_string)
                    .ok_or_else(|| "Task external MCP id must be a string".to_string())
            })
            .collect::<Result<HashSet<_>, _>>()?,
    };
    let skill_controls_enabled = plugins_enabled
        || !external_mcps.is_empty()
        || enabled.contains("CodeMaintainerRead")
        || enabled.contains("RequirementSurveyRead")
        || (requires_execution && enabled.contains("RequirementSurveyWrite"))
        || (requires_execution
            && (enabled.contains("CodeMaintainerWrite") || enabled.contains("TerminalController")));
    let has_attachments = input
        .get("attachments")
        .and_then(Value::as_array)
        .is_some_and(|attachments| !attachments.is_empty());
    let has_bound_remote_connection = input
        .get("remote_connection_id")
        .and_then(Value::as_str)
        .is_some_and(|value| !value.trim().is_empty());
    Ok(tools
        .into_iter()
        .filter_map(|mut tool| {
            let name = tool_name(&tool).map(str::to_string)?;
            let external_mcp_id = tool
                .get("x-chatos-external-mcp-id")
                .and_then(Value::as_str)
                .map(str::to_string);
            if let Some(resource_id) = external_mcp_id {
                if !external_mcps.contains(resource_id.as_str()) {
                    return None;
                }
                if let Some(object) = tool.as_object_mut() {
                    object.remove("x-chatos-external-mcp-id");
                }
                return Some(tool);
            }
            if name.starts_with("task_run_process_") {
                return Some(tool);
            }
            if matches!(name.as_str(), "tool_result_read" | "tool_result_query") {
                return Some(tool);
            }
            if name.starts_with("ask_user_") {
                return enabled.contains("AskUser").then_some(tool);
            }
            if name == "local_attachment_read" {
                return has_attachments.then_some(tool);
            }
            if matches!(name.as_str(), "team_asset_list" | "team_asset_get") {
                let is_team_executor = input
                    .get("agent_execution_context")
                    .and_then(|context| context.get("schema_version"))
                    .and_then(Value::as_u64)
                    == Some(2)
                    && input.get("team_executor_context").is_some();
                return is_team_executor.then_some(tool);
            }
            if matches!(
                name.as_str(),
                "team_asset_create" | "team_asset_update" | "team_asset_archive"
            ) {
                return None;
            }
            if name.starts_with("notepad_") {
                let notepad_enabled = enabled.contains("Notepad");
                let is_write =
                    matches!(name.as_str(), "notepad_create_note" | "notepad_update_note");
                // A Plugin Task must deliver through its selected Plugin. If a note is
                // also an explicit deliverable, Main Chat creates a separate downstream
                // Notepad Task instead of giving the Plugin worker a writable fallback.
                return (notepad_enabled && !(plugins_enabled && is_write)).then_some(tool);
            }
            if name.starts_with("requirement_survey_") || name == "requirement_plan_save" {
                return (enabled.contains("RequirementSurveyRead")
                    || (requires_execution && enabled.contains("RequirementSurveyWrite")))
                .then_some(tool);
            }
            if matches!(
                name.as_str(),
                "read_file_raw"
                    | "read_file_range"
                    | "list_dir"
                    | "search_text"
                    | "read_file"
                    | "search_files"
                    | "project_list"
                    | "project_read"
                    | "project_search"
            ) {
                return enabled.contains("CodeMaintainerRead").then_some(tool);
            }
            if matches!(
                name.as_str(),
                "open_edit_session"
                    | "stage_edit_batch"
                    | "commit_edit_session"
                    | "abort_edit_session"
                    | "project_write"
            ) {
                return (requires_execution && enabled.contains("CodeMaintainerWrite"))
                    .then_some(tool);
            }
            if matches!(
                name.as_str(),
                "execute_command"
                    | "process_poll"
                    | "process_log"
                    | "process_wait"
                    | "process_write"
                    | "process_kill"
                    | "terminal_exec"
            ) {
                return (requires_execution && enabled.contains("TerminalController"))
                    .then_some(tool);
            }
            if matches!(
                name.as_str(),
                "capability_search"
                    | "capability_describe"
                    | "capability_invoke"
                    | "agent_skill_activate"
                    | "agent_skill_read_resource"
            ) {
                return skill_controls_enabled.then_some(tool);
            }
            if name.starts_with("remote_connection_controller_") {
                return (has_bound_remote_connection
                    && enabled.contains("RemoteConnectionController"))
                .then_some(tool);
            }
            None
        })
        .collect())
}

fn tool_name(tool: &Value) -> Option<&str> {
    tool.get("name")
        .and_then(Value::as_str)
        .or_else(|| tool.pointer("/function/name").and_then(Value::as_str))
}
