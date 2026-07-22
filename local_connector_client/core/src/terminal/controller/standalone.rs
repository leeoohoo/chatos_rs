// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::PathBuf;
use std::process::Stdio;

use chatos_mcp::TerminalControllerContext;
use serde_json::{json, Value};

use super::registry::{
    append_local_mcp_terminal_log, collect_local_mcp_terminal_output_by_kinds,
    register_local_mcp_terminal_session, wait_for_local_mcp_terminal_session,
};
use super::shell::shell_command_for_terminal_controller;

pub(super) async fn execute_local_mcp_standalone_command(
    context: TerminalControllerContext,
    project_root: PathBuf,
    cwd: PathBuf,
    display_project_root: String,
    display_cwd: String,
    command: String,
    background: bool,
    reuse_skipped_reason: Option<&str>,
) -> std::result::Result<Value, String> {
    let mut child = shell_command_for_terminal_controller(command.as_str());
    child
        .current_dir(cwd.as_path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let child = child.spawn().map_err(|err| err.to_string())?;
    let session = register_local_mcp_terminal_session(
        context.clone(),
        project_root.clone(),
        cwd.clone(),
        command.clone(),
        child,
    )
    .await?;
    append_local_mcp_terminal_log(session.clone(), "command", command.clone()).await;
    let session_id = session.meta.lock().await.id.clone();

    if background {
        let mut response = json!({
            "project_id": context.project_id,
            "project_root": display_project_root,
            "terminal_id": session_id.clone(),
            "process_id": session_id,
            "terminal_reused": false,
            "path": display_cwd,
            "common": command,
            "background": true,
            "busy": true,
            "output": "",
            "output_chars": 0,
            "truncated": false,
            "finished_by": "background",
            "idle_timeout_ms": context.idle_timeout_ms,
            "max_wait_ms": context.max_wait_ms,
            "max_output_chars": context.max_output_chars
        });
        if let Some(reason) = reuse_skipped_reason {
            if let Some(map) = response.as_object_mut() {
                map.insert(
                    "terminal_reuse_skipped_reason".to_string(),
                    Value::String(reason.to_string()),
                );
            }
        }
        return Ok(response);
    }

    let wait_result =
        wait_for_local_mcp_terminal_session(session.clone(), context.max_wait_ms).await?;
    let stdout =
        collect_local_mcp_terminal_output_by_kinds(&session, context.max_output_chars, &["stdout"])
            .await;
    let stderr =
        collect_local_mcp_terminal_output_by_kinds(&session, context.max_output_chars, &["stderr"])
            .await;
    let output = collect_local_mcp_terminal_output_by_kinds(
        &session,
        context.max_output_chars,
        &["stdout", "stderr"],
    )
    .await;
    let mut response = json!({
        "project_id": context.project_id,
        "project_root": display_project_root,
        "terminal_id": session_id.clone(),
        "process_id": session_id,
        "terminal_reused": false,
        "path": display_cwd,
        "common": command,
        "background": false,
        "busy": wait_result.busy,
        "success": wait_result.exit_code == Some(0),
        "stdout": stdout.text,
        "stderr": stderr.text,
        "output": output.text,
        "output_chars": output.char_count,
        "truncated": output.truncated,
        "finished_by": wait_result.finished_by,
        "exit_code": wait_result.exit_code,
        "idle_timeout_ms": context.idle_timeout_ms,
        "max_wait_ms": context.max_wait_ms,
        "max_output_chars": context.max_output_chars
    });
    if let Some(reason) = reuse_skipped_reason {
        if let Some(map) = response.as_object_mut() {
            map.insert(
                "terminal_reuse_skipped_reason".to_string(),
                Value::String(reason.to_string()),
            );
        }
    }
    Ok(response)
}
