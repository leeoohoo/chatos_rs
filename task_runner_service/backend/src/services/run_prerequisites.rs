use std::time::Duration;

use serde_json::json;
use tokio::time::Instant;
use tracing::warn;
use uuid::Uuid;

use crate::models::{
    now_rfc3339, StartTaskRunRequest, TaskRecord, TaskRunEventRecord, TaskRunRecord, TaskRunStatus,
    TaskStatus,
};

use super::prerequisite_context::{
    build_prerequisite_context, prerequisite_context_json, PrerequisiteTaskContext,
};
use super::status_display::TaskStatusExt;
use super::workspace_mcp::ensure_effective_task_workspace_dir;
use super::{is_terminal_run_status, normalized_optional, RunService, TaskService};

impl RunService {
    pub(super) async fn prepare_prerequisite_context(
        &self,
        task: &TaskRecord,
        run: &TaskRunRecord,
        input: &StartTaskRunRequest,
    ) -> Result<Vec<PrerequisiteTaskContext>, String> {
        let prerequisite_ids = self.resolve_prerequisite_order(task.id.as_str()).await?;
        if prerequisite_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.store
            .append_run_event(TaskRunEventRecord::new(
                run.id.clone(),
                "dependency_graph_resolved",
                Some(format!("解析到 {} 个前置任务", prerequisite_ids.len())),
                Some(json!({ "prerequisite_task_ids": prerequisite_ids.clone() })),
            ))
            .await?;

        let mut contexts = Vec::new();
        for prerequisite_task_id in prerequisite_ids {
            let prerequisite_task = self
                .store
                .get_task(&prerequisite_task_id)
                .await?
                .ok_or_else(|| format!("前置任务不存在: {prerequisite_task_id}"))?;
            let prerequisite_run = self
                .ensure_prerequisite_succeeded(&prerequisite_task, run, input)
                .await?;
            let prerequisite_task = self
                .store
                .get_task(&prerequisite_task_id)
                .await?
                .unwrap_or(prerequisite_task);
            contexts.push(build_prerequisite_context(
                &prerequisite_task,
                prerequisite_run.as_ref(),
            ));
        }
        self.store
            .append_run_event(TaskRunEventRecord::new(
                run.id.clone(),
                "dependency_context_attached",
                Some("前置任务结果已加入本次任务 prompt".to_string()),
                Some(prerequisite_context_json(&contexts)),
            ))
            .await?;
        Ok(contexts)
    }

    pub(super) async fn ensure_prerequisite_succeeded(
        &self,
        task: &TaskRecord,
        parent_run: &TaskRunRecord,
        input: &StartTaskRunRequest,
    ) -> Result<Option<TaskRunRecord>, String> {
        if matches!(task.status, TaskStatus::Archived) {
            return Err(format!("前置任务已归档，不能执行: {}", task.id));
        }
        if matches!(task.status, TaskStatus::Succeeded) {
            return Ok(self.latest_successful_run(task.id.as_str()).await?);
        }

        let active_run = self.active_run_for_task(task.id.as_str()).await?;
        let run = if let Some(active_run) = active_run {
            self.store
                .append_run_event(TaskRunEventRecord::new(
                    parent_run.id.clone(),
                    "dependency_waiting_active_run",
                    Some(format!("等待前置任务正在运行的 run: {}", task.title)),
                    Some(json!({
                        "task_id": task.id,
                        "run_id": active_run.id,
                    })),
                ))
                .await?;
            active_run
        } else {
            self.store
                .append_run_event(TaskRunEventRecord::new(
                    parent_run.id.clone(),
                    "dependency_run_started",
                    Some(format!("开始执行前置任务: {}", task.title)),
                    Some(json!({ "task_id": task.id })),
                ))
                .await?;
            self.queue_dependency_run(
                task.clone(),
                StartTaskRunRequest {
                    model_config_id: input.model_config_id.clone(),
                    prompt_override: None,
                },
            )
            .await?
        };

        let completed = self
            .wait_for_run_terminal(run.id.as_str(), parent_run.id.as_str())
            .await?;
        self.store
            .append_run_event(TaskRunEventRecord::new(
                parent_run.id.clone(),
                "dependency_run_finished",
                Some(format!(
                    "前置任务执行结束: {} / {}",
                    task.title,
                    completed.status.status_string()
                )),
                Some(json!({
                    "task_id": task.id,
                    "run_id": completed.id,
                    "status": completed.status.status_string(),
                    "result_summary": completed.result_summary,
                    "error_message": completed.error_message,
                })),
            ))
            .await?;
        if completed.status == TaskRunStatus::Succeeded {
            Ok(Some(completed))
        } else {
            Err(format!(
                "前置任务未成功完成: {} ({})",
                task.title,
                completed.status.status_string()
            ))
        }
    }

