// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::process::Stdio;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::history::{
    command_history_entry_from_exec_result, normalize_history_source, output_text,
    CommandExecutionContext, CommandHistoryRecorder,
};
use crate::relay::RelayRequest;
use crate::workspace::paths::{
    relative_to_workspace, resolve_request_workspace_dir, workspace_for_request,
};
use crate::{
    local_now_rfc3339, LocalState, DEFAULT_TERMINAL_EXEC_TIMEOUT_MS, MAX_TERMINAL_EXEC_TIMEOUT_MS,
};

#[derive(Debug, Deserialize)]
struct TerminalExecRequest {
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default, alias = "working_dir")]
    cwd: Option<String>,
    timeout_ms: Option<u64>,
    source: Option<String>,
}

pub(super) async fn run_terminal_exec(
    request: &RelayRequest,
    state: &LocalState,
    body: Value,
    mut context: CommandExecutionContext,
    history_recorder: Option<&CommandHistoryRecorder>,
) -> Result<Value> {
    let started_at = local_now_rfc3339();
    let exec = serde_json::from_value::<TerminalExecRequest>(body)
        .context("parse terminal exec request")?;
    let command = exec.command.trim().to_string();
    if command.is_empty() {
        return Err(anyhow!("terminal exec requires command"));
    }
    let args = exec.args;
    let workspace = workspace_for_request(state, request.workspace_id.as_str())?;
    let cwd =
        resolve_request_workspace_dir(workspace, request, exec.cwd.as_deref().unwrap_or("."))?;
    let cwd_label = relative_to_workspace(workspace, cwd.as_path());
    if let Some(source) = exec.source.as_deref().and_then(normalize_history_source) {
        context.source = source;
    }
    let timeout_ms = exec
        .timeout_ms
        .unwrap_or(DEFAULT_TERMINAL_EXEC_TIMEOUT_MS)
        .clamp(1_000, MAX_TERMINAL_EXEC_TIMEOUT_MS);

    let mut child = tokio::process::Command::new(command.as_str());
    child
        .args(&args)
        .current_dir(cwd.as_path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let output = match tokio::time::timeout(Duration::from_millis(timeout_ms), child.output()).await
    {
        Ok(Ok(output)) => output,
        Ok(Err(err)) => {
            let body = json!({
                "command": command,
                "args": args,
                "cwd": cwd_label,
                "workspace_id": request.workspace_id.as_str(),
                "success": false,
                "exit_code": Option::<i32>::None,
                "timed_out": false,
                "timeout_ms": timeout_ms,
                "stdout": "",
                "stderr": "",
                "error": err.to_string(),
            });
            append_terminal_exec_history(
                state,
                request,
                &context,
                command.as_str(),
                &args,
                cwd_label.as_str(),
                started_at,
                &body,
                history_recorder,
            )
            .await;
            return Ok(body);
        }
        Err(_) => {
            let body = json!({
                "command": command,
                "args": args,
                "cwd": cwd_label,
                "workspace_id": request.workspace_id.as_str(),
                "success": false,
                "exit_code": Option::<i32>::None,
                "timed_out": true,
                "timeout_ms": timeout_ms,
                "stdout": "",
                "stderr": format!("command timed out after {timeout_ms} ms"),
            });
            append_terminal_exec_history(
                state,
                request,
                &context,
                command.as_str(),
                &args,
                cwd_label.as_str(),
                started_at,
                &body,
                history_recorder,
            )
            .await;
            return Ok(body);
        }
    };

    let (stdout, stdout_truncated) = output_text(output.stdout.as_slice());
    let (stderr, stderr_truncated) = output_text(output.stderr.as_slice());
    let body = json!({
        "command": command,
        "args": args,
        "cwd": cwd_label,
        "workspace_id": request.workspace_id.as_str(),
        "success": output.status.success(),
        "exit_code": output.status.code(),
        "timed_out": false,
        "timeout_ms": timeout_ms,
        "stdout": stdout,
        "stderr": stderr,
        "stdout_bytes": output.stdout.len(),
        "stderr_bytes": output.stderr.len(),
        "stdout_truncated": stdout_truncated,
        "stderr_truncated": stderr_truncated,
    });
    append_terminal_exec_history_from_body(
        state,
        request,
        &context,
        started_at,
        &body,
        history_recorder,
    )
    .await;
    Ok(body)
}

async fn append_terminal_exec_history_from_body(
    state: &LocalState,
    request: &RelayRequest,
    context: &CommandExecutionContext,
    started_at: String,
    body: &Value,
    history_recorder: Option<&CommandHistoryRecorder>,
) {
    let Some(recorder) = history_recorder else {
        return;
    };
    let command = body
        .get("command")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let args = body
        .get("args")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let cwd_label = body.get("cwd").and_then(Value::as_str).unwrap_or(".");
    append_terminal_exec_history(
        state,
        request,
        context,
        command,
        &args,
        cwd_label,
        started_at,
        body,
        Some(recorder),
    )
    .await;
}

async fn append_terminal_exec_history(
    state: &LocalState,
    request: &RelayRequest,
    context: &CommandExecutionContext,
    command: &str,
    args: &[String],
    cwd_label: &str,
    started_at: String,
    body: &Value,
    history_recorder: Option<&CommandHistoryRecorder>,
) {
    let Some(recorder) = history_recorder else {
        return;
    };
    recorder
        .append(command_history_entry_from_exec_result(
            state, request, context, command, args, cwd_label, started_at, body,
        ))
        .await;
}
