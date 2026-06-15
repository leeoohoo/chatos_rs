use super::*;

impl RunService {
    pub(in crate::services) async fn prepare_prerequisite_context(
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

    async fn ensure_prerequisite_succeeded(
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

    async fn resolve_prerequisite_order(&self, task_id: &str) -> Result<Vec<String>, String> {
        TaskService::new(self.config.clone(), self.store.clone())
            .resolve_prerequisite_order(task_id)
            .await
    }
}
