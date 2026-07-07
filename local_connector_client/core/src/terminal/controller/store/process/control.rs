// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::Path;

use chatos_builtin_tools::TerminalControllerContext;
use serde_json::{json, Value};
use tokio::io::AsyncWriteExt;

use super::super::super::registry::{
    append_local_mcp_terminal_log, collect_local_mcp_terminal_output,
    local_mcp_session_for_context, mark_local_mcp_terminal_exited,
    refresh_local_mcp_terminal_session_status, wait_for_local_mcp_terminal_session,
};
use super::super::super::shell::{
    canonicalize_terminal_root, derive_local_mcp_terminal_name, display_local_mcp_workspace_path,
};

pub(in crate::terminal::controller::store) async fn process_wait(
    context: TerminalControllerContext,
    terminal_id: String,
    timeout_ms: u64,
) -> std::result::Result<Value, String> {
    let session = local_mcp_session_for_context(&context, terminal_id.as_str()).await?;
    let result = wait_for_local_mcp_terminal_session(session.clone(), timeout_ms).await?;
    let output = collect_local_mcp_terminal_output(&session, context.max_output_chars).await;
    let meta = session.meta.lock().await.clone();
    let project_root = canonicalize_terminal_root(context.root.as_path())?;
    let cwd =
        display_local_mcp_workspace_path(project_root.as_path(), Path::new(meta.cwd.as_str()));
    Ok(json!({
        "terminal_id": meta.id,
        "process_id": meta.id,
        "terminal_name": derive_local_mcp_terminal_name(cwd.as_str()),
        "status": meta.status,
        "wait_status": if result.timed_out { "timeout" } else if meta.status == "exited" { "exited" } else { "running" },
        "busy": result.busy,
        "exited": meta.status == "exited",
        "completed": !result.timed_out,
        "timed_out": result.timed_out,
        "finished_by": result.finished_by,
        "exit_code": result.exit_code,
        "timeout_ms": timeout_ms,
        "waited_ms": result.waited_ms,
        "output": output.text,
        "output_preview": output.text,
        "output_chars": output.char_count,
        "truncated": output.truncated,
    }))
}

pub(in crate::terminal::controller::store) async fn process_write(
    context: TerminalControllerContext,
    terminal_id: String,
    data: String,
    submit: bool,
) -> std::result::Result<Value, String> {
    let session = local_mcp_session_for_context(&context, terminal_id.as_str()).await?;
    refresh_local_mcp_terminal_session_status(&session).await?;
    {
        let mut stdin = session.stdin.lock().await;
        let Some(stdin) = stdin.as_mut() else {
            return Err("terminal stdin is unavailable".to_string());
        };
        stdin
            .write_all(data.as_bytes())
            .await
            .map_err(|err| err.to_string())?;
        if submit {
            stdin
                .write_all(b"\n")
                .await
                .map_err(|err| err.to_string())?;
        }
        stdin.flush().await.map_err(|err| err.to_string())?;
    }
    let mut content = data.clone();
    if submit {
        content.push('\n');
    }
    append_local_mcp_terminal_log(session, "input", content).await;
    Ok(json!({
        "ok": true,
        "terminal_id": terminal_id,
        "bytes_written": data.len() + usize::from(submit),
        "submit": submit,
    }))
}

pub(in crate::terminal::controller::store) async fn process_kill(
    context: TerminalControllerContext,
    terminal_id: String,
) -> std::result::Result<Value, String> {
    let session = local_mcp_session_for_context(&context, terminal_id.as_str()).await?;
    {
        let mut child = session.child.lock().await;
        child.kill().await.map_err(|err| err.to_string())?;
        let _ = child.wait().await;
    }
    mark_local_mcp_terminal_exited(&session, None).await;
    append_local_mcp_terminal_log(session, "system", "[terminal killed]\n".to_string()).await;
    Ok(json!({
        "ok": true,
        "terminal_id": terminal_id,
        "killed": true,
    }))
}
