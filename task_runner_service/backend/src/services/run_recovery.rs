use serde_json::json;
use tracing::warn;

use crate::models::{now_rfc3339, RunListFilters, TaskRunEventRecord, TaskRunStatus, TaskStatus};

use super::RunService;

impl RunService {
    pub async fn recover_incomplete_runs(&self) -> Result<usize, String> {
        let mut active_runs = self
            .store
            .list_runs_filtered(&RunListFilters {
                status: Some(TaskRunStatus::Queued),
                ..RunListFilters::default()
            })
            .await?;
        active_runs.extend(
            self.store
                .list_runs_filtered(&RunListFilters {
                    status: Some(TaskRunStatus::Running),
                    ..RunListFilters::default()
                })
                .await?,
        );
        self.repair_stale_cancel_requested_runs().await?;

        if active_runs.is_empty() {
            self.store.refresh_runtime_guards().await?;
            return Ok(0);
        }

        let mut recovered_count = 0usize;
        for mut run in active_runs {
            let now = now_rfc3339();
            let previous_status = match run.status {
                TaskRunStatus::Queued => "queued",
                TaskRunStatus::Running => "running",
                TaskRunStatus::Succeeded => "succeeded",
                TaskRunStatus::Failed => "failed",
                TaskRunStatus::Cancelled => "cancelled",
                TaskRunStatus::Blocked => "blocked",
            };
            let was_cancel_requested =
                run.cancel_requested || self.store.fetch_cancel_requested(&run.id).await?;

            let (next_status, event_type, message, error_message, task_status) =
                if was_cancel_requested {
                    (
                        TaskRunStatus::Cancelled,
                        "recovered_cancelled_after_restart",
                        "任务在服务重启后按取消状态收尾".to_string(),
                        Some("run was cancelled while the service was restarting".to_string()),
                        TaskStatus::Cancelled,
                    )
                } else {
                    (
                        TaskRunStatus::Failed,
                        "recovered_failed_after_restart",
                        "任务运行因服务重启中断，已标记为失败".to_string(),
                        Some("run was interrupted by a task runner service restart".to_string()),
                        TaskStatus::Failed,
                    )
                };

            run.status = next_status;
            run.cancel_requested = false;
            run.finished_at = Some(now.clone());
            run.updated_at = now.clone();
            run.result_summary = Some(message.clone());
            run.error_message = error_message;

            if let Err(err) = self.store.save_run(run.clone()).await {
                warn!(
                    "failed to recover incomplete run {} during startup: {}",
                    run.id, err
                );
                continue;
            }

            if let Err(err) = self
                .store
                .append_run_event(TaskRunEventRecord::new(
                    run.id.clone(),
                    event_type.to_string(),
                    Some(message.clone()),
                    Some(json!({
                        "reason": "service_restart_recovery",
                        "previous_status": previous_status,
                        "recovered_status": match next_status {
                            TaskRunStatus::Queued => "queued",
                            TaskRunStatus::Running => "running",
                            TaskRunStatus::Succeeded => "succeeded",
                            TaskRunStatus::Failed => "failed",
                            TaskRunStatus::Cancelled => "cancelled",
                            TaskRunStatus::Blocked => "blocked",
                        },
                    })),
                ))
                .await
            {
                warn!(
                    "failed to append recovery event for run {}: {}",
                    run.id, err
                );
            }

            if let Ok(Some(mut task_record)) = self.store.get_task(&run.task_id).await {
                task_record.status = task_status;
                task_record.result_summary = Some(message.clone());
                task_record.last_run_id = Some(run.id.clone());
                task_record.updated_at = now.clone();
                if let Err(err) = self.store.save_task(task_record).await {
                    warn!(
                        "failed to persist recovered task {} for run {}: {}",
                        run.task_id, run.id, err
                    );
                }
            }

            self.store.clear_cancel_requested(&run.id);
            recovered_count += 1;
        }

        self.store.refresh_runtime_guards().await?;
        Ok(recovered_count)
    }
}
