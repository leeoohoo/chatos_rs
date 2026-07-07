// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::Path;

use anyhow::{anyhow, Result};
use serde_json::{json, Value};

use crate::history::{
    command_history_entry_from_exec_result, CommandExecutionContext, CommandHistoryRecorder,
};
use crate::relay::RelayRequest;
use crate::terminal::controller::local_terminal_controller_service_for_root;
use crate::{
    local_now_rfc3339, LocalState, WorkspaceState, DEFAULT_TERMINAL_EXEC_TIMEOUT_MS,
    MAX_TERMINAL_EXEC_TIMEOUT_MS,
};

use super::code::code_maintainer_structured_result;
use super::project::{normalize_request_project_relative_path, request_project_root};

pub(crate) async fn call_local_terminal_controller_tool(
    request: &RelayRequest,
    state: &LocalState,
    workspace: &WorkspaceState,
    tool_name: &str,
    mut arguments: Value,
    history_recorder: &CommandHistoryRecorder,
) -> Result<Value> {
    let timeout_ms = arguments
        .get("timeout_ms")
        .or_else(|| arguments.get("max_wait_ms"))
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_TERMINAL_EXEC_TIMEOUT_MS)
        .clamp(1_000, MAX_TERMINAL_EXEC_TIMEOUT_MS);
    let project_root = request_project_root(workspace, request)?;
    let normalized_path = if tool_name == "execute_command" {
        let path = arguments.get("path").and_then(Value::as_str).unwrap_or(".");
        let normalized_path = normalize_request_project_relative_path(workspace, request, path)?;
        if let Some(map) = arguments.as_object_mut() {
            map.insert("path".to_string(), Value::String(normalized_path.clone()));
        }
        Some(normalized_path)
    } else {
        None
    };
    let service =
        local_terminal_controller_service_for_root(project_root.as_path(), request, timeout_ms)?;
    let result = service
        .call_tool(tool_name, arguments, None)
        .map_err(|err| anyhow!(err))?;
    if tool_name != "execute_command" {
        return Ok(result);
    }

    let structured = code_maintainer_structured_result(result.clone());
    let command = structured
        .get("common")
        .or_else(|| structured.get("command"))
        .and_then(Value::as_str)
        .unwrap_or("execute_command");
    let cwd_label = structured
        .get("path")
        .and_then(Value::as_str)
        .and_then(|path| {
            Path::new(path)
                .strip_prefix(project_root.as_path())
                .ok()
                .map(|path| path.to_string_lossy().replace('\\', "/"))
        })
        .filter(|value| !value.is_empty())
        .or(normalized_path)
        .unwrap_or_else(|| ".".to_string());
    let history_body = json!({
        "command": command,
        "args": [],
        "cwd": cwd_label,
        "success": structured.get("success").and_then(Value::as_bool).unwrap_or(false),
        "exit_code": structured.get("exit_code").and_then(Value::as_i64),
        "timed_out": structured.get("timed_out").and_then(Value::as_bool).unwrap_or(false),
        "stdout": structured
            .get("stdout")
            .or_else(|| structured.get("output"))
            .and_then(Value::as_str)
            .unwrap_or_default(),
        "stderr": structured.get("stderr").and_then(Value::as_str).unwrap_or_default(),
    });
    history_recorder
        .append(command_history_entry_from_exec_result(
            state,
            request,
            &CommandExecutionContext::local_mcp(request, "execute_command"),
            command,
            &[],
            history_body
                .get("cwd")
                .and_then(Value::as_str)
                .unwrap_or("."),
            local_now_rfc3339(),
            &history_body,
        ))
        .await;
    Ok(result)
}
