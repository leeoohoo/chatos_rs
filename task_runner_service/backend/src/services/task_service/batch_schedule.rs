// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl TaskService {
    pub async fn batch_update_status(
        &self,
        request: BatchTaskStatusUpdateRequest,
    ) -> Result<BatchTaskOperationResponse, String> {
        if request.status == TaskStatus::Cancelled {
            return Err("请使用 cancel_task 并提供取消原因".to_string());
        }
        let task_ids = normalize_batch_task_ids(request.task_ids)?;
        let mut results = Vec::with_capacity(task_ids.len());

        for task_id in task_ids {
            match self
                .update_task(
                    &task_id,
                    UpdateTaskRequest {
                        status: Some(request.status),
                        ..UpdateTaskRequest::default()
                    },
                    None,
                )
                .await
            {
                Ok(Some(_)) => results.push(BatchTaskOperationItem {
                    task_id,
                    ok: true,
                    message: None,
                    run_id: None,
                }),
                Ok(None) => results.push(BatchTaskOperationItem {
                    task_id,
                    ok: false,
                    message: Some("任务不存在".to_string()),
                    run_id: None,
                }),
                Err(err) => results.push(BatchTaskOperationItem {
                    task_id,
                    ok: false,
                    message: Some(err),
                    run_id: None,
                }),
            }
        }

        Ok(summarize_batch_results(results))
    }

    pub async fn batch_delete_tasks(
        &self,
        request: BatchTaskDeleteRequest,
    ) -> Result<BatchTaskOperationResponse, String> {
        let task_ids = normalize_batch_task_ids(request.task_ids)?;
        let mut results = Vec::with_capacity(task_ids.len());

        for task_id in task_ids {
            match self.delete_task(&task_id).await {
                Ok(true) => results.push(BatchTaskOperationItem {
                    task_id,
                    ok: true,
                    message: None,
                    run_id: None,
                }),
                Ok(false) => results.push(BatchTaskOperationItem {
                    task_id,
                    ok: false,
                    message: Some("任务不存在".to_string()),
                    run_id: None,
                }),
                Err(err) => results.push(BatchTaskOperationItem {
                    task_id,
                    ok: false,
                    message: Some(err),
                    run_id: None,
                }),
            }
        }

        Ok(summarize_batch_results(results))
    }

    pub async fn list_due_scheduled_tasks(
        &self,
        now: DateTime<Utc>,
    ) -> Result<Vec<TaskRecord>, String> {
        self.store.list_due_scheduled_tasks(now).await
    }

    pub async fn mark_scheduled_run_started(
        &self,
        id: &str,
        started_at: DateTime<Utc>,
    ) -> Result<Option<TaskRecord>, String> {
        let Some(task) = self.store.get_task(id).await? else {
            return Ok(None);
        };
        self.mark_scheduled_run_started_if_due(&task, started_at)
            .await
    }

    pub async fn mark_scheduled_run_started_if_due(
        &self,
        task: &TaskRecord,
        started_at: DateTime<Utc>,
    ) -> Result<Option<TaskRecord>, String> {
        let Some(expected_next_run_at) = task.schedule.next_run_at.as_deref() else {
            return Ok(None);
        };
        let next_schedule = advance_task_schedule_after_dispatch(&task.schedule, started_at)?;
        let updated_at = now_rfc3339();
        self.store
            .update_task_schedule_if_next_run_at(
                task.id.as_str(),
                expected_next_run_at,
                next_schedule,
                updated_at.as_str(),
            )
            .await
    }

    pub async fn mark_scheduled_run_failed(
        &self,
        id: &str,
        error: &str,
    ) -> Result<Option<TaskRecord>, String> {
        let Some(mut task) = self.store.get_task(id).await? else {
            return Ok(None);
        };
        task.result_summary = normalized_optional(Some(format!("scheduler error: {error}")));
        task.updated_at = now_rfc3339();
        Ok(Some(self.store.save_task(task).await?))
    }
}
