// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp::TerminalControllerContext;
use chatos_mcp_runtime::BuiltinMcpKind;
use tracing::{info, warn};

use crate::models::{TaskRecord, TaskRunEventRecord, TaskRunRecord};
use crate::terminal_store::TaskRunnerTerminalControllerStore;

use super::workspace_mcp::selected_builtin_kinds;
use super::RunService;

impl RunService {
    pub(super) fn request_task_terminal_cleanup(&self, task: &TaskRecord, run: &mut TaskRunRecord) {
        if task_terminal_enabled(task)
            && run.worker_id.is_some()
            && !run.terminal_cleanup_completed
            && !run.terminal_cleanup_event_enqueued
        {
            run.terminal_cleanup_event_pending = true;
        }
    }

    pub(super) async fn ensure_task_terminal_started(
        &self,
        task: &TaskRecord,
        run: &TaskRunRecord,
        workspace_dir: &str,
        authoritative_policy: bool,
    ) {
        if !task_terminal_enabled(task) {
            return;
        }
        match self
            .should_route_task_to_sandbox(task, authoritative_policy)
            .await
        {
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
        let context = match self.task_terminal_context(task, workspace_dir).await {
            Ok(context) => context,
            Err(err) => {
                warn!(
                    task_id = task.id.as_str(),
                    run_id = run.id.as_str(),
                    "task runner skipped local task terminal because tool result limits could not be loaded: {}",
                    err
                );
                return;
            }
        };
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

    pub(crate) async fn enqueue_terminal_cleanup_if_needed(
        &self,
        run: &TaskRunRecord,
    ) -> Result<bool, String> {
        if !run.terminal_cleanup_event_pending || run.terminal_cleanup_completed {
            return Ok(false);
        }
        let task = self
            .store
            .get_task(run.task_id.as_str())
            .await?
            .ok_or_else(|| format!("terminal cleanup task not found: {}", run.task_id))?;
        if !task_terminal_enabled(&task) {
            self.store
                .mark_terminal_cleanup_completed(run.id.as_str())
                .await?;
            return Ok(false);
        }
        let workspace_dir = run
            .input_snapshot
            .get("effective_workspace_dir")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                format!(
                    "Run {} is missing required effective_workspace_dir for terminal cleanup",
                    run.id
                )
            })?;
        crate::worker_control_queue::publish_terminal_cleanup_event(
            &self.task_queue_topology,
            run,
            task.id.as_str(),
            task.subject_id.as_str(),
            workspace_dir,
        )
        .await?;
        self.store
            .acknowledge_terminal_cleanup_event(run.id.as_str())
            .await?;
        Ok(true)
    }

    pub(crate) async fn publish_pending_terminal_cleanup_events(
        &self,
        limit: usize,
    ) -> Result<usize, String> {
        let pending = self.store.list_pending_terminal_cleanups(limit).await?;
        let mut published = 0usize;
        for run in pending {
            if self.enqueue_terminal_cleanup_if_needed(&run).await? {
                published += 1;
            }
        }
        Ok(published)
    }

    pub(crate) async fn process_terminal_cleanup_event(
        &self,
        run_id: &str,
        task_id: &str,
        subject_id: &str,
        workspace_dir: &str,
    ) -> Result<(), String> {
        let max_output_chars = self
            .effective_tool_result_model_budget_limits()
            .await?
            .per_result_max_chars;
        let context = TerminalControllerContext {
            root: workspace_dir.into(),
            user_id: Some(subject_id.to_string()),
            project_id: Some(task_id.to_string()),
            idle_timeout_ms: 5_000,
            max_wait_ms: 60_000,
            max_output_chars,
        };
        match TaskRunnerTerminalControllerStore
            .kill_sessions_for_context(context)
            .await
        {
            Ok(payload) => {
                info!(
                    task_id,
                    run_id, workspace_dir, "task runner cleaned up task terminals"
                );
                if let Err(err) = self
                    .store
                    .append_run_event(TaskRunEventRecord::new(
                        run_id.to_string(),
                        "terminal_cleanup",
                        Some("已关闭本次任务终端".to_string()),
                        Some(payload),
                    ))
                    .await
                {
                    warn!(
                        "failed to append terminal_cleanup event for run {}: {}",
                        run_id, err
                    );
                }
                self.store.mark_terminal_cleanup_completed(run_id).await?;
                Ok(())
            }
            Err(err) => {
                warn!(
                    task_id,
                    run_id, workspace_dir, "failed to clean up task terminals: {}", err
                );
                if let Err(event_err) = self
                    .store
                    .append_run_event(TaskRunEventRecord::new(
                        run_id.to_string(),
                        "terminal_cleanup_failed",
                        Some(format!("关闭任务终端失败: {err}")),
                        None,
                    ))
                    .await
                {
                    warn!(
                        "failed to append terminal_cleanup_failed event for run {}: {}",
                        run_id, event_err
                    );
                }
                Err(err)
            }
        }
    }

    pub(crate) async fn retry_terminal_cleanup(
        &self,
        run_id: &str,
        error: &str,
    ) -> Result<(), String> {
        self.store.retry_terminal_cleanup(run_id, error).await?;
        Ok(())
    }

    async fn task_terminal_context(
        &self,
        task: &TaskRecord,
        workspace_dir: &str,
    ) -> Result<TerminalControllerContext, String> {
        let max_output_chars = self
            .effective_tool_result_model_budget_limits()
            .await?
            .per_result_max_chars;
        Ok(TerminalControllerContext {
            root: workspace_dir.into(),
            user_id: Some(task.subject_id.clone()),
            project_id: Some(task.id.clone()),
            idle_timeout_ms: 5_000,
            max_wait_ms: 60_000,
            max_output_chars,
        })
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
