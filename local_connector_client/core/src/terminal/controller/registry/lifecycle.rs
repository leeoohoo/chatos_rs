// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::PathBuf;
use std::sync::Arc;

use chatos_builtin_tools::TerminalControllerContext;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::local_now_rfc3339;

use super::local_mcp_terminal_registry;
use super::logs::append_local_mcp_terminal_log;
use super::types::{LocalMcpTerminalMeta, LocalMcpTerminalSession};

pub(in crate::terminal::controller) async fn register_local_mcp_terminal_session(
    context: TerminalControllerContext,
    root: PathBuf,
    cwd: PathBuf,
    command: String,
    mut child: tokio::process::Child,
) -> std::result::Result<Arc<LocalMcpTerminalSession>, String> {
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdin = child.stdin.take();
    let session_id = format!("local-proc-{}", Uuid::new_v4());
    let now = local_now_rfc3339();
    let session = Arc::new(LocalMcpTerminalSession {
        meta: Mutex::new(LocalMcpTerminalMeta {
            id: session_id.clone(),
            root: root.to_string_lossy().to_string(),
            cwd: cwd.to_string_lossy().to_string(),
            project_id: context.project_id,
            user_id: context.user_id,
            command,
            started_at: now.clone(),
            last_active_at: now,
            finished_at: None,
            status: "running".to_string(),
            exit_code: None,
        }),
        child: Mutex::new(child),
        stdin: Mutex::new(stdin),
        logs: Mutex::new(Vec::new()),
        command_lock: Mutex::new(()),
        active_shell_marker: Mutex::new(None),
    });
    if let Some(stdout) = stdout {
        spawn_local_mcp_terminal_reader(session.clone(), stdout, "stdout");
    }
    if let Some(stderr) = stderr {
        spawn_local_mcp_terminal_reader(session.clone(), stderr, "stderr");
    }
    local_mcp_terminal_registry()
        .sessions
        .write()
        .await
        .insert(session_id, session.clone());
    Ok(session)
}

pub(in crate::terminal::controller) async fn refresh_local_mcp_terminal_session_status(
    session: &Arc<LocalMcpTerminalSession>,
) -> std::result::Result<(), String> {
    {
        let meta = session.meta.lock().await;
        if meta.status == "exited" {
            return Ok(());
        }
    }
    let status = {
        let mut child = session.child.lock().await;
        child.try_wait().map_err(|err| err.to_string())?
    };
    if let Some(status) = status {
        mark_local_mcp_terminal_exited(session, status.code()).await;
    }
    Ok(())
}

pub(in crate::terminal::controller) async fn mark_local_mcp_terminal_exited(
    session: &Arc<LocalMcpTerminalSession>,
    exit_code: Option<i32>,
) {
    let mut meta = session.meta.lock().await;
    if meta.status == "exited" {
        return;
    }
    meta.status = "exited".to_string();
    meta.exit_code = exit_code;
    meta.finished_at = Some(local_now_rfc3339());
    meta.last_active_at = meta.finished_at.clone().unwrap_or_else(local_now_rfc3339);
}

fn spawn_local_mcp_terminal_reader<R>(
    session: Arc<LocalMcpTerminalSession>,
    mut reader: R,
    kind: &'static str,
) where
    R: AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut buf = vec![0_u8; 2048];
        loop {
            match reader.read(buf.as_mut_slice()).await {
                Ok(0) => break,
                Ok(count) => {
                    let chunk = String::from_utf8_lossy(&buf[..count]).to_string();
                    append_local_mcp_terminal_log(session.clone(), kind, chunk).await;
                }
                Err(_) => break,
            }
        }
    });
}
