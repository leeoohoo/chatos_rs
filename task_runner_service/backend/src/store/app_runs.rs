// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl AppStore {
    pub async fn run_execution_stats(&self) -> Result<RunExecutionStats, String> {
        match self {
            Self::InMemory(store) => Ok(store.run_execution_stats()),
            Self::Mongo(store) => store.run_execution_stats().await,
        }
    }

    pub async fn list_runs(&self, task_id: Option<&str>) -> Result<Vec<TaskRunRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_runs(task_id)),
            Self::Mongo(store) => store.list_runs(task_id).await,
        }
    }

    pub async fn list_runs_filtered(
        &self,
        filters: &RunListFilters,
    ) -> Result<Vec<TaskRunRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_runs_filtered(filters)),
            Self::Mongo(store) => store.list_runs_filtered(filters).await,
        }
    }

    pub async fn list_runs_page(
        &self,
        filters: &RunListFilters,
    ) -> Result<PaginatedResponse<TaskRunRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_runs_page(filters)),
            Self::Mongo(store) => store.list_runs_page(filters).await,
        }
    }

    pub async fn list_run_summaries_filtered(
        &self,
        filters: &RunListFilters,
    ) -> Result<Vec<RunSummaryRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_run_summaries_filtered(filters)),
            Self::Mongo(store) => store.list_run_summaries_filtered(filters).await,
        }
    }

    pub async fn get_run_summaries_by_ids(
        &self,
        ids: &[String],
    ) -> Result<Vec<RunSummaryRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.get_run_summaries_by_ids(ids)),
            Self::Mongo(store) => store.get_run_summaries_by_ids(ids).await,
        }
    }

    pub async fn get_run(&self, id: &str) -> Result<Option<TaskRunRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.get_run(id)),
            Self::Mongo(store) => store.get_run(id).await,
        }
    }

    pub(crate) async fn subscribe_run_terminal(
        &self,
        subscription: RunTerminalSubscriptionRecord,
    ) -> Result<TaskRunRecord, String> {
        match self {
            Self::InMemory(store) => store.subscribe_run_terminal(subscription),
            Self::Mongo(store) => store.subscribe_run_terminal(subscription).await,
        }
    }

    pub(crate) async fn list_pending_run_terminal_subscriptions(
        &self,
        limit: usize,
    ) -> Result<Vec<(TaskRunRecord, RunTerminalSubscriptionRecord)>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_pending_run_terminal_subscriptions(limit)),
            Self::Mongo(store) => store.list_pending_run_terminal_subscriptions(limit).await,
        }
    }

    pub(crate) async fn acknowledge_run_terminal_subscription(
        &self,
        subscription_id: &str,
    ) -> Result<bool, String> {
        match self {
            Self::InMemory(store) => {
                Ok(store.acknowledge_run_terminal_subscription(subscription_id))
            }
            Self::Mongo(store) => {
                store
                    .acknowledge_run_terminal_subscription(subscription_id)
                    .await
            }
        }
    }

    pub async fn save_run(&self, run: TaskRunRecord) -> Result<TaskRunRecord, String> {
        match self {
            Self::InMemory(store) => store.save_run(run),
            Self::Mongo(store) => store.save_run(run).await,
        }
    }

    pub async fn claim_next_queued_run(
        &self,
        worker_id: &str,
        claim_token: &str,
        claim_until: &str,
    ) -> Result<Option<TaskRunRecord>, String> {
        match self {
            Self::InMemory(store) => {
                Ok(store.claim_next_queued_run(worker_id, claim_token, claim_until))
            }
            Self::Mongo(store) => {
                store
                    .claim_next_queued_run(worker_id, claim_token, claim_until)
                    .await
            }
        }
    }

    pub async fn has_queued_run_waiting_for_execution(&self) -> Result<bool, String> {
        match self {
            Self::InMemory(store) => Ok(store.has_queued_run_waiting_for_execution()),
            Self::Mongo(store) => store.has_queued_run_waiting_for_execution().await,
        }
    }

    pub async fn set_queued_runs_dispatch_paused(
        &self,
        task_ids: &[String],
        paused: bool,
    ) -> Result<u64, String> {
        match self {
            Self::InMemory(store) => {
                Ok(store.set_queued_runs_dispatch_paused(task_ids, paused) as u64)
            }
            Self::Mongo(store) => {
                store
                    .set_queued_runs_dispatch_paused(task_ids, paused)
                    .await
            }
        }
    }

    pub async fn list_pending_run_dispatches(
        &self,
        limit: usize,
    ) -> Result<Vec<TaskRunRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_pending_run_dispatches(limit)),
            Self::Mongo(store) => store.list_pending_run_dispatches(limit).await,
        }
    }

    pub async fn acknowledge_run_dispatch_event(&self, run_id: &str) -> Result<bool, String> {
        match self {
            Self::InMemory(store) => Ok(store.acknowledge_run_dispatch_event(run_id)),
            Self::Mongo(store) => store.acknowledge_run_dispatch_event(run_id).await,
        }
    }

    pub(crate) async fn list_pending_run_post_processes(
        &self,
        limit: usize,
    ) -> Result<Vec<TaskRunRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_pending_run_post_processes(limit)),
            Self::Mongo(store) => store.list_pending_run_post_processes(limit).await,
        }
    }

    pub(crate) async fn acknowledge_run_post_process_event(
        &self,
        run_id: &str,
    ) -> Result<bool, String> {
        match self {
            Self::InMemory(store) => Ok(store.acknowledge_run_post_process_event(run_id)),
            Self::Mongo(store) => store.acknowledge_run_post_process_event(run_id).await,
        }
    }

    pub(crate) async fn record_run_post_process_failure(
        &self,
        run_id: &str,
        error: &str,
    ) -> Result<bool, String> {
        match self {
            Self::InMemory(store) => Ok(store.record_run_post_process_failure(run_id, error)),
            Self::Mongo(store) => store.record_run_post_process_failure(run_id, error).await,
        }
    }

    pub(crate) async fn mark_run_memory_summary_processed(
        &self,
        run_id: &str,
        summary_job_run_id: Option<&str>,
    ) -> Result<bool, String> {
        match self {
            Self::InMemory(store) => {
                Ok(store.mark_run_memory_summary_processed(run_id, summary_job_run_id))
            }
            Self::Mongo(store) => {
                store
                    .mark_run_memory_summary_processed(run_id, summary_job_run_id)
                    .await
            }
        }
    }

    pub(crate) async fn mark_run_chatos_followup_processed(
        &self,
        run_id: &str,
    ) -> Result<bool, String> {
        match self {
            Self::InMemory(store) => Ok(store.mark_run_chatos_followup_processed(run_id)),
            Self::Mongo(store) => store.mark_run_chatos_followup_processed(run_id).await,
        }
    }

    pub(crate) async fn mark_run_post_process_completed(
        &self,
        run_id: &str,
    ) -> Result<bool, String> {
        match self {
            Self::InMemory(store) => Ok(store.mark_run_post_process_completed(run_id)),
            Self::Mongo(store) => store.mark_run_post_process_completed(run_id).await,
        }
    }

    pub(crate) async fn mark_run_post_process_dead_lettered(
        &self,
        run_id: &str,
        error: &str,
    ) -> Result<bool, String> {
        match self {
            Self::InMemory(store) => Ok(store.mark_run_post_process_dead_lettered(run_id, error)),
            Self::Mongo(store) => {
                store
                    .mark_run_post_process_dead_lettered(run_id, error)
                    .await
            }
        }
    }

    pub(crate) async fn rearm_run_post_process_dead_letter(
        &self,
        run_id: &str,
    ) -> Result<bool, String> {
        match self {
            Self::InMemory(store) => Ok(store.rearm_run_post_process_dead_letter(run_id)),
            Self::Mongo(store) => store.rearm_run_post_process_dead_letter(run_id).await,
        }
    }

    pub(crate) async fn list_pending_terminal_cleanups(
        &self,
        limit: usize,
    ) -> Result<Vec<TaskRunRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_pending_terminal_cleanups(limit)),
            Self::Mongo(store) => store.list_pending_terminal_cleanups(limit).await,
        }
    }

    pub(crate) async fn acknowledge_terminal_cleanup_event(
        &self,
        run_id: &str,
    ) -> Result<bool, String> {
        match self {
            Self::InMemory(store) => Ok(store.acknowledge_terminal_cleanup_event(run_id)),
            Self::Mongo(store) => store.acknowledge_terminal_cleanup_event(run_id).await,
        }
    }

    pub(crate) async fn retry_terminal_cleanup(
        &self,
        run_id: &str,
        error: &str,
    ) -> Result<bool, String> {
        match self {
            Self::InMemory(store) => Ok(store.retry_terminal_cleanup(run_id, error)),
            Self::Mongo(store) => store.retry_terminal_cleanup(run_id, error).await,
        }
    }

    pub(crate) async fn mark_terminal_cleanup_completed(
        &self,
        run_id: &str,
    ) -> Result<bool, String> {
        match self {
            Self::InMemory(store) => Ok(store.mark_terminal_cleanup_completed(run_id)),
            Self::Mongo(store) => store.mark_terminal_cleanup_completed(run_id).await,
        }
    }

    pub async fn renew_run_claim(
        &self,
        run_id: &str,
        worker_id: &str,
        claim_token: &str,
        claim_until: &str,
    ) -> Result<bool, String> {
        match self {
            Self::InMemory(store) => {
                Ok(store.renew_run_claim(run_id, worker_id, claim_token, claim_until))
            }
            Self::Mongo(store) => {
                store
                    .renew_run_claim(run_id, worker_id, claim_token, claim_until)
                    .await
            }
        }
    }

    pub async fn reconcile_expired_run_claims(
        &self,
        expired_before: &str,
        reconciled_at: &str,
        max_attempts: i64,
    ) -> Result<Vec<TaskRunRecord>, String> {
        match self {
            Self::InMemory(store) => {
                Ok(store.reconcile_expired_run_claims(expired_before, reconciled_at, max_attempts))
            }
            Self::Mongo(store) => {
                store
                    .reconcile_expired_run_claims(expired_before, reconciled_at, max_attempts)
                    .await
            }
        }
    }

    pub async fn list_pending_chatos_callback_runs(
        &self,
        now: &str,
        limit: usize,
    ) -> Result<Vec<TaskRunRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_pending_chatos_callback_runs(now, limit)),
            Self::Mongo(store) => store.list_pending_chatos_callback_runs(now, limit).await,
        }
    }

    pub async fn list_run_events(&self, run_id: &str) -> Result<Vec<TaskRunEventRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_run_events(run_id)),
            Self::Mongo(store) => store.list_run_events(run_id).await,
        }
    }

    pub async fn get_run_event(
        &self,
        run_id: &str,
        event_id: &str,
    ) -> Result<Option<TaskRunEventRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.get_run_event(run_id, event_id)),
            Self::Mongo(store) => store.get_run_event(run_id, event_id).await,
        }
    }

    pub async fn list_run_events_after(
        &self,
        run_id: &str,
        after_created_at: Option<&str>,
        after_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<TaskRunEventRecord>, String> {
        match self {
            Self::InMemory(store) => {
                Ok(store.list_run_events_after(run_id, after_created_at, after_id, limit))
            }
            Self::Mongo(store) => {
                store
                    .list_run_events_after(run_id, after_created_at, after_id, limit)
                    .await
            }
        }
    }

    pub async fn latest_run_event_cursor(
        &self,
        run_id: &str,
    ) -> Result<Option<(String, String)>, String> {
        match self {
            Self::InMemory(store) => Ok(store.latest_run_event_cursor(run_id)),
            Self::Mongo(store) => store.latest_run_event_cursor(run_id).await,
        }
    }

    pub async fn append_run_event(&self, event: TaskRunEventRecord) -> Result<(), String> {
        let publish_event = event.clone();
        match self {
            Self::InMemory(store) => {
                store.append_run_event(event);
                if let Err(err) = crate::run_event_queue::publish_run_event(&publish_event).await {
                    warn!(
                        run_id = publish_event.run_id.as_str(),
                        event_id = publish_event.id.as_str(),
                        event_type = publish_event.event_type.as_str(),
                        error = err.as_str(),
                        "failed to publish run event to rabbitmq"
                    );
                }
                Ok(())
            }
            Self::Mongo(store) => {
                store.append_run_event(event).await?;
                if let Err(err) = crate::run_event_queue::publish_run_event(&publish_event).await {
                    warn!(
                        run_id = publish_event.run_id.as_str(),
                        event_id = publish_event.id.as_str(),
                        event_type = publish_event.event_type.as_str(),
                        error = err.as_str(),
                        "failed to publish run event to rabbitmq"
                    );
                }
                Ok(())
            }
        }
    }

    pub fn append_run_event_sync(&self, event: TaskRunEventRecord) {
        let publish_event = event.clone();
        match self.clone() {
            Self::InMemory(store) => {
                store.append_run_event(event);
                tokio::spawn(async move {
                    if let Err(err) =
                        crate::run_event_queue::publish_run_event(&publish_event).await
                    {
                        warn!(
                            run_id = publish_event.run_id.as_str(),
                            event_id = publish_event.id.as_str(),
                            event_type = publish_event.event_type.as_str(),
                            error = err.as_str(),
                            "failed to publish run event to rabbitmq"
                        );
                    }
                });
            }
            Self::Mongo(store) => {
                tokio::spawn(async move {
                    if let Err(err) = store.append_run_event(event).await {
                        warn!("failed to append run event: {err}");
                        return;
                    }
                    if let Err(err) =
                        crate::run_event_queue::publish_run_event(&publish_event).await
                    {
                        warn!(
                            run_id = publish_event.run_id.as_str(),
                            event_id = publish_event.id.as_str(),
                            event_type = publish_event.event_type.as_str(),
                            error = err.as_str(),
                            "failed to publish run event to rabbitmq"
                        );
                    }
                });
            }
        }
    }

    pub async fn mark_cancel_requested(
        &self,
        run_id: &str,
    ) -> Result<Option<TaskRunRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.mark_cancel_requested(run_id)),
            Self::Mongo(store) => store.mark_cancel_requested(run_id).await,
        }
    }

    pub async fn repair_stale_cancel_requested_runs(&self) -> Result<u64, String> {
        match self {
            Self::InMemory(store) => Ok(store.repair_stale_cancel_requested_runs() as u64),
            Self::Mongo(store) => store.repair_stale_cancel_requested_runs().await,
        }
    }

    pub async fn list_pending_run_cancel_events(
        &self,
        limit: usize,
    ) -> Result<Vec<TaskRunRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_pending_run_cancel_events(limit)),
            Self::Mongo(store) => store.list_pending_run_cancel_events(limit).await,
        }
    }

    pub async fn acknowledge_run_cancel_event(&self, run_id: &str) -> Result<bool, String> {
        match self {
            Self::InMemory(store) => Ok(store.acknowledge_run_cancel_event(run_id)),
            Self::Mongo(store) => store.acknowledge_run_cancel_event(run_id).await,
        }
    }

    pub fn clear_cancel_requested(&self, run_id: &str) {
        match self.clone() {
            Self::InMemory(store) => store.clear_cancel_requested(run_id),
            Self::Mongo(store) => store.clear_cancel_requested(run_id),
        }
    }

    pub fn signal_local_run_abort(&self, run_id: &str) {
        match self {
            Self::InMemory(store) => store.signal_local_run_abort(run_id),
            Self::Mongo(store) => store.signal_local_run_abort(run_id),
        }
    }

    pub fn clear_local_run_abort(&self, run_id: &str) {
        match self {
            Self::InMemory(store) => store.clear_local_run_abort(run_id),
            Self::Mongo(store) => store.clear_local_run_abort(run_id),
        }
    }

    pub fn is_cancel_requested(&self, run_id: &str) -> bool {
        match self {
            Self::InMemory(store) => store.is_cancel_requested(run_id),
            Self::Mongo(store) => store.is_cancel_requested(run_id),
        }
    }

    pub async fn fetch_cancel_requested(&self, run_id: &str) -> Result<bool, String> {
        if self.is_cancel_requested(run_id) {
            return Ok(true);
        }
        Ok(self
            .get_run(run_id)
            .await?
            .is_some_and(|run| run.cancel_requested))
    }

    pub async fn refresh_runtime_guards(&self) -> Result<(), String> {
        match self {
            Self::InMemory(_) => Ok(()),
            Self::Mongo(store) => store.ensure_task_run_indexes().await,
        }
    }

    pub async fn has_active_run_for_task(&self, task_id: &str) -> Result<bool, String> {
        match self {
            Self::InMemory(store) => Ok(store.has_active_run_for_task(task_id)),
            Self::Mongo(store) => store.has_active_run_for_task(task_id).await,
        }
    }

    pub fn subscribe_run_events(&self) -> broadcast::Receiver<TaskRunEventRecord> {
        match self {
            Self::InMemory(store) => store.run_event_sender.subscribe(),
            Self::Mongo(store) => store.run_event_sender.subscribe(),
        }
    }

    pub fn broadcast_run_event(&self, event: TaskRunEventRecord) {
        let sender = match self {
            Self::InMemory(store) => &store.run_event_sender,
            Self::Mongo(store) => &store.run_event_sender,
        };
        let _ = sender.send(event);
    }
}
