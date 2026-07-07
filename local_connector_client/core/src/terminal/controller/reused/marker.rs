// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;
use std::time::Duration;

use super::super::registry::{
    refresh_local_mcp_terminal_session_status, LocalMcpTerminalSession, LocalMcpTerminalWaitResult,
};

pub(super) async fn wait_for_local_mcp_shell_command(
    session: Arc<LocalMcpTerminalSession>,
    done_marker: &str,
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
        if let Some(exit_code) = local_mcp_shell_done_exit_code(&session, done_marker).await {
            return Ok(LocalMcpTerminalWaitResult {
                waited_ms: started.elapsed().as_millis() as u64,
                busy: false,
                timed_out: false,
                finished_by: "sentinel",
                exit_code: Some(exit_code),
            });
        }
        if started.elapsed() >= timeout {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Ok(LocalMcpTerminalWaitResult {
        waited_ms: started.elapsed().as_millis() as u64,
        busy: true,
        timed_out: true,
        finished_by: "timeout",
        exit_code: None,
    })
}

pub(super) async fn clear_local_mcp_shell_active_marker(
    session: &Arc<LocalMcpTerminalSession>,
    done_marker: &str,
) {
    let mut active = session.active_shell_marker.lock().await;
    if active.as_deref() == Some(done_marker) {
        *active = None;
    }
}

pub(super) fn spawn_clear_local_mcp_shell_active_marker_when_done(
    session: Arc<LocalMcpTerminalSession>,
    done_marker: String,
) {
    tokio::spawn(async move {
        let started = std::time::Instant::now();
        loop {
            if refresh_local_mcp_terminal_session_status(&session)
                .await
                .is_err()
            {
                clear_local_mcp_shell_active_marker(&session, done_marker.as_str()).await;
                break;
            }
            let exited = {
                let meta = session.meta.lock().await;
                meta.status == "exited"
            };
            if exited
                || local_mcp_shell_done_exit_code(&session, done_marker.as_str())
                    .await
                    .is_some()
            {
                clear_local_mcp_shell_active_marker(&session, done_marker.as_str()).await;
                break;
            }
            if started.elapsed() >= Duration::from_secs(10 * 60) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    });
}

async fn local_mcp_shell_done_exit_code(
    session: &Arc<LocalMcpTerminalSession>,
    done_marker: &str,
) -> Option<i32> {
    let logs = session.logs.lock().await;
    let text = logs
        .iter()
        .filter(|entry| matches!(entry.kind.as_str(), "stdout" | "stderr"))
        .map(|entry| entry.content.as_str())
        .collect::<Vec<_>>()
        .join("");
    parse_local_mcp_shell_done_exit_code(text.as_str(), done_marker)
}

fn parse_local_mcp_shell_done_exit_code(text: &str, done_marker: &str) -> Option<i32> {
    let needle = format!("{done_marker}:");
    let start = text.find(needle.as_str())? + needle.len();
    let code = text[start..]
        .trim_start()
        .chars()
        .take_while(|ch| ch.is_ascii_digit() || *ch == '-')
        .collect::<String>();
    if code.is_empty() {
        return None;
    }
    code.parse::<i32>().ok()
}
