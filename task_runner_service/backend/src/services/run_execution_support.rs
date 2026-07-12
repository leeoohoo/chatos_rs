// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_ai_runtime::{MemoryContextComposer, MemoryScope, TaskMcpInitMode, TaskRuntimeConfig};
use serde_json::{json, Value};
use tracing::{info, warn};

use crate::models::{
    now_rfc3339, TaskMcpConfig, TaskRecord, TaskRunEventRecord, TaskRunRecord, TaskRunStatus,
    TaskStatus,
};

use super::RunService;

impl RunService {
    pub(super) async fn compose_context_snapshot(
        &self,
        memory_scope: Option<&MemoryScope>,
    ) -> Option<Value> {
        let scope = memory_scope?;
        let client = self.config.memory_client().ok().flatten()?;
        let composer = MemoryContextComposer::from_client(client);
        match composer.compose(scope).await {
            Ok(response) => serde_json::to_value(response).ok(),
            Err(err) => {
                warn!("failed to compose context snapshot: {}", err);
                None
            }
        }
    }

    pub(super) async fn trigger_memory_summary(
        &self,
        task: &TaskRecord,
        run: &mut TaskRunRecord,
    ) -> Result<(), String> {
        let Some(client) = self.config.memory_client()? else {
            return Ok(());
        };
        let response = client
            .run_thread_repair_summary(&task.memory_thread_id, &task.tenant_id)
            .await?;
        info!(
            run_id = run.id.as_str(),
            task_id = task.id.as_str(),
            task_title = task.title.as_str(),
            memory_thread_id = task.memory_thread_id.as_str(),
            summary_job_run_id = response.job_run_id.as_deref().unwrap_or(""),
            "task runner triggered memory summary job"
        );
        run.summary_job_run_id = response.job_run_id.clone();
        run.updated_at = now_rfc3339();
        self.store.save_run(run.clone()).await?;
        self.store
            .append_run_event(TaskRunEventRecord::new(
                run.id.clone(),
                "memory_summary_requested",
                Some("已触发 Memory Engine repair summary".to_string()),
                Some(serde_json::to_value(response).unwrap_or_else(|_| json!({}))),
            ))
            .await?;
        Ok(())
    }

    pub(super) async fn finish_cancelled_before_start(
        &self,
        task: &TaskRecord,
        run: &mut TaskRunRecord,
        workspace_dir: &str,
    ) {
        run.status = TaskRunStatus::Cancelled;
        run.cancel_requested = false;
        run.finished_at = Some(now_rfc3339());
        run.updated_at = now_rfc3339();
        match self.store.save_run(run.clone()).await {
            Ok(saved) => {
                *run = saved;
            }
            Err(err) => {
                warn!(
                    "failed to persist pre-start cancelled run {}: {}",
                    run.id, err
                );
                return;
            }
        }
        if let Err(err) = self
            .store
            .append_run_event(TaskRunEventRecord::new(
                run.id.clone(),
                "cancelled",
                Some("任务在真正启动前已取消".to_string()),
                None,
            ))
            .await
        {
            warn!(
                "failed to append pre-start cancelled event for run {}: {}",
                run.id, err
            );
        }
        let mut task_already_cancelled = false;
        if let Ok(Some(mut task_record)) = self.store.get_task(&task.id).await {
            task_already_cancelled = task_record.status == TaskStatus::Cancelled;
            if !task_already_cancelled {
                task_record.status = TaskStatus::Cancelled;
                task_record.updated_at = now_rfc3339();
                if let Err(err) = self.store.save_task(task_record).await {
                    warn!("failed to persist cancelled task {}: {}", task.id, err);
                }
            }
        }
        if !task_already_cancelled {
            self.try_send_terminal_callback(task.id.as_str(), run).await;
        }
        self.cleanup_task_terminals(task, run, workspace_dir).await;
        self.store.clear_cancel_requested(&run.id);
    }

    pub(super) async fn repair_stale_cancel_requested_runs(&self) -> Result<(), String> {
        let runs = self.store.list_runs(None).await?;
        for mut run in runs.into_iter().filter(|run| {
            run.cancel_requested
                && !matches!(run.status, TaskRunStatus::Queued | TaskRunStatus::Running)
        }) {
            run.cancel_requested = false;
            run.updated_at = now_rfc3339();
            if let Err(err) = self.store.save_run(run.clone()).await {
                warn!(
                    "failed to repair stale cancel_requested flag for run {}: {}",
                    run.id, err
                );
            }
            self.store.clear_cancel_requested(&run.id);
        }
        Ok(())
    }

    pub(super) fn apply_task_mcp_config(
        &self,
        mut runtime_config: TaskRuntimeConfig,
        mcp_config: &TaskMcpConfig,
    ) -> TaskRuntimeConfig {
        runtime_config = runtime_config
            .with_builtin_prompt_locale(mcp_config.locale())
            .with_builtin_prompt_mode(mcp_config.builtin_prompt_mode);
        runtime_config.with_mcp_init_mode(effective_task_mcp_init_mode(mcp_config))
    }
}

fn effective_task_mcp_init_mode(mcp_config: &TaskMcpConfig) -> TaskMcpInitMode {
    if !mcp_config.enabled {
        return TaskMcpInitMode::Disabled;
    }
    TaskMcpInitMode::Full
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enabled_mcp_always_uses_full_runtime_mode() {
        let config = TaskMcpConfig {
            init_mode: TaskMcpInitMode::BuiltinOnly,
            external_mcp_config_ids: vec!["external-1".to_string()],
            ..TaskMcpConfig::default()
        };

        assert_eq!(effective_task_mcp_init_mode(&config), TaskMcpInitMode::Full);
    }

    #[test]
    fn builtin_only_without_external_mcp_is_normalized_to_full() {
        let config = TaskMcpConfig {
            init_mode: TaskMcpInitMode::BuiltinOnly,
            external_mcp_config_ids: Vec::new(),
            ..TaskMcpConfig::default()
        };

        assert_eq!(effective_task_mcp_init_mode(&config), TaskMcpInitMode::Full);
    }

    #[test]
    fn init_mode_disabled_is_ignored_when_mcp_is_enabled() {
        let config = TaskMcpConfig {
            enabled: true,
            init_mode: TaskMcpInitMode::Disabled,
            ..TaskMcpConfig::default()
        };

        assert_eq!(effective_task_mcp_init_mode(&config), TaskMcpInitMode::Full);
    }

    #[test]
    fn disabled_mcp_stays_disabled() {
        let config = TaskMcpConfig {
            enabled: false,
            init_mode: TaskMcpInitMode::BuiltinOnly,
            ..TaskMcpConfig::default()
        };

        assert_eq!(
            effective_task_mcp_init_mode(&config),
            TaskMcpInitMode::Disabled
        );
    }
}
