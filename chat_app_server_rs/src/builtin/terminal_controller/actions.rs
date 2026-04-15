use std::path::Path;

use serde_json::{json, Value};
use tokio::time::{Duration, Instant};

use crate::models::terminal::{Terminal, TerminalService};
use crate::models::terminal_log::{TerminalLog, TerminalLogService};
use crate::services::terminal_manager::{get_terminal_manager, TerminalEvent};

use super::capture::{capture_command_output, compact_recent_logs};
use super::context::{
    build_input_payload, derive_terminal_name, resolve_project_root, resolve_target_path,
    terminal_cwd_in_root,
};
use super::{
    BoundContext, PROCESS_WAIT_MAX_TIMEOUT_MS, RECENT_LOGS_PER_ENTRY_MAX_CHARS,
    RECENT_LOGS_TOTAL_MAX_CHARS_PER_TERMINAL,
};

const PROCESS_SNAPSHOT_TAIL_LINES: usize = 80;
const PROCESS_POLL_OFFSET_LIMIT_MAX: i64 = 500;
const PROCESS_WRITE_MAX_CHARS: usize = 32_768;

pub(super) async fn execute_command_with_context(
    ctx: BoundContext,
    path_input: &str,
    command: &str,
    background: bool,
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
    let terminal_id = terminal.id.clone();

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

    if background {
        let busy = manager.get_busy(terminal.id.as_str()).unwrap_or(false);
        return Ok(json!({
            "project_id": project_id,
            "project_root": project_root.to_string_lossy(),
            "terminal_id": terminal_id.clone(),
            "process_id": terminal_id.clone(),
            "terminal_reused": reused,
            "path": target_path.to_string_lossy(),
            "common": command,
            "background": true,
            "busy": busy,
            "output": "",
            "output_chars": 0,
            "truncated": false,
            "finished_by": "background",
            "idle_timeout_ms": ctx.idle_timeout_ms,
            "max_wait_ms": ctx.max_wait_ms,
            "max_output_chars": ctx.max_output_chars
        }));
    }

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
        "terminal_id": terminal_id.clone(),
        "process_id": terminal_id.clone(),
        "terminal_reused": reused,
        "path": target_path.to_string_lossy(),
        "common": command,
        "background": false,
        "busy": manager.get_busy(terminal_id.as_str()).unwrap_or(false),
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

pub(super) async fn list_processes_with_context(
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

pub(super) async fn poll_process_with_context(
    ctx: BoundContext,
    terminal_id: &str,
    offset: Option<i64>,
    limit: i64,
) -> Result<Value, String> {
    let terminal = get_terminal_for_context(&ctx, terminal_id).await?;
    let manager = get_terminal_manager();
    let effective_limit = limit.clamp(1, PROCESS_POLL_OFFSET_LIMIT_MAX);
    let requested_offset = offset.map(|value| value.max(0));

    let logs = if let Some(offset_value) = requested_offset {
        TerminalLogService::list(terminal_id, Some(effective_limit), offset_value).await?
    } else {
        TerminalLogService::list_recent(terminal_id, effective_limit).await?
    };
    let fetched_log_count = logs.len();
    let (compact_logs, truncation) = compact_recent_logs(
        logs.as_slice(),
        RECENT_LOGS_PER_ENTRY_MAX_CHARS,
        RECENT_LOGS_TOTAL_MAX_CHARS_PER_TERMINAL,
    );

    let session = manager.get(terminal_id);
    let busy = session.as_ref().map(|item| item.is_busy()).unwrap_or(false);
    let output_tail = session
        .as_ref()
        .map(|item| item.output_snapshot_tail_lines(PROCESS_SNAPSHOT_TAIL_LINES))
        .unwrap_or_default();
    let status = normalize_process_status(terminal.status.as_str(), busy);

    Ok(json!({
        "terminal_id": terminal.id.clone(),
        "process_id": terminal.id.clone(),
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
        "mode": if requested_offset.is_some() { "offset" } else { "recent" },
        "requested_offset": requested_offset,
        "next_offset": requested_offset.map(|value| value + fetched_log_count as i64),
        "limit": effective_limit,
        "fetched_log_count": fetched_log_count,
        "returned_log_count": compact_logs.len(),
        "has_more": requested_offset.is_some() && fetched_log_count as i64 >= effective_limit,
        "truncated": truncation.get("truncated").and_then(Value::as_bool).unwrap_or(false),
        "truncation": truncation,
        "logs": compact_logs,
        "output_preview": output_tail.clone(),
        "output_tail": output_tail,
        "output_tail_chars": output_tail.chars().count()
    }))
}

pub(super) async fn read_process_log_with_context(
    ctx: BoundContext,
    terminal_id: &str,
    offset: Option<i64>,
    limit: i64,
) -> Result<Value, String> {
    let poll = poll_process_with_context(ctx, terminal_id, offset, limit).await?;
    let status = poll
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("running")
        .to_string();
    let terminal_id = poll
        .get("terminal_id")
        .cloned()
        .unwrap_or_else(|| Value::String(terminal_id.to_string()));
    let logs = poll
        .get("logs")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut line_buffer = Vec::new();
    for entry in &logs {
        if let Some(content) = entry.get("content").and_then(Value::as_str) {
            line_buffer.extend(content.lines().map(|line| line.to_string()));
        }
    }

    let requested_offset = offset.unwrap_or(0).max(0) as usize;
    let requested_limit = limit.max(1) as usize;
    let total_lines = line_buffer.len();

    let selected: Vec<String> = if offset.is_some() {
        line_buffer
            .iter()
            .skip(requested_offset)
            .take(requested_limit)
            .cloned()
            .collect()
    } else {
        if total_lines > requested_limit {
            line_buffer
                .into_iter()
                .skip(total_lines - requested_limit)
                .collect()
        } else {
            line_buffer
        }
    };
    let output = selected.join("\n");

    Ok(json!({
        "terminal_id": terminal_id,
        "status": status,
        "output": output,
        "total_lines": total_lines,
        "showing": format!("{} lines", selected.len()),
        "offset": offset.map(|_| requested_offset),
        "limit": requested_limit,
        "next_offset": if offset.is_some() {
            Some((requested_offset + selected.len()).min(total_lines))
        } else {
            None
        },
        "has_more": offset.is_some() && requested_offset + selected.len() < total_lines
    }))
}

pub(super) async fn wait_process_with_context(
    ctx: BoundContext,
    terminal_id: &str,
    timeout_ms: u64,
) -> Result<Value, String> {
    let terminal = get_terminal_for_context(&ctx, terminal_id).await?;
    let manager = get_terminal_manager();

    if terminal.status == "exited" {
        return Ok(json!({
            "terminal_id": terminal.id.clone(),
            "process_id": terminal.id.clone(),
            "terminal_name": terminal.name,
            "status": terminal.status,
            "wait_status": "exited",
            "busy": false,
            "exited": true,
            "completed": true,
            "timed_out": false,
            "finished_by": "already_exited",
            "exit_code": Value::Null,
            "timeout_ms": timeout_ms,
            "waited_ms": 0,
            "output": "",
            "output_preview": "",
            "output_chars": 0,
            "truncated": false,
            "timeout_note": Value::Null
        }));
    }

    let session = manager.ensure_running(&terminal).await?;
    let mut receiver = session.subscribe();
    let effective_timeout_ms = timeout_ms.max(1_000).min(PROCESS_WAIT_MAX_TIMEOUT_MS);
    let timeout = Duration::from_millis(effective_timeout_ms);
    let started_at = Instant::now();
    let mut output = String::new();
    let mut truncated = false;
    let mut exit_code: Option<i32> = None;
    let mut completed = false;
    let mut finished_by = "timeout";

    let mut busy_now = session.is_busy();
    if !busy_now {
        completed = true;
        finished_by = "already_idle";
    }

    while !completed {
        let elapsed = started_at.elapsed();
        if elapsed >= timeout {
            break;
        }

        let remaining = timeout.saturating_sub(elapsed);
        match tokio::time::timeout(remaining, receiver.recv()).await {
            Ok(Ok(TerminalEvent::Output(chunk))) => {
                append_output_tail(
                    &mut output,
                    chunk.as_str(),
                    ctx.max_output_chars,
                    &mut truncated,
                );
            }
            Ok(Ok(TerminalEvent::Exit(code))) => {
                append_output_tail(
                    &mut output,
                    format!("\n[terminal exited with code {code}]\n").as_str(),
                    ctx.max_output_chars,
                    &mut truncated,
                );
                exit_code = Some(code);
                completed = true;
                busy_now = false;
                finished_by = "terminal_exit";
            }
            Ok(Ok(TerminalEvent::State(busy))) => {
                busy_now = busy;
                if !busy {
                    completed = true;
                    finished_by = "idle";
                }
            }
            Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => {}
            Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) => {
                completed = true;
                finished_by = "receiver_closed";
                busy_now = session.is_busy();
            }
            Err(_) => break,
        }
    }

    let waited_ms = started_at.elapsed().as_millis() as u64;
    let terminal_now = TerminalService::get_by_id(terminal_id)
        .await?
        .unwrap_or_else(|| terminal.clone());
    let exited = terminal_now.status == "exited";
    if exited {
        busy_now = false;
        if !completed {
            completed = true;
            finished_by = "terminal_exited";
        }
    } else if !completed {
        busy_now = manager.get_busy(terminal_id).unwrap_or(false);
        if !busy_now {
            completed = true;
            finished_by = "idle";
        }
    }
    let timed_out = !completed;
    let wait_status = if timed_out {
        "timeout"
    } else if exited {
        "exited"
    } else {
        "completed"
    };
    let timeout_note = if timed_out {
        Some(format!(
            "Waited {}ms and process is still active",
            effective_timeout_ms
        ))
    } else {
        None
    };

    Ok(json!({
        "terminal_id": terminal_now.id.clone(),
        "process_id": terminal_now.id.clone(),
        "terminal_name": terminal_now.name,
        "status": terminal_now.status,
        "wait_status": wait_status,
        "busy": busy_now,
        "exited": exited,
        "completed": completed,
        "timed_out": timed_out,
        "finished_by": finished_by,
        "exit_code": exit_code,
        "timeout_ms": effective_timeout_ms,
        "waited_ms": waited_ms,
        "timeout_note": timeout_note,
        "output": output,
        "output_preview": output.chars().rev().take(1_000).collect::<Vec<_>>().into_iter().rev().collect::<String>(),
        "output_chars": output.chars().count(),
        "truncated": truncated
    }))
}

pub(super) async fn write_process_with_context(
    ctx: BoundContext,
    terminal_id: &str,
    data: &str,
    submit: bool,
) -> Result<Value, String> {
    let terminal = get_terminal_for_context(&ctx, terminal_id).await?;
    if terminal.status == "exited" {
        return Err(format!("terminal already exited: {}", terminal.id));
    }

    let input_chars = data.chars().count();
    if input_chars > PROCESS_WRITE_MAX_CHARS {
        return Err(format!(
            "input too large: {} chars exceeds {}",
            input_chars, PROCESS_WRITE_MAX_CHARS
        ));
    }

    let mut payload = data.to_string();
    if submit && !payload.ends_with('\n') && !payload.ends_with('\r') {
        payload.push('\n');
    }

    let manager = get_terminal_manager();
    let session = manager.ensure_running(&terminal).await?;
    session.write_input(payload.as_str())?;

    if !payload.is_empty() {
        let _ = TerminalLogService::create(TerminalLog::new(
            terminal.id.clone(),
            "input".to_string(),
            payload.clone(),
        ))
        .await;
    }
    let _ = TerminalService::touch(terminal.id.as_str()).await;

    Ok(json!({
        "terminal_id": terminal.id.clone(),
        "process_id": terminal.id.clone(),
        "terminal_name": terminal.name,
        "status": terminal.status,
        "operation_status": "ok",
        "submit": submit,
        "written_chars": payload.chars().count(),
        "bytes_written": payload.len(),
        "busy": session.is_busy()
    }))
}

pub(super) async fn kill_process_with_context(
    ctx: BoundContext,
    terminal_id: &str,
) -> Result<Value, String> {
    let terminal = get_terminal_for_context(&ctx, terminal_id).await?;
    let manager = get_terminal_manager();
    let busy_before = manager.get_busy(terminal_id).unwrap_or(false);
    let already_exited = terminal.status == "exited";

    if !already_exited {
        manager.close(terminal_id).await?;
        let _ = TerminalLogService::create(TerminalLog::new(
            terminal.id.clone(),
            "signal".to_string(),
            "terminate:process_kill".to_string(),
        ))
        .await;
    }

    let terminal_now = TerminalService::get_by_id(terminal_id)
        .await?
        .unwrap_or_else(|| terminal.clone());
    let busy_after = manager.get_busy(terminal_id).unwrap_or(false);

    Ok(json!({
        "terminal_id": terminal_now.id.clone(),
        "process_id": terminal_now.id.clone(),
        "terminal_name": terminal_now.name,
        "status": terminal_now.status,
        "operation_status": if already_exited { "already_exited" } else { "killed" },
        "already_exited": already_exited,
        "killed": !already_exited,
        "busy_before": busy_before,
        "busy_after": busy_after
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

async fn get_terminal_for_context(
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

fn append_output_tail(output: &mut String, chunk: &str, max_chars: usize, truncated: &mut bool) {
    if chunk.is_empty() {
        return;
    }
    output.push_str(chunk);
    let char_count = output.chars().count();
    if char_count <= max_chars {
        return;
    }
    *truncated = true;
    let tail: String = output
        .chars()
        .rev()
        .take(max_chars)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    *output = tail;
}

fn normalize_process_status(terminal_status: &str, busy: bool) -> &'static str {
    if terminal_status == "exited" {
        "exited"
    } else if busy {
        "running"
    } else {
        "idle"
    }
}
