use super::*;

impl RunService {
    pub(super) async fn finalize_model_phase(
        &self,
        task: &TaskRecord,
        run: &mut TaskRunRecord,
        report: TaskRunReport,
        effective_workspace_dir: &str,
    ) {
        let report_json = serde_json::to_value(&report).ok();
        let result_summary = summarized_report_content(&report.content);
        run.updated_at = now_rfc3339();
        run.finished_at = Some(report.completed_at.clone());
        run.result_summary = result_summary.clone();
        run.error_message = report.error.clone();
        run.usage = report.usage.clone();
        run.report = report_json.clone();
        run.cancel_requested = false;
        run.status = match report.status {
            chatos_ai_runtime::AiTurnStatus::Completed => TaskRunStatus::Succeeded,
            chatos_ai_runtime::AiTurnStatus::Failed => TaskRunStatus::Failed,
            chatos_ai_runtime::AiTurnStatus::Aborted => TaskRunStatus::Cancelled,
        };
        if let Err(err) = self.store.save_run(run.clone()).await {
            warn!("failed to persist completed task run {}: {}", run.id, err);
        }

        let event_type = match run.status {
            TaskRunStatus::Succeeded => "completed",
            TaskRunStatus::Failed => "failed",
            TaskRunStatus::Cancelled => "cancelled",
            TaskRunStatus::Blocked => "blocked",
            TaskRunStatus::Queued | TaskRunStatus::Running => "finished",
        };
        if let Err(err) = self
            .store
            .append_run_event(TaskRunEventRecord::new(
                run.id.clone(),
                event_type,
                Some(report.user_message()),
                report_json.clone(),
            ))
            .await
        {
            warn!(
                "failed to append completion event for run {}: {}",
                run.id, err
            );
        }

        let mut task_already_cancelled = false;
        if let Ok(Some(mut task_record)) = self.store.get_task(&task.id).await {
            task_already_cancelled = task_record.status == TaskStatus::Cancelled;
            if !task_already_cancelled {
                task_record.status = match run.status {
                    TaskRunStatus::Succeeded => TaskStatus::Succeeded,
                    TaskRunStatus::Failed => TaskStatus::Failed,
                    TaskRunStatus::Cancelled => TaskStatus::Cancelled,
                    TaskRunStatus::Blocked => TaskStatus::Blocked,
                    TaskRunStatus::Queued | TaskRunStatus::Running => TaskStatus::Running,
                };
                task_record.result_summary = result_summary;
                task_record.last_run_id = Some(run.id.clone());
                task_record.updated_at = now_rfc3339();
                if let Err(err) = self.store.save_task(task_record).await {
                    warn!("failed to persist completed task {}: {}", task.id, err);
                }
            }
        }
        if !task_already_cancelled {
            self.try_send_terminal_callback(task.id.as_str(), run).await;
        }
        self.cleanup_task_terminals(task, run, effective_workspace_dir)
            .await;
        self.maybe_trigger_auto_memory_summary(task, run).await;
        self.store.clear_cancel_requested(&run.id);
    }

    async fn maybe_trigger_auto_memory_summary(&self, task: &TaskRecord, run: &mut TaskRunRecord) {
        if matches!(run.status, TaskRunStatus::Succeeded)
            && self.config.memory_engine_base_url.is_some()
            && self.config.auto_memory_summary
        {
            if let Err(err) = self.trigger_memory_summary(task, run).await {
                if let Err(event_err) = self
                    .store
                    .append_run_event(TaskRunEventRecord::new(
                        run.id.clone(),
                        "memory_summary_error",
                        Some(format!("触发 Memory Engine 总结失败: {err}")),
                        None,
                    ))
                    .await
                {
                    warn!(
                        "failed to append memory summary error event for run {}: {}",
                        run.id, event_err
                    );
                }
                warn!(
                    "failed to trigger memory summary for run {}: {}",
                    run.id, err
                );
            }
        } else if matches!(run.status, TaskRunStatus::Succeeded)
            && self.config.memory_engine_base_url.is_some()
            && !self.config.auto_memory_summary
        {
            info!(
                run_id = run.id.as_str(),
                task_id = task.id.as_str(),
                task_title = task.title.as_str(),
                memory_thread_id = task.memory_thread_id.as_str(),
                "task runner skipped automatic memory summary because TASK_RUNNER_AUTO_MEMORY_SUMMARY is disabled"
            );
        }
    }
}
