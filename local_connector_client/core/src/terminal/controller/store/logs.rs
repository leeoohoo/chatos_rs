// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::Path;

use chatos_builtin_tools::TerminalControllerContext;
use serde_json::{json, Value};

use super::super::output::take_recent_local_mcp_logs;
use super::super::registry::{
    local_mcp_sessions_for_context, refresh_local_mcp_terminal_session_status,
};
use super::super::shell::{
    canonicalize_terminal_root, derive_local_mcp_terminal_name, display_local_mcp_workspace_path,
};

pub(super) async fn get_recent_logs(
    context: TerminalControllerContext,
    per_terminal_limit: i64,
    terminal_limit: usize,
) -> std::result::Result<Value, String> {
    let sessions = local_mcp_sessions_for_context(&context).await?;
    let project_root = canonicalize_terminal_root(context.root.as_path())?;
    let total = sessions.len();
    let mut terminals = Vec::new();
    for session in sessions.into_iter().take(terminal_limit) {
        refresh_local_mcp_terminal_session_status(&session).await?;
        let meta = session.meta.lock().await.clone();
        let logs = session.logs.lock().await;
        let recent = take_recent_local_mcp_logs(&logs, per_terminal_limit.max(1) as usize);
        let cwd =
            display_local_mcp_workspace_path(project_root.as_path(), Path::new(meta.cwd.as_str()));
        terminals.push(json!({
            "terminal_id": meta.id,
            "terminal_name": derive_local_mcp_terminal_name(cwd.as_str()),
            "status": meta.status,
            "cwd": cwd,
            "project_id": meta.project_id,
            "last_active_at": meta.last_active_at,
            "log_count": logs.len(),
            "returned_log_count": recent.len(),
            "truncated": false,
            "truncation": { "truncated": false },
            "logs": recent,
        }));
    }
    Ok(json!({
        "result_scope": if terminals.len() > 1 { "multiple_terminals" } else if terminals.is_empty() { "no_terminal" } else { "single_terminal" },
        "is_multiple_terminals": terminals.len() > 1,
        "terminal_count": terminals.len(),
        "total_terminals": total,
        "per_terminal_limit": per_terminal_limit,
        "terminal_limit": terminal_limit,
        "terminals": terminals,
    }))
}
