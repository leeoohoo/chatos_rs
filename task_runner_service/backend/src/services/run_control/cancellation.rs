// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::auth::CurrentUser;

impl RunService {
    pub async fn cancel_run(&self, run_id: &str) -> Result<Option<TaskRunRecord>, String> {
        let Some(current_run) = self.store.get_run(run_id).await? else {
            return Ok(None);
        };
        match current_run.status {
            TaskRunStatus::Queued | TaskRunStatus::Running => {}
            TaskRunStatus::Succeeded => {
                return Err("cannot cancel a succeeded run".to_string());
            }
            TaskRunStatus::Failed => {
                return Err("cannot cancel a failed run".to_string());
            }
            TaskRunStatus::Cancelled => {
                return Err("cannot cancel an already cancelled run".to_string());
            }
            TaskRunStatus::Blocked => {
                return Err("cannot cancel a blocked run".to_string());
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
                Some("run cancellation requested".to_string()),
                None,
            ))
            .await?;
        if matches!(run.status, TaskRunStatus::Queued) {
            run.status = TaskRunStatus::Cancelled;
            run.cancel_requested = true;
            run.finished_at = Some(now_rfc3339());
            run.updated_at = now_rfc3339();
            self.store.save_run(run.clone()).await?;
            self.store
                .append_run_event(TaskRunEventRecord::new(
                    run_id.to_string(),
                    "cancelled",
                    Some("run cancelled before execution started".to_string()),
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
        self.retry_run_with_user(run_id, None).await
    }

    pub async fn retry_run_for_user(
        &self,
        run_id: &str,
        current_user: &CurrentUser,
    ) -> Result<Option<TaskRunRecord>, String> {
        self.retry_run_with_user(run_id, Some(current_user)).await
    }

    async fn retry_run_with_user(
        &self,
        run_id: &str,
        current_user: Option<&CurrentUser>,
    ) -> Result<Option<TaskRunRecord>, String> {
        let Some(run) = self.store.get_run(run_id).await? else {
            return Ok(None);
        };
        if matches!(run.status, TaskRunStatus::Queued | TaskRunStatus::Running) {
            return Err("run is still active and cannot be retried yet".to_string());
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
        let restarted = if let Some(current_user) = current_user {
            self.start_run_for_user(&run.task_id, request, current_user)
                .await?
        } else {
            self.start_run(&run.task_id, request).await?
        };
        Ok(Some(restarted))
    }
}
