// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::Path;

use chatos_builtin_tools::{
    terminal_process_list_entry, terminal_process_list_response, terminal_process_log_response,
    terminal_process_poll_response, TerminalControllerContext, TerminalProcessPollDetails,
    TerminalProcessSnapshot,
};
use serde_json::Value;

use super::super::super::output::{collect_local_mcp_output_from_logs, select_local_mcp_logs};
use super::super::super::registry::{
    collect_local_mcp_terminal_output, local_mcp_session_for_context,
    local_mcp_sessions_for_context, refresh_local_mcp_terminal_session_status,
};
use super::super::super::shell::{
    canonicalize_terminal_root, derive_local_mcp_terminal_name, display_local_mcp_workspace_path,
};
use super::super::super::{is_local_mcp_primary_shell_command, local_mcp_shell_session_is_busy};

pub(in crate::terminal::controller::store) async fn process_list(
    context: TerminalControllerContext,
    include_exited: bool,
    limit: usize,
) -> std::result::Result<Value, String> {
    let sessions = local_mcp_sessions_for_context(&context).await?;
    let project_root = canonicalize_terminal_root(context.root.as_path())?;
    let mut processes = Vec::new();
    for session in sessions {
        refresh_local_mcp_terminal_session_status(&session).await?;
        let meta = session.meta.lock().await.clone();
        if !include_exited && meta.status == "exited" {
            continue;
        }
        let busy = if is_local_mcp_primary_shell_command(meta.command.as_str()) {
            local_mcp_shell_session_is_busy(&session).await
        } else {
            meta.status != "exited"
        };
        let output = collect_local_mcp_terminal_output(&session, 1200).await;
        let cwd =
            display_local_mcp_workspace_path(project_root.as_path(), Path::new(meta.cwd.as_str()));
        let is_exited = meta.status == "exited";
        processes.push(terminal_process_list_entry(TerminalProcessSnapshot {
            terminal_id: meta.id,
            terminal_name: derive_local_mcp_terminal_name(cwd.as_str()),
            status: meta.status,
            process_status: if is_exited {
                "exited"
            } else if busy {
                "running"
            } else {
                "idle"
            }
            .to_string(),
            busy,
            command: meta.command,
            started_at: meta.started_at,
            cwd,
            project_id: meta.project_id,
            last_active_at: meta.last_active_at,
            output_preview: output.text,
            output_tail_chars: output.char_count,
            exit_code: meta.exit_code,
        }));
        if processes.len() >= limit {
            break;
        }
    }
    Ok(terminal_process_list_response(
        processes,
        include_exited,
        limit,
    ))
}

pub(in crate::terminal::controller::store) async fn process_poll(
    context: TerminalControllerContext,
    terminal_id: String,
    offset: Option<i64>,
    limit: i64,
) -> std::result::Result<Value, String> {
    let session = local_mcp_session_for_context(&context, terminal_id.as_str()).await?;
    refresh_local_mcp_terminal_session_status(&session).await?;
    let meta = session.meta.lock().await.clone();
    let busy = if is_local_mcp_primary_shell_command(meta.command.as_str()) {
        local_mcp_shell_session_is_busy(&session).await
    } else {
        meta.status != "exited"
    };
    let project_root = canonicalize_terminal_root(context.root.as_path())?;
    let cwd =
        display_local_mcp_workspace_path(project_root.as_path(), Path::new(meta.cwd.as_str()));
    let logs = session.logs.lock().await;
    let effective_limit = limit.clamp(1, 200) as usize;
    let selected = select_local_mcp_logs(&logs, offset, effective_limit);
    let output = collect_local_mcp_output_from_logs(
        selected
            .iter()
            .filter_map(|value| value.get("content").and_then(Value::as_str)),
        1200,
    );
    let is_exited = meta.status == "exited";
    Ok(terminal_process_poll_response(
        TerminalProcessSnapshot {
            terminal_id: meta.id,
            terminal_name: derive_local_mcp_terminal_name(cwd.as_str()),
            status: meta.status,
            process_status: if is_exited {
                "exited"
            } else if busy {
                "running"
            } else {
                "idle"
            }
            .to_string(),
            busy,
            command: meta.command,
            started_at: meta.started_at,
            cwd,
            project_id: meta.project_id,
            last_active_at: meta.last_active_at,
            output_preview: output.text,
            output_tail_chars: output.char_count,
            exit_code: meta.exit_code,
        },
        TerminalProcessPollDetails {
            offset,
            limit: effective_limit,
            has_more: offset.is_some() && logs.len() > selected.len(),
            logs: selected,
        },
    ))
}

pub(in crate::terminal::controller::store) async fn process_log(
    context: TerminalControllerContext,
    terminal_id: String,
    offset: Option<i64>,
    limit: i64,
) -> std::result::Result<Value, String> {
    let poll = process_poll(context, terminal_id, offset, limit).await?;
    Ok(terminal_process_log_response(&poll, offset, limit))
}
