// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use chatos_mcp::TerminalControllerContext;

use super::lifecycle::refresh_local_mcp_terminal_session_status;
use super::local_mcp_terminal_registry;
use super::types::{LocalMcpTerminalSession, LocalMcpTerminalWaitResult};
use crate::terminal::controller::shell::canonicalize_terminal_root;
use crate::terminal::controller::{
    is_local_mcp_primary_shell_command, local_mcp_shell_session_is_busy,
};

pub(in crate::terminal::controller) async fn local_mcp_sessions_for_context(
    context: &TerminalControllerContext,
) -> std::result::Result<Vec<Arc<LocalMcpTerminalSession>>, String> {
    let root = canonicalize_terminal_root(context.root.as_path())?;
    let sessions = local_mcp_terminal_registry().sessions.read().await;
    let mut matched = Vec::new();
    for session in sessions.values() {
        let meta = session.meta.lock().await.clone();
        let same_user = match context.user_id.as_deref() {
            Some(user_id) => meta.user_id.as_deref() == Some(user_id),
            None => true,
        };
        let same_project = match context.project_id.as_deref() {
            Some(project_id) => meta.project_id.as_deref() == Some(project_id),
            None => true,
        };
        let same_root = Path::new(meta.root.as_str()) == root.as_path();
        if same_user && same_project && same_root {
            matched.push(session.clone());
        }
    }
    matched.sort_by(|left, right| {
        let left = left.meta.try_lock();
        let right = right.meta.try_lock();
        match (left, right) {
            (Ok(left), Ok(right)) => right.last_active_at.cmp(&left.last_active_at),
            _ => std::cmp::Ordering::Equal,
        }
    });
    Ok(matched)
}

pub(in crate::terminal::controller) async fn local_mcp_session_for_context(
    context: &TerminalControllerContext,
    terminal_id: &str,
) -> std::result::Result<Arc<LocalMcpTerminalSession>, String> {
    let sessions = local_mcp_sessions_for_context(context).await?;
    sessions
        .into_iter()
        .find(|session| {
            session
                .meta
                .try_lock()
                .map(|meta| meta.id == terminal_id)
                .unwrap_or(false)
        })
        .ok_or_else(|| format!("terminal not found in current project context: {terminal_id}"))
}

pub(in crate::terminal::controller) async fn wait_for_local_mcp_terminal_session(
    session: Arc<LocalMcpTerminalSession>,
    timeout_ms: u64,
) -> std::result::Result<LocalMcpTerminalWaitResult, String> {
    let timeout = Duration::from_millis(timeout_ms.clamp(1_000, 600_000));
    let started = std::time::Instant::now();
    loop {
        refresh_local_mcp_terminal_session_status(&session).await?;
        let meta = session.meta.lock().await.clone();
        if meta.status == "exited" {
            return Ok(LocalMcpTerminalWaitResult {
                waited_ms: started.elapsed().as_millis() as u64,
                busy: false,
                timed_out: false,
                finished_by: "exit",
                exit_code: meta.exit_code,
            });
        }
        if is_local_mcp_primary_shell_command(meta.command.as_str())
            && !local_mcp_shell_session_is_busy(&session).await
        {
            return Ok(LocalMcpTerminalWaitResult {
                waited_ms: started.elapsed().as_millis() as u64,
                busy: false,
                timed_out: false,
                finished_by: "idle",
                exit_code: meta.exit_code,
            });
        }
        if started.elapsed() >= timeout {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let meta = session.meta.lock().await.clone();
    Ok(LocalMcpTerminalWaitResult {
        waited_ms: started.elapsed().as_millis() as u64,
        busy: meta.status != "exited",
        timed_out: true,
        finished_by: "timeout",
        exit_code: meta.exit_code,
    })
}
