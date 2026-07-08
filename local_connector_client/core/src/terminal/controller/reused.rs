// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::PathBuf;
use std::sync::Arc;

use chatos_builtin_tools::TerminalControllerContext;
use serde_json::{json, Value};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use crate::local_now_rfc3339;

use super::registry::{
    append_local_mcp_terminal_log, collect_local_mcp_terminal_output_since_by_kinds,
    local_mcp_sessions_for_context, next_local_mcp_log_offset,
    refresh_local_mcp_terminal_session_status, LocalMcpTerminalSession,
};
use super::shell::build_local_mcp_shell_command_script;

mod marker;

use marker::{
    clear_local_mcp_shell_active_marker, spawn_clear_local_mcp_shell_active_marker_when_done,
    wait_for_local_mcp_shell_command,
};

pub(super) async fn execute_local_mcp_reused_shell_command(
    context: TerminalControllerContext,
    session: Arc<LocalMcpTerminalSession>,
    _project_root: PathBuf,
    cwd: PathBuf,
    display_project_root: String,
    display_cwd: String,
    command: String,
) -> std::result::Result<Value, String> {
    let _guard = session.command_lock.lock().await;
    refresh_local_mcp_terminal_session_status(&session).await?;
    {
        let meta = session.meta.lock().await;
        if meta.status == "exited" {
            return Err("primary terminal has exited".to_string());
        }
    }
    {
        let mut active = session.active_shell_marker.lock().await;
        if active.is_some() {
            return Err(
                "primary terminal is busy; run long commands with background=true".to_string(),
            );
        }
        let marker = format!("__CHATO_LOCAL_CMD_DONE_{}__", Uuid::new_v4().simple());
        *active = Some(marker);
    }

    let active_marker = session
        .active_shell_marker
        .lock()
        .await
        .clone()
        .ok_or_else(|| "primary terminal marker is unavailable".to_string())?;
    let start_marker = active_marker.replace("_DONE_", "_START_");
    {
        let mut meta = session.meta.lock().await;
        meta.cwd = cwd.to_string_lossy().to_string();
        meta.last_active_at = local_now_rfc3339();
    }
    append_local_mcp_terminal_log(session.clone(), "command", command.clone()).await;
    let output_start_offset = next_local_mcp_log_offset(&session).await;
    let script = build_local_mcp_shell_command_script(
        cwd.as_path(),
        command.as_str(),
        start_marker.as_str(),
        active_marker.as_str(),
    );
    let write_result = async {
        let mut stdin = session.stdin.lock().await;
        let Some(stdin) = stdin.as_mut() else {
            return Err("primary terminal stdin is unavailable".to_string());
        };
        stdin
            .write_all(script.as_bytes())
            .await
            .map_err(|err| err.to_string())?;
        stdin.flush().await.map_err(|err| err.to_string())
    }
    .await;
    if let Err(err) = write_result {
        clear_local_mcp_shell_active_marker(&session, active_marker.as_str()).await;
        return Err(err);
    }

    let wait_result = wait_for_local_mcp_shell_command(
        session.clone(),
        active_marker.as_str(),
        context.max_wait_ms,
    )
    .await?;
    if wait_result.timed_out {
        spawn_clear_local_mcp_shell_active_marker_when_done(session.clone(), active_marker.clone());
    } else {
        clear_local_mcp_shell_active_marker(&session, active_marker.as_str()).await;
    }

    let stdout = collect_local_mcp_terminal_output_since_by_kinds(
        &session,
        output_start_offset,
        context.max_output_chars,
        &["stdout"],
    )
    .await;
    let stderr = collect_local_mcp_terminal_output_since_by_kinds(
        &session,
        output_start_offset,
        context.max_output_chars,
        &["stderr"],
    )
    .await;
    let output = collect_local_mcp_terminal_output_since_by_kinds(
        &session,
        output_start_offset,
        context.max_output_chars,
        &["stdout", "stderr"],
    )
    .await;
    let session_id = session.meta.lock().await.id.clone();
    Ok(json!({
        "project_id": context.project_id,
        "project_root": display_project_root,
        "terminal_id": session_id.clone(),
        "process_id": session_id,
        "terminal_reused": true,
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
    }))
}

pub(super) async fn find_local_mcp_primary_shell_session(
    context: &TerminalControllerContext,
) -> std::result::Result<Option<Arc<LocalMcpTerminalSession>>, String> {
    let sessions = local_mcp_sessions_for_context(context).await?;
    for session in sessions {
        refresh_local_mcp_terminal_session_status(&session).await?;
        let is_match = {
            let meta = session.meta.lock().await;
            meta.status != "exited" && is_local_mcp_primary_shell_command(meta.command.as_str())
        };
        if is_match {
            return Ok(Some(session));
        }
    }
    Ok(None)
}

pub(super) fn is_local_mcp_primary_shell_command(command: &str) -> bool {
    command.starts_with("task terminal shell:")
}

pub(super) async fn local_mcp_shell_session_is_busy(
    session: &Arc<LocalMcpTerminalSession>,
) -> bool {
    session.active_shell_marker.lock().await.is_some()
}
