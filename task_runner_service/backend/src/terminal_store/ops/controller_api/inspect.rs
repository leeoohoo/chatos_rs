// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl TaskRunnerTerminalControllerStore {
    pub(super) async fn get_recent_logs_value(
        &self,
        context: TerminalControllerContext,
        per_terminal_limit: i64,
        terminal_limit: usize,
    ) -> Result<Value, String> {
        let sessions = sessions_for_context(&context).await?;
        let total = sessions.len();
        let mut terminals = Vec::new();
        for session in sessions.into_iter().take(terminal_limit) {
            refresh_session_status(&session).await?;
            let meta = session.meta.lock().await.clone();
            let logs = session.logs.lock().await;
            let recent = take_recent_logs(&logs, per_terminal_limit.max(1) as usize);
            terminals.push(json!({
                "terminal_id": meta.id,
                "terminal_name": derive_terminal_name(meta.cwd.as_str()),
                "status": meta.status,
                "cwd": meta.cwd,
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

    pub(super) async fn process_list_value(
        &self,
        context: TerminalControllerContext,
        include_exited: bool,
        limit: usize,
    ) -> Result<Value, String> {
        let sessions = sessions_for_context(&context).await?;
        let mut processes = Vec::new();
        for session in sessions {
            refresh_session_status(&session).await?;
            let meta = session.meta.lock().await.clone();
            if !include_exited && meta.status == "exited" {
                continue;
            }
            let output = collect_output(&session, 1200).await;
            processes.push(json!({
                "terminal_id": meta.id,
                "process_id": meta.id,
                "terminal_name": derive_terminal_name(meta.cwd.as_str()),
                "status": meta.status,
                "process_status": if meta.status == "exited" { "exited" } else { "running" },
                "busy": meta.status != "exited",
                "has_session": true,
                "command": meta.command,
                "pid": Value::Null,
                "started_at": meta.started_at,
                "uptime_seconds": Value::Null,
                "cwd": meta.cwd,
                "project_id": meta.project_id,
                "last_active_at": meta.last_active_at,
                "output_preview": output.text,
                "output_tail": output.text,
                "output_tail_chars": output.char_count,
                "exit_code": meta.exit_code,
            }));
            if processes.len() >= limit {
                break;
            }
        }
        Ok(json!({
            "status": "ok",
            "result_scope": if processes.len() > 1 { "multiple_terminals" } else if processes.is_empty() { "no_terminal" } else { "single_terminal" },
            "is_multiple_terminals": processes.len() > 1,
            "terminal_count": processes.len(),
            "process_count": processes.len(),
            "visible_total": processes.len(),
            "total_terminals": processes.len(),
            "include_exited": include_exited,
            "limit": limit,
            "terminals": processes.clone(),
            "processes": processes,
        }))
    }

    pub(super) async fn process_poll_value(
        &self,
        context: TerminalControllerContext,
        terminal_id: String,
        offset: Option<i64>,
        limit: i64,
    ) -> Result<Value, String> {
        let session = session_for_context(&context, terminal_id.as_str()).await?;
        refresh_session_status(&session).await?;
        let meta = session.meta.lock().await.clone();
        let logs = session.logs.lock().await;
        let effective_limit = limit.clamp(1, 200) as usize;
        let selected = select_logs(&logs, offset, effective_limit);
        let output = collect_output_from_logs(selected.iter().filter_map(log_value_content), 1200);
        Ok(json!({
            "terminal_id": meta.id,
            "process_id": meta.id,
            "terminal_name": derive_terminal_name(meta.cwd.as_str()),
            "status": meta.status,
            "process_status": if meta.status == "exited" { "exited" } else { "running" },
            "busy": meta.status != "exited",
            "has_session": true,
            "command": meta.command,
            "pid": Value::Null,
            "started_at": meta.started_at,
            "uptime_seconds": Value::Null,
            "cwd": meta.cwd,
            "project_id": meta.project_id,
            "last_active_at": meta.last_active_at,
            "mode": if offset.is_some() { "offset" } else { "recent" },
            "requested_offset": offset,
            "next_offset": selected.last().and_then(|value| value.get("offset")).and_then(Value::as_i64).map(|value| value + 1),
            "limit": effective_limit,
            "fetched_log_count": selected.len(),
            "returned_log_count": selected.len(),
            "has_more": offset.is_some() && logs.len() > selected.len(),
            "truncated": false,
            "truncation": { "truncated": false },
            "logs": selected,
            "output_preview": output.text,
            "output_tail": output.text,
            "output_tail_chars": output.char_count,
            "exit_code": meta.exit_code,
        }))
    }

    pub(super) async fn process_log_value(
        &self,
        context: TerminalControllerContext,
        terminal_id: String,
        offset: Option<i64>,
        limit: i64,
    ) -> Result<Value, String> {
        let poll = self
            .process_poll_value(context, terminal_id, offset, limit)
            .await?;
        let output = poll
            .get("logs")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(log_value_content)
                    .collect::<Vec<_>>()
                    .join("")
            })
            .unwrap_or_default();
        Ok(json!({
            "terminal_id": poll.get("terminal_id").cloned().unwrap_or(Value::Null),
            "status": poll.get("status").cloned().unwrap_or(Value::String("unknown".to_string())),
            "output": output,
            "offset": offset,
            "limit": limit,
            "has_more": poll.get("has_more").cloned().unwrap_or(Value::Bool(false)),
            "next_offset": poll.get("next_offset").cloned().unwrap_or(Value::Null),
        }))
    }

    pub(super) async fn process_wait_value(
        &self,
        context: TerminalControllerContext,
        terminal_id: String,
        timeout_ms: u64,
    ) -> Result<Value, String> {
        let session = session_for_context(&context, terminal_id.as_str()).await?;
        let result = wait_for_session(session.clone(), timeout_ms).await?;
        let output = collect_output(&session, context.max_output_chars).await;
        let meta = session.meta.lock().await.clone();
        Ok(json!({
            "terminal_id": meta.id,
            "process_id": meta.id,
            "terminal_name": derive_terminal_name(meta.cwd.as_str()),
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
}
