use std::path::Path;

use serde_json::{json, Value};
use tokio::time::Duration;

use crate::models::terminal::{Terminal, TerminalService};
use crate::models::terminal_log::{TerminalLog, TerminalLogService};
use crate::services::terminal_manager::get_terminal_manager;

use super::capture::{capture_command_output, compact_recent_logs};
use super::context::{
    build_input_payload, derive_terminal_name, resolve_project_root, resolve_target_path,
    terminal_cwd_in_root,
};
use super::{
    BoundContext, RECENT_LOGS_PER_ENTRY_MAX_CHARS, RECENT_LOGS_TOTAL_MAX_CHARS_PER_TERMINAL,
};

pub(super) async fn execute_command_with_context(
    ctx: BoundContext,
    path_input: &str,
    command: &str,
) -> Result<Value, String> {
    let (project_id, project_root) = resolve_project_root(&ctx).await?;
    let target_path = resolve_target_path(project_root.as_path(), path_input)?;

    let manager = get_terminal_manager();
    let (terminal, reused) = if let Some(idle) =
        find_idle_terminal(&project_id, project_root.as_path(), ctx.user_id.as_deref()).await?
    {
        (idle, true)
    } else {
        let name = derive_terminal_name(project_root.as_path());
        let created = manager
            .create(
                name,
                project_root.to_string_lossy().to_string(),
                ctx.user_id.clone(),
                project_id.clone(),
            )
            .await?;
        (created, false)
    };

    let session = manager.ensure_running(&terminal).await?;
    let mut receiver = session.subscribe();

    let input_data = build_input_payload(project_root.as_path(), target_path.as_path(), command);
    session.write_input(input_data.as_str())?;

    let trimmed_command = command.trim();
    if !trimmed_command.is_empty() {
        let cmd_log = TerminalLog::new(
            terminal.id.clone(),
            "command".to_string(),
            trimmed_command.to_string(),
        );
        let _ = TerminalLogService::create(cmd_log).await;
    }
    if !input_data.is_empty() {
        let input_log = TerminalLog::new(terminal.id.clone(), "input".to_string(), input_data);
        let _ = TerminalLogService::create(input_log).await;
    }
    let _ = TerminalService::touch(terminal.id.as_str()).await;

    let capture = capture_command_output(
        &mut receiver,
        Duration::from_millis(ctx.idle_timeout_ms),
        Duration::from_millis(ctx.max_wait_ms),
        ctx.max_output_chars,
    )
    .await;

    Ok(json!({
        "project_id": project_id,
        "project_root": project_root.to_string_lossy(),
        "terminal_id": terminal.id,
        "terminal_reused": reused,
        "path": target_path.to_string_lossy(),
        "common": command,
        "output": capture.output,
        "output_chars": capture.output.chars().count(),
        "truncated": capture.truncated,
        "finished_by": capture.finished_by,
        "idle_timeout_ms": ctx.idle_timeout_ms,
        "max_wait_ms": ctx.max_wait_ms,
        "max_output_chars": ctx.max_output_chars
    }))
}

pub(super) async fn get_recent_logs_with_context(
    ctx: BoundContext,
    per_terminal_limit: i64,
    terminal_limit: usize,
) -> Result<Value, String> {
    let terminals = list_terminals_for_context(&ctx).await?;
    let total_terminals = terminals.len();

    if total_terminals == 0 {
        return Ok(json!({
            "result_scope": "no_terminal",
            "is_multiple_terminals": false,
            "terminal_count": 0,
            "total_terminals": 0,
            "per_terminal_limit": per_terminal_limit,
            "terminal_limit": terminal_limit,
            "terminals": []
        }));
    }

    let mut selected = terminals;
    if selected.len() > terminal_limit {
        selected.truncate(terminal_limit);
    }

    let mut terminal_results = Vec::new();
    for terminal in selected {
        let logs =
            TerminalLogService::list_recent(terminal.id.as_str(), per_terminal_limit).await?;
        let (compact_logs, truncation) = compact_recent_logs(
            logs.as_slice(),
            RECENT_LOGS_PER_ENTRY_MAX_CHARS,
            RECENT_LOGS_TOTAL_MAX_CHARS_PER_TERMINAL,
        );
        terminal_results.push(json!({
            "terminal_id": terminal.id,
            "terminal_name": terminal.name,
            "status": terminal.status,
            "cwd": terminal.cwd,
            "project_id": terminal.project_id,
            "last_active_at": terminal.last_active_at,
            "log_count": logs.len(),
            "returned_log_count": compact_logs.len(),
            "truncated": truncation.get("truncated").and_then(Value::as_bool).unwrap_or(false),
            "truncation": truncation,
            "logs": compact_logs
        }));
    }

    let terminal_count = terminal_results.len();
    let result_scope = if terminal_count > 1 {
        "multiple_terminals"
    } else {
        "single_terminal"
    };

    Ok(json!({
        "result_scope": result_scope,
        "is_multiple_terminals": terminal_count > 1,
        "terminal_count": terminal_count,
        "total_terminals": total_terminals,
        "per_terminal_limit": per_terminal_limit,
        "terminal_limit": terminal_limit,
        "terminals": terminal_results
    }))
}

async fn list_terminals_for_context(ctx: &BoundContext) -> Result<Vec<Terminal>, String> {
    let mut terminals = TerminalService::list(ctx.user_id.clone()).await?;
    terminals.retain(|terminal| {
        if let Some(pid) = ctx.project_id.as_deref() {
            terminal.project_id.as_deref() == Some(pid)
        } else {
            terminal_cwd_in_root(terminal.cwd.as_str(), ctx.root.as_path())
        }
    });
    terminals.sort_by(|a, b| b.last_active_at.cmp(&a.last_active_at));
    Ok(terminals)
}

async fn find_idle_terminal(
    project_id: &Option<String>,
    project_root: &Path,
    user_id: Option<&str>,
) -> Result<Option<Terminal>, String> {
    let terminals = TerminalService::list(user_id.map(|v| v.to_string())).await?;
    let manager = get_terminal_manager();

    for terminal in terminals {
        if terminal.status == "exited" {
            continue;
        }

        if let Some(pid) = project_id.as_deref() {
            if terminal.project_id.as_deref() != Some(pid) {
                continue;
            }
        } else if !terminal_cwd_in_root(terminal.cwd.as_str(), project_root) {
            continue;
        }

        let busy = manager.get_busy(terminal.id.as_str()).unwrap_or(false);
        if !busy {
            return Ok(Some(terminal));
        }
    }

    Ok(None)
}
