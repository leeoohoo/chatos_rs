use serde_json::{Value, json};

use crate::models::terminal::{Terminal, TerminalService};
use crate::models::terminal_log::TerminalLogService;
use crate::services::terminal_manager::get_terminal_manager;

use super::super::capture::compact_recent_logs;
use super::super::context::terminal_cwd_in_root;
use super::super::{
    BoundContext, RECENT_LOGS_PER_ENTRY_MAX_CHARS, RECENT_LOGS_TOTAL_MAX_CHARS_PER_TERMINAL,
};
use super::PROCESS_SNAPSHOT_TAIL_LINES;

pub(in crate::builtin::terminal_controller) async fn get_recent_logs_with_context(
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

pub(in crate::builtin::terminal_controller) async fn list_processes_with_context(
    ctx: BoundContext,
    include_exited: bool,
    limit: usize,
) -> Result<Value, String> {
    let terminals = list_terminals_for_context(&ctx).await?;
    let total_terminals = terminals.len();
    let visible_total = terminals
        .iter()
        .filter(|terminal| include_exited || terminal.status != "exited")
        .count();

    let manager = get_terminal_manager();
    let mut terminal_results = Vec::new();
    for terminal in terminals {
        if !include_exited && terminal.status == "exited" {
            continue;
        }
        if terminal_results.len() >= limit {
            break;
        }

        let session = manager.get(terminal.id.as_str());
        let busy = session.as_ref().map(|item| item.is_busy()).unwrap_or(false);
        let output_tail = session
            .as_ref()
            .map(|item| item.output_snapshot_tail_lines(PROCESS_SNAPSHOT_TAIL_LINES))
            .unwrap_or_default();
        let status = normalize_process_status(terminal.status.as_str(), busy);

        let terminal_id = terminal.id.clone();
        terminal_results.push(json!({
            "terminal_id": terminal_id.clone(),
            "process_id": terminal_id.clone(),
            "terminal_name": terminal.name,
            "status": terminal.status,
            "process_status": status,
            "busy": busy,
            "has_session": session.is_some(),
            "command": Value::Null,
            "pid": Value::Null,
            "started_at": terminal.last_active_at,
            "uptime_seconds": Value::Null,
            "cwd": terminal.cwd,
            "project_id": terminal.project_id,
            "last_active_at": terminal.last_active_at,
            "output_preview": output_tail.clone(),
            "output_tail": output_tail,
            "output_tail_chars": output_tail.chars().count()
        }));
    }

    let terminal_count = terminal_results.len();
    let result_scope = if terminal_count == 0 {
        "no_terminal"
    } else if terminal_count > 1 {
        "multiple_terminals"
    } else {
        "single_terminal"
    };
    let process_results = terminal_results.clone();

    Ok(json!({
        "status": "ok",
        "result_scope": result_scope,
        "is_multiple_terminals": terminal_count > 1,
        "terminal_count": terminal_count,
        "process_count": terminal_count,
        "visible_total": visible_total,
        "total_terminals": total_terminals,
        "include_exited": include_exited,
        "limit": limit,
        "terminals": terminal_results,
        "processes": process_results
    }))
}

pub(super) async fn get_terminal_for_context(
    ctx: &BoundContext,
    terminal_id: &str,
) -> Result<Terminal, String> {
    let terminals = list_terminals_for_context(ctx).await?;
    terminals
        .into_iter()
        .find(|terminal| terminal.id == terminal_id)
        .ok_or_else(|| {
            format!(
                "terminal not found in current project context: {}",
                terminal_id
            )
        })
}

pub(super) fn normalize_process_status(terminal_status: &str, busy: bool) -> &'static str {
    if terminal_status == "exited" {
        "exited"
    } else if busy {
        "running"
    } else {
        "idle"
    }
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
    terminals.sort_by(|left, right| right.last_active_at.cmp(&left.last_active_at));
    Ok(terminals)
}
