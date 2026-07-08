// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;
use uuid::Uuid;

use super::format::{format_command_display, history_output_preview, truncate_text};
use super::sandbox::{extract_sandbox_tool_result, SandboxToolCallDetails};
use super::types::{CommandExecutionContext, CommandHistoryEntry};
use crate::relay::RelayRequest;
use crate::terminal::session::InteractiveCommandSubmission;
use crate::workspace::paths::relative_to_workspace;
use crate::{local_now_rfc3339, LocalState, MAX_TERMINAL_OUTPUT_BYTES};

pub(crate) fn output_text(bytes: &[u8]) -> (String, bool) {
    truncate_text(
        String::from_utf8_lossy(bytes).into_owned(),
        MAX_TERMINAL_OUTPUT_BYTES,
    )
}

pub(crate) fn command_history_entry_from_exec_result(
    state: &LocalState,
    request: &RelayRequest,
    context: &CommandExecutionContext,
    command: &str,
    args: &[String],
    cwd: &str,
    started_at: String,
    body: &Value,
) -> CommandHistoryEntry {
    let workspace = state.workspace_by_id(request.workspace_id.as_str());
    let timed_out = body
        .get("timed_out")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let success = body
        .get("success")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let error = body
        .get("error")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let exit_code = body
        .get("exit_code")
        .and_then(Value::as_i64)
        .map(|value| value as i32);
    let status = if timed_out {
        "timed_out"
    } else if success {
        "succeeded"
    } else {
        "failed"
    };
    CommandHistoryEntry {
        id: format!("cmd-{}", Uuid::new_v4()),
        source: context.source.clone(),
        workspace_id: Some(request.workspace_id.clone()),
        workspace_alias: workspace.map(|workspace| workspace.alias.clone()),
        cwd: Some(cwd.to_string()),
        command: command.to_string(),
        args: args.to_vec(),
        display: format_command_display(command, args),
        status: status.to_string(),
        exit_code,
        stdout_preview: body
            .get("stdout")
            .and_then(Value::as_str)
            .map(history_output_preview),
        stderr_preview: body
            .get("stderr")
            .and_then(Value::as_str)
            .map(history_output_preview),
        error,
        started_at,
        finished_at: Some(local_now_rfc3339()),
        request_id: context.request_id.clone(),
        terminal_session_id: context.terminal_session_id.clone(),
        sandbox_id: context.sandbox_id.clone(),
        tool_name: context.tool_name.clone(),
    }
}

pub(crate) fn command_history_entry_for_interactive_submission(
    state: &LocalState,
    request: &RelayRequest,
    terminal_session_id: &str,
    submission: InteractiveCommandSubmission,
) -> CommandHistoryEntry {
    let workspace = state.workspace_by_id(request.workspace_id.as_str());
    let cwd = workspace
        .map(|workspace| relative_to_workspace(workspace, submission.cwd.as_path()))
        .unwrap_or_else(|| submission.cwd.display().to_string());
    let status = if submission.blocked_reason.is_some() {
        "blocked"
    } else {
        "submitted"
    };
    CommandHistoryEntry {
        id: format!("cmd-{}", Uuid::new_v4()),
        source: "chatos_terminal_session".to_string(),
        workspace_id: Some(request.workspace_id.clone()),
        workspace_alias: workspace.map(|workspace| workspace.alias.clone()),
        cwd: Some(cwd),
        command: submission.command.clone(),
        args: Vec::new(),
        display: submission.command,
        status: status.to_string(),
        exit_code: None,
        stdout_preview: None,
        stderr_preview: None,
        error: submission.blocked_reason,
        started_at: local_now_rfc3339(),
        finished_at: None,
        request_id: Some(request.request_id.clone()),
        terminal_session_id: Some(terminal_session_id.to_string()),
        sandbox_id: None,
        tool_name: None,
    }
}

pub(crate) fn command_history_entry_for_sandbox_tool_call(
    state: &LocalState,
    request: &RelayRequest,
    context: &CommandExecutionContext,
    details: SandboxToolCallDetails,
    http_status: u16,
    body: &Value,
    started_at: String,
) -> CommandHistoryEntry {
    let workspace = state.workspace_by_id(request.workspace_id.as_str());
    let extracted = extract_sandbox_tool_result(body);
    let failed_http = !(200..300).contains(&http_status);
    let has_error = extracted.error.is_some();
    let timed_out = extracted.timed_out.unwrap_or(false);
    let exit_failed = extracted.exit_code.map(|code| code != 0).unwrap_or(false);
    let status = if timed_out {
        "timed_out"
    } else if failed_http || has_error || exit_failed {
        "failed"
    } else {
        "succeeded"
    };
    CommandHistoryEntry {
        id: format!("cmd-{}", Uuid::new_v4()),
        source: context.source.clone(),
        workspace_id: Some(request.workspace_id.clone()),
        workspace_alias: workspace.map(|workspace| workspace.alias.clone()),
        cwd: details.cwd,
        command: details.command,
        args: details.args,
        display: details.display,
        status: status.to_string(),
        exit_code: extracted.exit_code,
        stdout_preview: extracted
            .stdout
            .map(|value| history_output_preview(value.as_str())),
        stderr_preview: extracted
            .stderr
            .map(|value| history_output_preview(value.as_str())),
        error: extracted.error,
        started_at,
        finished_at: Some(local_now_rfc3339()),
        request_id: context.request_id.clone(),
        terminal_session_id: None,
        sandbox_id: context.sandbox_id.clone(),
        tool_name: context.tool_name.clone(),
    }
}

pub(crate) fn normalize_history_source(source: &str) -> Option<String> {
    let normalized = source.trim().to_ascii_lowercase().replace('-', "_");
    match normalized.as_str() {
        "chatos_terminal_exec"
        | "chatos_terminal_session"
        | "local_mcp"
        | "task_runner_sandbox"
        | "local_connector_ui" => Some(normalized),
        _ => None,
    }
}
