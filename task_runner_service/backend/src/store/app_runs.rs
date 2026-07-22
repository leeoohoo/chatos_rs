// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl AppStore {
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

    pub async fn fail_expired_run_claims(&self, now: &str) -> Result<Vec<TaskRunRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.fail_expired_run_claims(now)),
            Self::Mongo(store) => store.fail_expired_run_claims(now).await,
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

    pub async fn append_run_event(&self, event: TaskRunEventRecord) -> Result<(), String> {
        match self {
            Self::InMemory(store) => {
                store.append_run_event(event);
                Ok(())
            }
            Self::Mongo(store) => store.append_run_event(event).await,
        }
    }

    pub fn append_run_event_sync(&self, event: TaskRunEventRecord) {
        match self.clone() {
            Self::InMemory(store) => store.append_run_event(event),
            Self::Mongo(store) => {
                tokio::spawn(async move {
                    if let Err(err) = store.append_run_event(event).await {
                        warn!("failed to append run event: {err}");
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

    pub fn clear_cancel_requested(&self, run_id: &str) {
        match self.clone() {
            Self::InMemory(store) => store.clear_cancel_requested(run_id),
            Self::Mongo(store) => store.clear_cancel_requested(run_id),
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
}
