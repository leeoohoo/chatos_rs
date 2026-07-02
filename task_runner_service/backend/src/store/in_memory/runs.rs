// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl InMemoryStore {
    pub(in crate::store) fn list_runs(&self, task_id: Option<&str>) -> Vec<TaskRunRecord> {
        let data = self.inner.read();
        let mut items = data
            .runs
            .values()
            .filter(|run| task_id.is_none_or(|value| run.task_id == value))
            .cloned()
            .collect::<Vec<_>>();
        items.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        items
    }

    pub(in crate::store) fn list_runs_filtered(
        &self,
        filters: &RunListFilters,
    ) -> Vec<TaskRunRecord> {
        let data = self.inner.read();
        let mut items = data
            .runs
            .values()
            .filter(|run| {
                filters
                    .task_id
                    .as_deref()
                    .is_none_or(|value| run.task_id == value)
            })
            .filter(|run| filters.status.is_none_or(|value| run.status == value))
            .filter(|run| {
                filters
                    .model_config_id
                    .as_deref()
                    .is_none_or(|value| run.model_config_id == value)
            })
            .filter(|run| {
                filters.keyword.as_deref().is_none_or(|value| {
                    run.id.to_ascii_lowercase().contains(value)
                        || run.task_id.to_ascii_lowercase().contains(value)
                        || run.model_config_id.to_ascii_lowercase().contains(value)
                        || run
                            .result_summary
                            .as_deref()
                            .unwrap_or_default()
                            .to_ascii_lowercase()
                            .contains(value)
                        || run
                            .error_message
                            .as_deref()
                            .unwrap_or_default()
                            .to_ascii_lowercase()
                            .contains(value)
                })
            })
            .cloned()
            .collect::<Vec<_>>();
        items.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        apply_offset_limit(&mut items, filters.offset, filters.limit);
        items
    }

    pub(in crate::store) fn list_runs_page(
        &self,
        filters: &RunListFilters,
    ) -> PaginatedResponse<TaskRunRecord> {
        let mut count_filters = filters.clone();
        count_filters.limit = None;
        count_filters.offset = None;
        let total = self.list_runs_filtered(&count_filters).len();
        build_page_response(
            self.list_runs_filtered(filters),
            total,
            filters.limit.unwrap_or(DEFAULT_PAGE_LIMIT),
            filters.offset.unwrap_or(0),
        )
    }

    pub(in crate::store) fn list_run_summaries_filtered(
        &self,
        filters: &RunListFilters,
    ) -> Vec<RunSummaryRecord> {
        self.list_runs_filtered(filters)
            .iter()
            .map(RunSummaryRecord::from)
            .collect()
    }

    pub(in crate::store) fn get_run_summaries_by_ids(
        &self,
        ids: &[String],
    ) -> Vec<RunSummaryRecord> {
        let wanted = ids.iter().collect::<std::collections::HashSet<_>>();
        let data = self.inner.read();
        let mut items = data
            .runs
            .values()
            .filter(|run| wanted.contains(&run.id))
            .map(RunSummaryRecord::from)
            .collect::<Vec<_>>();
        items.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        items
    }

    pub(in crate::store) fn get_run(&self, id: &str) -> Option<TaskRunRecord> {
        self.inner.read().runs.get(id).cloned()
    }

    pub(in crate::store) fn save_run(&self, run: TaskRunRecord) -> Result<TaskRunRecord, String> {
        let mut data = self.inner.write();
        let persisted = if let Some(claim_token) = run.claim_token.as_deref() {
            let Some(current) = data.runs.get(&run.id) else {
                return Err(lost_run_claim_error(&run.id));
            };
            if current.claim_token.as_deref() != Some(claim_token)
                || current.worker_id.as_deref() != run.worker_id.as_deref()
            {
                return Err(lost_run_claim_error(&run.id));
            }
            prepare_run_for_claim_guarded_persist(run)
        } else {
            run
        };
        data.runs.insert(persisted.id.clone(), persisted.clone());
        Ok(persisted)
    }

    pub(in crate::store) fn claim_next_queued_run(
        &self,
        worker_id: &str,
        claim_token: &str,
        claim_until: &str,
    ) -> Option<TaskRunRecord> {
        let mut data = self.inner.write();
        let run_id = data
            .runs
            .values()
            .filter(|run| run.status == TaskRunStatus::Queued)
            .min_by(|left, right| {
                left.created_at
                    .cmp(&right.created_at)
                    .then_with(|| left.id.cmp(&right.id))
            })
            .map(|run| run.id.clone())?;
        let run = data.runs.get_mut(&run_id)?;
        run.status = TaskRunStatus::Running;
        run.worker_id = Some(worker_id.to_string());
        run.claim_token = Some(claim_token.to_string());
        run.claim_until = Some(claim_until.to_string());
        run.attempt += 1;
        if run.started_at.is_none() {
            run.started_at = Some(now_rfc3339());
        }
        run.updated_at = now_rfc3339();
        Some(run.clone())
    }

    pub(in crate::store) fn renew_run_claim(
        &self,
        run_id: &str,
        worker_id: &str,
        claim_token: &str,
        claim_until: &str,
    ) -> bool {
        let mut data = self.inner.write();
        let Some(run) = data.runs.get_mut(run_id) else {
            return false;
        };
        if run.status != TaskRunStatus::Running
            || run.worker_id.as_deref() != Some(worker_id)
            || run.claim_token.as_deref() != Some(claim_token)
        {
            return false;
        }
        run.claim_until = Some(claim_until.to_string());
        run.updated_at = now_rfc3339();
        true
    }

    pub(in crate::store) fn fail_expired_run_claims(&self, now: &str) -> usize {
        let mut data = self.inner.write();
        let mut count = 0usize;
        for run in data.runs.values_mut() {
            if run.status != TaskRunStatus::Running {
                continue;
            }
            let expired = run
                .claim_until
                .as_deref()
                .is_some_and(|claim_until| claim_until <= now);
            if !expired {
                continue;
            }
            run.status = TaskRunStatus::Failed;
            run.finished_at = Some(now.to_string());
            run.updated_at = now.to_string();
            run.result_summary = Some("任务运行节点心跳过期，已标记为失败".to_string());
            run.error_message = Some("worker claim expired".to_string());
            run.cancel_requested = false;
            run.claim_token = None;
            run.claim_until = None;
            count += 1;
        }
        count
    }

    pub(in crate::store) fn list_run_events(&self, run_id: &str) -> Vec<TaskRunEventRecord> {
        self.inner
            .read()
            .run_events
            .get(run_id)
            .cloned()
            .unwrap_or_default()
    }

    pub(in crate::store) fn append_run_event(&self, event: TaskRunEventRecord) {
        let mut data = self.inner.write();
        data.run_events
            .entry(event.run_id.clone())
            .or_default()
            .push(event.clone());
        let _ = self.run_event_sender.send(event);
    }

    pub(in crate::store) fn mark_cancel_requested(&self, run_id: &str) -> Option<TaskRunRecord> {
        let mut data = self.inner.write();
        data.cancel_requested_runs.insert(run_id.to_string());
        let run = data.runs.get_mut(run_id)?;
        run.cancel_requested = true;
        Some(run.clone())
    }

    pub(in crate::store) fn clear_cancel_requested(&self, run_id: &str) {
        let mut data = self.inner.write();
        data.cancel_requested_runs.remove(run_id);
        if let Some(run) = data.runs.get_mut(run_id) {
            run.cancel_requested = false;
        }
    }

    pub(in crate::store) fn is_cancel_requested(&self, run_id: &str) -> bool {
        self.inner.read().cancel_requested_runs.contains(run_id)
    }

    pub(in crate::store) fn has_active_run_for_task(&self, task_id: &str) -> bool {
        self.inner.read().runs.values().any(|run| {
            run.task_id == task_id
                && matches!(run.status, TaskRunStatus::Queued | TaskRunStatus::Running)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> InMemoryStore {
        let (sender, _) = broadcast::channel(16);
        InMemoryStore::new(sender)
    }

    fn queued_run() -> TaskRunRecord {
        let now = now_rfc3339();
        TaskRunRecord {
            id: "run-1".to_string(),
            task_id: "task-1".to_string(),
            model_config_id: "model-1".to_string(),
            memory_thread_id: "thread-1".to_string(),
            status: TaskRunStatus::Queued,
            started_at: None,
            finished_at: None,
            input_snapshot: serde_json::json!({}),
            context_snapshot: None,
            result_summary: None,
            error_message: None,
            usage: None,
            report: None,
            cancel_requested: false,
            summary_job_run_id: None,
            worker_id: None,
            claim_token: None,
            claim_until: None,
            attempt: 0,
            created_at: now.clone(),
            updated_at: now,
        }
    }

    #[test]
    fn terminal_save_with_matching_claim_clears_claim_metadata() {
        let store = test_store();
        store.save_run(queued_run()).expect("save queued run");
        let mut claimed = store
            .claim_next_queued_run("worker-1", "claim-1", "2999-01-01T00:00:00Z")
            .expect("claim run");

        claimed.status = TaskRunStatus::Succeeded;
        claimed.finished_at = Some(now_rfc3339());
        claimed.updated_at = now_rfc3339();
        let saved = store.save_run(claimed).expect("save terminal run");

        assert_eq!(saved.status, TaskRunStatus::Succeeded);
        assert_eq!(saved.worker_id.as_deref(), Some("worker-1"));
        assert!(saved.claim_token.is_none());
        assert!(saved.claim_until.is_none());
        let persisted = store.get_run("run-1").expect("persisted run");
        assert!(persisted.claim_token.is_none());
        assert!(persisted.claim_until.is_none());
    }

    #[test]
    fn stale_worker_cannot_save_after_claim_expires() {
        let store = test_store();
        store.save_run(queued_run()).expect("save queued run");
        let mut stale = store
            .claim_next_queued_run("worker-1", "claim-1", "2000-01-01T00:00:00Z")
            .expect("claim run");

        assert_eq!(store.fail_expired_run_claims("2001-01-01T00:00:00Z"), 1);
        stale.status = TaskRunStatus::Succeeded;
        stale.finished_at = Some(now_rfc3339());
        stale.updated_at = now_rfc3339();

        let err = store.save_run(stale).expect_err("stale claim rejected");
        assert!(err.contains("run claim lost"));
        let persisted = store.get_run("run-1").expect("persisted run");
        assert_eq!(persisted.status, TaskRunStatus::Failed);
        assert_eq!(
            persisted.error_message.as_deref(),
            Some("worker claim expired")
        );
    }
}