    pub(super) async fn wait_for_run_terminal(
        &self,
        run_id: &str,
        parent_run_id: &str,
    ) -> Result<TaskRunRecord, String> {
        let timeout = self.config.execution_timeout + Duration::from_secs(30);
        let started = Instant::now();
        loop {
            let run = self
                .store
                .get_run(run_id)
                .await?
                .ok_or_else(|| format!("运行不存在: {run_id}"))?;
            if is_terminal_run_status(run.status) {
                return Ok(run);
            }
            if self.store.is_cancel_requested(parent_run_id) {
                return Err("当前任务已请求取消，停止等待前置任务".to_string());
            }
            if started.elapsed() > timeout {
                return Err(format!("等待前置任务运行超时: {run_id}"));
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    pub(super) async fn active_run_for_task(
        &self,
        task_id: &str,
    ) -> Result<Option<TaskRunRecord>, String> {
        Ok(self
            .store
            .list_runs(Some(task_id))
            .await?
            .into_iter()
            .find(|run| matches!(run.status, TaskRunStatus::Queued | TaskRunStatus::Running)))
    }

    pub(super) async fn latest_successful_run(
        &self,
        task_id: &str,
    ) -> Result<Option<TaskRunRecord>, String> {
        Ok(self
            .store
            .list_runs(Some(task_id))
            .await?
            .into_iter()
            .find(|run| run.status == TaskRunStatus::Succeeded))
    }

    pub(super) async fn collect_succeeded_prerequisite_context(
        &self,
        task_id: &str,
    ) -> Result<Vec<PrerequisiteTaskContext>, String> {
        let prerequisite_ids = self.resolve_prerequisite_order(task_id).await?;
        let mut contexts = Vec::new();
        for prerequisite_task_id in prerequisite_ids {
            let task = self
                .store
                .get_task(&prerequisite_task_id)
                .await?
                .ok_or_else(|| format!("前置任务不存在: {prerequisite_task_id}"))?;
            if task.status != TaskStatus::Succeeded {
                return Err(format!("前置任务尚未成功完成: {}", task.title));
            }
            let run = self.latest_successful_run(task.id.as_str()).await?;
            contexts.push(build_prerequisite_context(&task, run.as_ref()));
        }
        Ok(contexts)
    }

    pub(super) async fn queue_dependency_run(
        &self,
        task: TaskRecord,
        input: StartTaskRunRequest,
    ) -> Result<TaskRunRecord, String> {
        if self.store.has_active_run_for_task(task.id.as_str()).await? {
            return self
                .active_run_for_task(task.id.as_str())
                .await?
                .ok_or_else(|| "前置任务已有运行中记录，但读取失败".to_string());
        }
        self.ensure_task_thread(&task).await?;

        let model_config_id = normalized_optional(input.model_config_id.clone())
            .or(task.default_model_config_id.clone())
            .ok_or_else(|| "前置任务未绑定模型配置，且本次执行也没有指定模型配置".to_string())?;
        let model_config = self
            .store
            .get_model_config(&model_config_id)
            .await?
            .ok_or_else(|| format!("模型配置不存在: {model_config_id}"))?;
        if !model_config.enabled {
            return Err(format!("模型配置已禁用: {model_config_id}"));
        }
        let effective_workspace_dir =
            ensure_effective_task_workspace_dir(&self.config, &task, &model_config)?;

        let run_id = Uuid::new_v4().to_string();
        let input_snapshot = json!({
            "task_id": task.id,
            "task_title": task.title,
            "objective": task.objective,
            "description": task.description,
            "input_payload": task.input_payload,
            "prompt_override": input.prompt_override,
            "model_config_id": model_config_id,
            "mcp_config": task.mcp_config,
            "started_as_prerequisite": true,
        });
        let now = now_rfc3339();
        let run = TaskRunRecord {
            id: run_id.clone(),
            task_id: task.id.clone(),
            model_config_id: model_config_id.clone(),
            memory_thread_id: task.memory_thread_id.clone(),
            status: TaskRunStatus::Queued,
            started_at: None,
            finished_at: None,
            input_snapshot,
            context_snapshot: None,
            result_summary: None,
            error_message: None,
            usage: None,
            report: None,
            cancel_requested: false,
            summary_job_run_id: None,
            created_at: now.clone(),
            updated_at: now,
        };
        self.store.save_run(run.clone()).await?;
        if let Ok(Some(mut task_record)) = self.store.get_task(&task.id).await {
            task_record.status = TaskStatus::Running;
            task_record.last_run_id = Some(run.id.clone());
            task_record.updated_at = now_rfc3339();
            if let Err(err) = self.store.save_task(task_record).await {
                warn!(
                    "failed to persist queued prerequisite task state for task {} and run {}: {}",
                    task.id, run.id, err
                );
            }
        }
        self.store
            .append_run_event(TaskRunEventRecord::new(
                run.id.clone(),
                "queued",
                Some("前置任务已进入队列".to_string()),
                None,
            ))
            .await?;

        let prerequisite_context = self
            .collect_succeeded_prerequisite_context(task.id.as_str())
            .await?;
        let service = self.clone();
        let run_for_spawn = run.clone();
        let input_for_spawn = input.clone();
        tokio::spawn(async move {
            service
                .execute_run_model_phase(
                    task,
                    model_config,
                    run_for_spawn,
                    input_for_spawn,
                    effective_workspace_dir,
                    prerequisite_context,
                )
                .await;
        });

        Ok(run)
    }

    pub(super) async fn resolve_prerequisite_order(
        &self,
        task_id: &str,
    ) -> Result<Vec<String>, String> {
        TaskService::new(self.config.clone(), self.store.clone())
            .resolve_prerequisite_order(task_id)
            .await
    }

    pub(super) async fn finish_blocked_by_prerequisite(
        &self,
        task: &TaskRecord,
        run: &mut TaskRunRecord,
        workspace_dir: &str,
        message: String,
    ) {
        run.status = TaskRunStatus::Blocked;
        run.finished_at = Some(now_rfc3339());
        run.updated_at = now_rfc3339();
        run.error_message = Some(message.clone());
        run.result_summary = Some(message.clone());
        run.cancel_requested = false;
        if let Err(err) = self.store.save_run(run.clone()).await {
            warn!("failed to persist blocked task run {}: {}", run.id, err);
        }
        if let Err(err) = self
            .store
            .append_run_event(TaskRunEventRecord::new(
                run.id.clone(),
                "dependency_failed",
                Some(message.clone()),
                None,
            ))
            .await
        {
            warn!(
                "failed to append dependency_failed event for run {}: {}",
                run.id, err
            );
        }
        if let Ok(Some(mut task_record)) = self.store.get_task(&task.id).await {
            task_record.status = TaskStatus::Blocked;
            task_record.result_summary = Some(message);
            task_record.last_run_id = Some(run.id.clone());
            task_record.updated_at = now_rfc3339();
            if let Err(err) = self.store.save_task(task_record).await {
                warn!("failed to persist blocked task {}: {}", task.id, err);
            }
        }
        self.try_send_terminal_callback(task.id.as_str(), run).await;
        self.cleanup_task_terminals(task, run, workspace_dir).await;
    }

    pub(super) async fn finish_failed_before_execution(
        &self,
        task: &TaskRecord,
        run: &mut TaskRunRecord,
        workspace_dir: &str,
        message: String,
    ) {
        run.status = TaskRunStatus::Failed;
        run.finished_at = Some(now_rfc3339());
        run.updated_at = now_rfc3339();
        run.error_message = Some(message.clone());
        run.result_summary = Some(message.clone());
        run.cancel_requested = false;
        if let Err(err) = self.store.save_run(run.clone()).await {
            warn!("failed to persist failed task run {}: {}", run.id, err);
        }
        if let Err(err) = self
            .store
            .append_run_event(TaskRunEventRecord::new(
                run.id.clone(),
                "failed",
                Some(message.clone()),
                None,
            ))
            .await
        {
            warn!("failed to append failed event for run {}: {}", run.id, err);
        }
        if let Ok(Some(mut task_record)) = self.store.get_task(&task.id).await {
            task_record.status = TaskStatus::Failed;
            task_record.result_summary = Some(message);
            task_record.last_run_id = Some(run.id.clone());
            task_record.updated_at = now_rfc3339();
            if let Err(err) = self.store.save_task(task_record).await {
                warn!("failed to persist failed task {}: {}", task.id, err);
            }
        }
        self.try_send_terminal_callback(task.id.as_str(), run).await;
        self.cleanup_task_terminals(task, run, workspace_dir).await;
    }
}
