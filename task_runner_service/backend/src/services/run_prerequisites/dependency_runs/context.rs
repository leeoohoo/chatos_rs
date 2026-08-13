// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl RunService {
    pub(in crate::services) async fn prepare_prerequisite_context_event_driven(
        &self,
        task: &TaskRecord,
        run: &TaskRunRecord,
        input: &StartTaskRunRequest,
    ) -> Result<Option<Vec<PrerequisiteTaskContext>>, String> {
        let prerequisite_ids = self.resolve_prerequisite_order(task.id.as_str()).await?;
        if prerequisite_ids.is_empty() {
            return Ok(Some(Vec::new()));
        }
        let mut contexts = Vec::with_capacity(prerequisite_ids.len());
        for prerequisite_task_id in prerequisite_ids {
            let prerequisite_task = self
                .store
                .get_task(&prerequisite_task_id)
                .await?
                .ok_or_else(|| format!("前置任务不存在: {prerequisite_task_id}"))?;
            if prerequisite_task.status == TaskStatus::Archived {
                return Err(format!("前置任务已归档，不能执行: {prerequisite_task_id}"));
            }
            if prerequisite_task.status == TaskStatus::Succeeded {
                let latest = self
                    .latest_successful_run(prerequisite_task_id.as_str())
                    .await?;
                contexts.push(build_prerequisite_context(
                    &prerequisite_task,
                    latest.as_ref(),
                ));
                continue;
            }
            let dependency_run = match self
                .active_run_for_task(prerequisite_task_id.as_str())
                .await?
            {
                Some(active) => active,
                None => {
                    self.queue_dependency_run(
                        prerequisite_task.clone(),
                        StartTaskRunRequest {
                            model_config_id: input.model_config_id.clone(),
                            prompt_override: None,
                            retry_instruction: None,
                        },
                    )
                    .await?
                }
            };
            if dependency_run.status == TaskRunStatus::Succeeded {
                contexts.push(build_prerequisite_context(
                    &prerequisite_task,
                    Some(&dependency_run),
                ));
                continue;
            }
            if is_terminal_run_status(dependency_run.status) {
                return Err(format!(
                    "前置任务未成功完成: {} ({})",
                    prerequisite_task.title,
                    dependency_run.status.status_string()
                ));
            }
            self.store
                .subscribe_run_terminal(crate::store::RunTerminalSubscriptionRecord::cloud_agent(
                    dependency_run.id.as_str(),
                    run.id.as_str(),
                ))
                .await?;
            self.store
                .append_run_event(TaskRunEventRecord::new(
                    run.id.clone(),
                    "dependency_waiting_event",
                    Some(format!("等待前置任务完成: {}", prerequisite_task.title)),
                    Some(json!({
                        "task_id": prerequisite_task.id,
                        "run_id": dependency_run.id,
                    })),
                ))
                .await?;
            return Ok(None);
        }
        Ok(Some(contexts))
    }

    async fn resolve_prerequisite_order(&self, task_id: &str) -> Result<Vec<String>, String> {
        TaskService::new(self.config.clone(), self.store.clone())
            .resolve_prerequisite_order(task_id)
            .await
    }
}
