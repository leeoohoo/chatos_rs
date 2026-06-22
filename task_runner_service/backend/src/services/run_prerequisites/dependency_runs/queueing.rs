use super::*;

impl RunService {
    pub(super) async fn queue_dependency_run(
        &self,
        task: TaskRecord,
        input: StartTaskRunRequest,
    ) -> Result<TaskRunRecord, String> {
        if task.status == TaskStatus::Cancelled {
            return Err(format!("前置任务已取消，不能执行: {}", task.id));
        }
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
            if task_record.status != TaskStatus::Cancelled {
                task_record.status = TaskStatus::Queued;
                task_record.last_run_id = Some(run.id.clone());
                task_record.updated_at = now_rfc3339();
                if let Err(err) = self.store.save_task(task_record).await {
                    warn!(
                        "failed to persist queued prerequisite task state for task {} and run {}: {}",
                        task.id, run.id, err
                    );
                }
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
}
