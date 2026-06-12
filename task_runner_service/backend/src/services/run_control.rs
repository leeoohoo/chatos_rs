use std::sync::Arc;

use memory_engine_sdk::SdkUpsertThreadRequest;
use serde_json::{json, Value};
use tokio::sync::Mutex as AsyncMutex;
use tracing::{info, warn};
use uuid::Uuid;

use crate::models::{
    now_rfc3339, ModelConfigRecord, StartTaskRunRequest, TaskRecord, TaskRunEventRecord,
    TaskRunRecord, TaskRunStatus, TaskScheduleMode, TaskStatus,
};

use super::workspace_mcp::ensure_effective_task_workspace_dir;
use super::{
    normalized_optional, RunService, RunTriggerSource, TaskScheduleModeExt, TaskStatusExt,
};

impl RunService {
    pub async fn start_run(
        &self,
        task_id: &str,
        input: StartTaskRunRequest,
    ) -> Result<TaskRunRecord, String> {
        self.start_run_with_trigger(task_id, input, RunTriggerSource::Manual)
            .await
    }

    pub async fn start_scheduled_run(
        &self,
        task_id: &str,
        input: StartTaskRunRequest,
    ) -> Result<TaskRunRecord, String> {
        self.start_run_with_trigger(task_id, input, RunTriggerSource::Scheduler)
            .await
    }

    pub(super) fn start_lock_for_task(&self, task_id: &str) -> Arc<AsyncMutex<()>> {
        let mut locks = self.start_locks.lock();
        locks
            .entry(task_id.to_string())
            .or_insert_with(|| Arc::new(AsyncMutex::new(())))
            .clone()
    }

    async fn start_run_with_trigger(
        &self,
        task_id: &str,
        input: StartTaskRunRequest,
        trigger: RunTriggerSource,
    ) -> Result<TaskRunRecord, String> {
        let start_lock = self.start_lock_for_task(task_id);
        let _guard = start_lock.lock().await;
        let task = self
            .store
            .get_task(task_id)
            .await?
            .ok_or_else(|| format!("任务不存在: {task_id}"))?;
        info!(
            task_id = task.id.as_str(),
            task_title = task.title.as_str(),
            task_status = task.status.status_string(),
            schedule_mode = task.schedule.mode.mode_key(),
            parent_task_id = task.parent_task_id.as_deref().unwrap_or(""),
            source_run_id = task.source_run_id.as_deref().unwrap_or(""),
            requested_model_config_id = input.model_config_id.as_deref().unwrap_or(""),
            has_prompt_override = input
                .prompt_override
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty()),
            "task runner received start_run request"
        );
        if matches!(task.schedule.mode, TaskScheduleMode::ContactAsync)
            && !matches!(trigger, RunTriggerSource::Scheduler)
        {
            return Err("联系人异步任务只能由后台调度器触发执行".to_string());
        }
        if self.store.has_active_run_for_task(task_id).await? {
            info!(
                task_id = task.id.as_str(),
                task_title = task.title.as_str(),
                "task runner rejected start_run because an active run already exists"
            );
            return Err("当前任务已有正在执行的运行".to_string());
        }
        self.ensure_task_thread(&task).await?;

        let model_config_id = normalized_optional(input.model_config_id.clone())
            .or(task.default_model_config_id.clone())
            .ok_or_else(|| "任务未绑定模型配置，且本次执行也没有指定模型配置".to_string())?;
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
        info!(
            run_id = run.id.as_str(),
            task_id = task.id.as_str(),
            task_title = task.title.as_str(),
            model_config_id = model_config_id.as_str(),
            workspace_dir = effective_workspace_dir.as_str(),
            schedule_mode = task.schedule.mode.mode_key(),
            parent_task_id = task.parent_task_id.as_deref().unwrap_or(""),
            source_run_id = task.source_run_id.as_deref().unwrap_or(""),
            "task runner queued run"
        );
        if let Ok(Some(mut task_record)) = self.store.get_task(task_id).await {
            task_record.status = TaskStatus::Running;
            task_record.last_run_id = Some(run.id.clone());
            task_record.updated_at = now_rfc3339();
            if let Err(err) = self.store.save_task(task_record).await {
                warn!(
                    "failed to persist queued task state for task {} and run {}: {}",
                    task_id, run.id, err
                );
            }
        }
        self.store
            .append_run_event(TaskRunEventRecord::new(
                run.id.clone(),
                "queued",
                Some("任务已进入队列".to_string()),
                None,
            ))
            .await?;

