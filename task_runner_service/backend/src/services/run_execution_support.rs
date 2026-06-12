use chatos_ai_runtime::{MemoryContextComposer, MemoryScope, TaskRuntimeConfig};
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
        let Some(base_url) = self.config.memory_engine_base_url.clone() else {
            return None;
        };
        let composer = MemoryContextComposer::new_direct(
            base_url,
            self.config.memory_timeout,
            self.config.memory_engine_source_id.clone(),
        )
        .ok()?;
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
        if let Err(err) = self.store.save_run(run.clone()).await {
            warn!(
                "failed to persist pre-start cancelled run {}: {}",
                run.id, err
            );
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
        if let Ok(Some(mut task_record)) = self.store.get_task(&task.id).await {
            task_record.status = TaskStatus::Cancelled;
            task_record.updated_at = now_rfc3339();
            if let Err(err) = self.store.save_task(task_record).await {
                warn!("failed to persist cancelled task {}: {}", task.id, err);
            }
        }
        self.try_send_terminal_callback(task.id.as_str(), run).await;
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
        if !mcp_config.enabled {
            runtime_config.with_mcp_init_mode(chatos_ai_runtime::TaskMcpInitMode::Disabled)
        } else {
            runtime_config.with_mcp_init_mode(mcp_config.init_mode)
        }
    }
}
