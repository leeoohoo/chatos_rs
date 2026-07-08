// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use crate::local_now_rfc3339;

use super::super::output::{
    collect_local_mcp_output_from_logs, collect_local_mcp_output_from_strings,
    strip_local_mcp_internal_shell_markers, LocalMcpTerminalOutput,
};
use super::types::{LocalMcpTerminalLog, LocalMcpTerminalSession};

pub(in crate::terminal::controller) async fn next_local_mcp_log_offset(
    session: &Arc<LocalMcpTerminalSession>,
) -> i64 {
    let logs = session.logs.lock().await;
    logs.last().map(|entry| entry.offset + 1).unwrap_or(0)
}

pub(in crate::terminal::controller) async fn collect_local_mcp_terminal_output_since_by_kinds(
    session: &Arc<LocalMcpTerminalSession>,
    min_offset: i64,
    max_chars: usize,
    kinds: &[&str],
) -> LocalMcpTerminalOutput {
    let logs = session.logs.lock().await;
    collect_local_mcp_output_from_strings(
        logs.iter()
            .filter(|entry| entry.offset >= min_offset)
            .filter(|entry| kinds.iter().any(|kind| *kind == entry.kind))
            .map(|entry| strip_local_mcp_internal_shell_markers(entry.content.as_str())),
        max_chars,
    )
}

pub(in crate::terminal::controller) async fn append_local_mcp_terminal_log(
    session: Arc<LocalMcpTerminalSession>,
    kind: &str,
    content: String,
) {
    if content.is_empty() {
        return;
    }
    let now = local_now_rfc3339();
    {
        let mut logs = session.logs.lock().await;
        let offset = logs.last().map(|entry| entry.offset + 1).unwrap_or(0);
        logs.push(LocalMcpTerminalLog {
            offset,
            kind: kind.to_string(),
            content,
            created_at: now.clone(),
        });
        if logs.len() > 4_000 {
            let drain = logs.len() - 4_000;
            logs.drain(0..drain);
        }
    }
    let mut meta = session.meta.lock().await;
    meta.last_active_at = now;
}

pub(in crate::terminal::controller) async fn collect_local_mcp_terminal_output(
    session: &Arc<LocalMcpTerminalSession>,
    max_chars: usize,
) -> LocalMcpTerminalOutput {
    collect_local_mcp_terminal_output_by_kinds(session, max_chars, &["stdout", "stderr"]).await
}

pub(in crate::terminal::controller) async fn collect_local_mcp_terminal_output_by_kinds(
    session: &Arc<LocalMcpTerminalSession>,
    max_chars: usize,
    kinds: &[&str],
) -> LocalMcpTerminalOutput {
    let logs = session.logs.lock().await;
    collect_local_mcp_output_from_logs(
        logs.iter()
            .filter(|entry| kinds.iter().any(|kind| *kind == entry.kind))
            .map(|entry| entry.content.as_str()),
        max_chars,
    )
}
