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
        let project_root = canonicalize_existing(context.root.as_path())?;
        let total = sessions.len();
        let mut terminals = Vec::new();
        for session in sessions.into_iter().take(terminal_limit) {
            refresh_session_status(&session).await?;
            let meta = session.meta.lock().await.clone();
            let display_cwd =
                display_workspace_path(project_root.as_path(), Path::new(meta.cwd.as_str()));
            let logs = session.logs.lock().await;
            let recent = take_recent_logs(&logs, per_terminal_limit.max(1) as usize);
            terminals.push(terminal_recent_logs_entry(TerminalRecentLogsEntry {
                terminal_id: meta.id,
                terminal_name: derive_terminal_name(display_cwd.as_str()),
                status: meta.status,
                cwd: display_cwd,
                project_id: meta.project_id,
                last_active_at: meta.last_active_at,
                log_count: logs.len(),
                logs: recent,
            }));
        }
        Ok(terminal_recent_logs_response(
            terminals,
            total,
            per_terminal_limit,
            terminal_limit,
        ))
    }

    pub(super) async fn process_list_value(
        &self,
        context: TerminalControllerContext,
        include_exited: bool,
        limit: usize,
    ) -> Result<Value, String> {
        let sessions = sessions_for_context(&context).await?;
        let project_root = canonicalize_existing(context.root.as_path())?;
        let mut processes = Vec::new();
        for session in sessions {
            refresh_session_status(&session).await?;
            let meta = session.meta.lock().await.clone();
            if !include_exited && meta.status == "exited" {
                continue;
            }
            let output = collect_output(&session, 1200).await;
            let display_cwd =
                display_workspace_path(project_root.as_path(), Path::new(meta.cwd.as_str()));
            let is_exited = meta.status == "exited";
            processes.push(terminal_process_list_entry(TerminalProcessSnapshot {
                terminal_id: meta.id,
                terminal_name: derive_terminal_name(display_cwd.as_str()),
                status: meta.status,
                process_status: if is_exited { "exited" } else { "running" }.to_string(),
                busy: !is_exited,
                command: meta.command,
                started_at: meta.started_at,
                cwd: display_cwd,
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
        let project_root = canonicalize_existing(context.root.as_path())?;
        let display_cwd =
            display_workspace_path(project_root.as_path(), Path::new(meta.cwd.as_str()));
        let logs = session.logs.lock().await;
        let effective_limit = limit.clamp(1, 200) as usize;
        let selected = select_logs(&logs, offset, effective_limit);
        let output = collect_output_from_logs(selected.iter().filter_map(log_value_content), 1200);
        let is_exited = meta.status == "exited";
        Ok(terminal_process_poll_response(
            TerminalProcessSnapshot {
                terminal_id: meta.id,
                terminal_name: derive_terminal_name(display_cwd.as_str()),
                status: meta.status,
                process_status: if is_exited { "exited" } else { "running" }.to_string(),
                busy: !is_exited,
                command: meta.command,
                started_at: meta.started_at,
                cwd: display_cwd,
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
        Ok(terminal_process_log_response(&poll, offset, limit))
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
        let project_root = canonicalize_existing(context.root.as_path())?;
        let display_cwd =
            display_workspace_path(project_root.as_path(), Path::new(meta.cwd.as_str()));
        let is_exited = meta.status == "exited";
        Ok(terminal_process_wait_response(
            TerminalProcessWaitResponse {
                terminal_id: meta.id,
                terminal_name: derive_terminal_name(display_cwd.as_str()),
                status: meta.status,
                wait_status: if result.timed_out {
                    "timeout"
                } else if is_exited {
                    "exited"
                } else {
                    "running"
                }
                .to_string(),
                busy: result.busy,
                exited: is_exited,
                completed: !result.timed_out,
                timed_out: result.timed_out,
                finished_by: result.finished_by.to_string(),
                exit_code: result.exit_code,
                timeout_ms,
                waited_ms: result.waited_ms,
                output: output.text,
                output_chars: output.char_count,
                truncated: output.truncated,
            },
        ))
    }
}
