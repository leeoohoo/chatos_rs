// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use tracing::{info, warn};

use crate::models::{TaskRunEventRecord, TaskRunRecord, TaskRunStatus};

use super::RunService;

impl RunService {
    pub async fn replay_run_post_process_dead_letter(
        &self,
        run_id: &str,
    ) -> Result<(TaskRunRecord, bool), String> {
        if !self
            .store
            .rearm_run_post_process_dead_letter(run_id)
            .await?
        {
            return Err(format!(
                "Run {run_id} is not an eligible dead-lettered post-process"
            ));
        }
        let run =
            self.store.get_run(run_id).await?.ok_or_else(|| {
                format!("Run not found after post-process replay rearm: {run_id}")
            })?;
        self.enqueue_run_post_process_if_needed(&run).await?;
        let replayed =
            self.store.get_run(run_id).await?.ok_or_else(|| {
                format!("Run not found after post-process replay publish: {run_id}")
            })?;
        let archived = match crate::run_post_process_queue::archive_run_post_process_dead_letter(
            &self.task_queue_topology,
            run_id,
            1_000,
        )
        .await
        {
            Ok(archived) => archived,
            Err(error) => {
                warn!(
                    run_id,
                    error = error.as_str(),
                    "Run post-process replay succeeded but old DLQ message archival failed"
                );
                false
            }
        };
        Ok((replayed, archived))
    }

    pub(crate) async fn enqueue_run_post_process_if_needed(
        &self,
        run: &TaskRunRecord,
    ) -> Result<bool, String> {
        if !run.post_process_event_pending || run.post_process_completed {
            return Ok(false);
        }
        crate::run_post_process_queue::enqueue_run_post_process(
            &self.task_queue_topology,
            run.id.as_str(),
        )
        .await?;
        self.store
            .acknowledge_run_post_process_event(run.id.as_str())
            .await?;
        Ok(true)
    }

    pub(crate) async fn publish_pending_run_post_processes(
        &self,
        limit: usize,
    ) -> Result<usize, String> {
        let pending = self.store.list_pending_run_post_processes(limit).await?;
        let mut published = 0usize;
        for run in pending {
            if self.enqueue_run_post_process_if_needed(&run).await? {
                published += 1;
            }
        }
        Ok(published)
    }

    pub(crate) async fn record_run_post_process_failure(
        &self,
        run_id: &str,
        error: &str,
    ) -> Result<u32, String> {
        self.store
            .record_run_post_process_failure(run_id, error)
            .await?;
        self.store
            .get_run(run_id)
            .await?
            .map(|run| run.post_process_attempt_count)
            .ok_or_else(|| format!("Run not found after post-process failure: {run_id}"))
    }

    pub(crate) async fn mark_run_post_process_dead_lettered(
        &self,
        run_id: &str,
        error: &str,
    ) -> Result<(), String> {
        let updated = self
            .store
            .mark_run_post_process_dead_lettered(run_id, error)
            .await?;
        if !updated {
            return Ok(());
        }
        if let Err(event_error) = self
            .store
            .append_run_event(TaskRunEventRecord::new(
                run_id.to_string(),
                "post_process_dead_lettered",
                Some(format!("Run 后处理达到最大重试次数并进入死信队列: {error}")),
                None,
            ))
            .await
        {
            warn!(
                run_id,
                error = event_error.as_str(),
                "failed to append Run post-process dead-letter event"
            );
        }
        Ok(())
    }

    pub(crate) async fn process_run_post_process(&self, run_id: &str) -> Result<(), String> {
        let Some(run) = self.store.get_run(run_id).await? else {
            return Ok(());
        };
        if run.post_process_completed || run.post_process_dead_lettered {
            return Ok(());
        }
        let task = self
            .store
            .get_task(run.task_id.as_str())
            .await?
            .ok_or_else(|| format!("Run post-process task not found: {}", run.task_id))?;
        self.finalize_mcp_management_run(&task, &run).await?;

        if run.status != TaskRunStatus::Succeeded {
            self.store.mark_run_post_process_completed(run_id).await?;
            return Ok(());
        }

        if !run.memory_summary_processed {
            let summary_job_run_id = if run.summary_job_run_id.is_some()
                || self.config.memory_engine_base_url.is_none()
                || !self.config.auto_memory_summary
            {
                run.summary_job_run_id.clone()
            } else {
                let client = self
                    .config
                    .memory_client()?
                    .ok_or_else(|| "Memory Engine client is not configured".to_string())?;
                let response = client
                    .run_thread_repair_summary(&run.memory_thread_id, &task.tenant_id)
                    .await?;
                info!(
                    run_id = run.id.as_str(),
                    task_id = task.id.as_str(),
                    memory_thread_id = run.memory_thread_id.as_str(),
                    summary_job_run_id = response.job_run_id.as_deref().unwrap_or(""),
                    "task runner post-processor triggered Memory Engine summary"
                );
                let event_payload = serde_json::to_value(&response).ok();
                if let Err(err) = self
                    .store
                    .append_run_event(TaskRunEventRecord::new(
                        run.id.clone(),
                        "memory_summary_requested",
                        Some("已触发 Memory Engine repair summary".to_string()),
                        event_payload,
                    ))
                    .await
                {
                    warn!(
                        run_id = run.id.as_str(),
                        error = err.as_str(),
                        "failed to append memory summary requested event"
                    );
                }
                response.job_run_id
            };
            self.store
                .mark_run_memory_summary_processed(run.id.as_str(), summary_job_run_id.as_deref())
                .await?;
        }

        if !run.chatos_followup_processed {
            let dispatched = self
                .dispatch_ready_chatos_async_tasks_for_source_task(&task)
                .await?;
            if !dispatched.is_empty() {
                info!(
                    task_id = task.id.as_str(),
                    run_id = run.id.as_str(),
                    dispatched_count = dispatched.len(),
                    "task runner post-processor dispatched ready Chatos follow-up tasks"
                );
            }
            self.store
                .mark_run_chatos_followup_processed(run.id.as_str())
                .await?;
        }

        self.store
            .mark_run_post_process_completed(run.id.as_str())
            .await?;
        Ok(())
    }

    pub(in crate::services) async fn enqueue_terminal_side_effects(&self, run: &TaskRunRecord) {
        if let Err(err) = self.enqueue_run_post_process_if_needed(run).await {
            warn!(
                run_id = run.id.as_str(),
                error = err.as_str(),
                "failed to enqueue Run post-processing; Outbox reconciliation will retry"
            );
        }
    }
}