        let service = self.clone();
        let run_for_spawn = run.clone();
        let input_for_spawn = input.clone();
        let workspace_dir_for_spawn = effective_workspace_dir.clone();
        tokio::spawn(async move {
            service
                .execute_run(
                    task,
                    model_config,
                    run_for_spawn,
                    input_for_spawn,
                    workspace_dir_for_spawn,
                )
                .await;
        });

        Ok(run)
    }

    pub async fn cancel_run(&self, run_id: &str) -> Result<Option<TaskRunRecord>, String> {
        let Some(current_run) = self.store.get_run(run_id).await? else {
            return Ok(None);
        };
        match current_run.status {
            TaskRunStatus::Queued | TaskRunStatus::Running => {}
            TaskRunStatus::Succeeded => {
                return Err("当前运行状态不允许取消: succeeded".to_string());
            }
            TaskRunStatus::Failed => {
                return Err("当前运行状态不允许取消: failed".to_string());
            }
            TaskRunStatus::Cancelled => {
                return Err("当前运行状态不允许取消: cancelled".to_string());
            }
            TaskRunStatus::Blocked => {
                return Err("当前运行状态不允许取消: blocked".to_string());
            }
        }
        if current_run.cancel_requested {
            return Ok(Some(current_run));
        }

        let Some(mut run) = self.store.mark_cancel_requested(run_id).await? else {
            return Ok(None);
        };
        self.store
            .append_run_event(TaskRunEventRecord::new(
                run_id.to_string(),
                "cancel_requested",
                Some("已请求取消任务运行".to_string()),
                None,
            ))
            .await?;
        if matches!(run.status, TaskRunStatus::Queued) {
            run.status = TaskRunStatus::Cancelled;
            run.cancel_requested = false;
            run.finished_at = Some(now_rfc3339());
            run.updated_at = now_rfc3339();
            self.store.save_run(run.clone()).await?;
            self.store
                .append_run_event(TaskRunEventRecord::new(
                    run_id.to_string(),
                    "cancelled",
                    Some("任务在启动前已取消".to_string()),
                    None,
                ))
                .await?;
            if let Some(mut task_record) = self.store.get_task(&run.task_id).await? {
                task_record.status = TaskStatus::Cancelled;
                task_record.last_run_id = Some(run.id.clone());
                task_record.updated_at = now_rfc3339();
                self.store.save_task(task_record).await?;
            }
            self.try_send_terminal_callback(run.task_id.as_str(), &run)
                .await;
        }
        Ok(Some(run))
    }

    pub async fn retry_run(&self, run_id: &str) -> Result<Option<TaskRunRecord>, String> {
        let Some(run) = self.store.get_run(run_id).await? else {
            return Ok(None);
        };
        if matches!(run.status, TaskRunStatus::Queued | TaskRunStatus::Running) {
            return Err("运行仍在进行中，暂时不能重试".to_string());
        }

        let prompt_override = run
            .input_snapshot
            .get("prompt_override")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        let request = StartTaskRunRequest {
            model_config_id: Some(run.model_config_id.clone()),
            prompt_override,
        };
        self.start_run(&run.task_id, request).await.map(Some)
    }

    pub(super) async fn execute_run(
        &self,
        task: TaskRecord,
        model_config: ModelConfigRecord,
        mut run: TaskRunRecord,
        input: StartTaskRunRequest,
        effective_workspace_dir: String,
    ) {
        let prerequisite_context =
            match self.prepare_prerequisite_context(&task, &run, &input).await {
                Ok(context) => context,
                Err(err) => {
                    self.finish_blocked_by_prerequisite(
                        &task,
                        &mut run,
                        effective_workspace_dir.as_str(),
                        err,
                    )
                    .await;
                    return;
                }
            };
        self.execute_run_model_phase(
            task,
            model_config,
            run,
            input,
            effective_workspace_dir,
            prerequisite_context,
        )
        .await;
    }

    pub(super) async fn ensure_task_thread(&self, task: &TaskRecord) -> Result<(), String> {
        let Some(client) = self.config.memory_client()? else {
            return Ok(());
        };
        client
            .upsert_thread(
                &task.memory_thread_id,
                &SdkUpsertThreadRequest {
                    tenant_id: task.tenant_id.clone(),
                    subject_id: task.subject_id.clone(),
                    thread_type: "task".to_string(),
                    external_thread_id: Some(task.id.clone()),
                    title: Some(task.title.clone()),
                    labels: Some(vec![
                        "task_runner".to_string(),
                        format!("task_status:{}", task.status.status_string()),
                    ]),
                    metadata: Some(json!({
                        "task_id": task.id,
                        "service": "task_runner_service",
                    })),
                    status: Some("active".to_string()),
                    created_at: None,
                    updated_at: None,
                    archived_at: None,
                },
            )
            .await
            .map(|_| ())
    }
}
