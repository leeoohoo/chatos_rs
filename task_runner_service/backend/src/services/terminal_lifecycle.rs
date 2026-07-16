// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_builtin_tools::TerminalControllerContext;
use chatos_mcp_runtime::BuiltinMcpKind;
use tracing::{info, warn};

use crate::models::{TaskRecord, TaskRunEventRecord, TaskRunRecord};
use crate::terminal_store::TaskRunnerTerminalControllerStore;

use super::workspace_mcp::selected_builtin_kinds;
use super::RunService;

impl RunService {
    pub(super) async fn ensure_task_terminal_started(
        &self,
        task: &TaskRecord,
        run: &TaskRunRecord,
        workspace_dir: &str,
    ) {
        if !task_terminal_enabled(task) {
            return;
        }
        match self.should_route_task_to_sandbox(task).await {
            Ok(true) => {
                info!(
                    task_id = task.id.as_str(),
                    run_id = run.id.as_str(),
                    workspace_dir,
                    "task runner skipped local task terminal because sandbox routing is enabled"
                );
                return;
            }
            Ok(false) => {}
            Err(err) => {
                warn!(
                    task_id = task.id.as_str(),
                    run_id = run.id.as_str(),
                    workspace_dir,
                    "task runner skipped local task terminal because sandbox routing config could not be loaded: {}",
                    err
                );
                return;
            }
        }
        let context = self.task_terminal_context(task, workspace_dir);
        match TaskRunnerTerminalControllerStore
            .start_shell_session(context, ".".to_string())
            .await
        {
            Ok(payload) => {
                info!(
                    task_id = task.id.as_str(),
                    run_id = run.id.as_str(),
                    workspace_dir,
                    "task runner started initial task terminal"
                );
                if let Err(err) = self
                    .store
                    .append_run_event(TaskRunEventRecord::new(
                        run.id.clone(),
                        "terminal_started",
                        Some("已创建任务终端".to_string()),
                        Some(payload),
                    ))
                    .await
                {
                    warn!(
                        "failed to append terminal_started event for run {}: {}",
                        run.id, err
                    );
                }
            }
            Err(err) => {
                warn!(
                    task_id = task.id.as_str(),
                    run_id = run.id.as_str(),
                    workspace_dir,
                    "failed to start initial task terminal: {}",
                    err
                );
                if let Err(event_err) = self
                    .store
                    .append_run_event(TaskRunEventRecord::new(
                        run.id.clone(),
                        "terminal_start_failed",
                        Some(format!("创建任务终端失败: {err}")),
                        None,
                    ))
                    .await
                {
                    warn!(
                        "failed to append terminal_start_failed event for run {}: {}",
                        run.id, event_err
                    );
                }
            }
        }
    }

    pub(super) async fn cleanup_task_terminals(
        &self,
        task: &TaskRecord,
        run: &TaskRunRecord,
        workspace_dir: &str,
    ) {
        if !task_terminal_enabled(task) {
            return;
        }
        let context = self.task_terminal_context(task, workspace_dir);
        match TaskRunnerTerminalControllerStore
            .kill_sessions_for_context(context)
            .await
        {
            Ok(payload) => {
                info!(
                    task_id = task.id.as_str(),
                    run_id = run.id.as_str(),
                    workspace_dir,
                    "task runner cleaned up task terminals"
                );
                if let Err(err) = self
                    .store
                    .append_run_event(TaskRunEventRecord::new(
                        run.id.clone(),
                        "terminal_cleanup",
                        Some("已关闭本次任务终端".to_string()),
                        Some(payload),
                    ))
                    .await
                {
                    warn!(
                        "failed to append terminal_cleanup event for run {}: {}",
                        run.id, err
                    );
                }
            }
            Err(err) => {
                warn!(
                    task_id = task.id.as_str(),
                    run_id = run.id.as_str(),
                    workspace_dir,
                    "failed to clean up task terminals: {}",
                    err
                );
                if let Err(event_err) = self
                    .store
                    .append_run_event(TaskRunEventRecord::new(
                        run.id.clone(),
                        "terminal_cleanup_failed",
                        Some(format!("关闭任务终端失败: {err}")),
                        None,
                    ))
                    .await
                {
                    warn!(
                        "failed to append terminal_cleanup_failed event for run {}: {}",
                        run.id, event_err
                    );
                }
            }
        }
    }

    fn task_terminal_context(
        &self,
        task: &TaskRecord,
        workspace_dir: &str,
    ) -> TerminalControllerContext {
        TerminalControllerContext {
            root: workspace_dir.into(),
            user_id: Some(task.subject_id.clone()),
            project_id: Some(task.id.clone()),
            idle_timeout_ms: 5_000,
            max_wait_ms: 60_000,
            max_output_chars: 20_000,
        }
    }
}

fn task_terminal_enabled(task: &TaskRecord) -> bool {
    if !task.mcp_config.enabled {
        return false;
    }
    selected_builtin_kinds(&task.mcp_config)
        .into_iter()
        .any(|kind| kind == BuiltinMcpKind::TerminalController)
}
