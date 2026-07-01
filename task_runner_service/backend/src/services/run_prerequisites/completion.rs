// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl RunService {
    pub(in crate::services) async fn finish_blocked_by_prerequisite(
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
        let mut task_already_cancelled = false;
        if let Ok(Some(mut task_record)) = self.store.get_task(&task.id).await {
            task_already_cancelled = task_record.status == TaskStatus::Cancelled;
            if !task_already_cancelled {
                task_record.status = TaskStatus::Blocked;
                task_record.result_summary = Some(message);
                task_record.last_run_id = Some(run.id.clone());
                task_record.updated_at = now_rfc3339();
                if let Err(err) = self.store.save_task(task_record).await {
                    warn!("failed to persist blocked task {}: {}", task.id, err);
                }
            }
        }
        if !task_already_cancelled {
            self.try_send_terminal_callback(task.id.as_str(), run).await;
        }
        self.cleanup_task_terminals(task, run, workspace_dir).await;
    }

    pub(in crate::services) async fn finish_failed_before_execution(
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
        let mut task_already_cancelled = false;
        if let Ok(Some(mut task_record)) = self.store.get_task(&task.id).await {
            task_already_cancelled = task_record.status == TaskStatus::Cancelled;
            if !task_already_cancelled {
                task_record.status = TaskStatus::Failed;
                task_record.result_summary = Some(message);
                task_record.last_run_id = Some(run.id.clone());
                task_record.updated_at = now_rfc3339();
                if let Err(err) = self.store.save_task(task_record).await {
                    warn!("failed to persist failed task {}: {}", task.id, err);
                }
            }
        }
        if !task_already_cancelled {
            self.try_send_terminal_callback(task.id.as_str(), run).await;
        }
        self.cleanup_task_terminals(task, run, workspace_dir).await;
    }
}
