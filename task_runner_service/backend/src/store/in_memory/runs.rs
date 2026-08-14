// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[path = "runs/events.rs"]
mod events;
#[cfg(test)]
#[path = "runs/tests.rs"]
mod tests;

impl InMemoryStore {
    pub(in crate::store) fn run_execution_stats(&self) -> RunExecutionStats {
        let data = self.inner.read();
        let mut stats = RunExecutionStats::default();
        for run in data.runs.values() {
            stats.total = stats.total.saturating_add(1);
            stats.dispatch_paused += usize::from(run.dispatch_paused);
            stats.dispatch_outbox_pending += usize::from(run.dispatch_event_pending);
            stats.cancellation_outbox_pending += usize::from(run.cancel_event_pending);
            stats.post_process_outbox_pending += usize::from(run.post_process_event_pending);
            if let Some(callback) = run.chatos_callback_delivery.as_ref() {
                match callback.status {
                    ChatosCallbackDeliveryStatus::Pending => {
                        stats.callback_pending = stats.callback_pending.saturating_add(1);
                    }
                    ChatosCallbackDeliveryStatus::Enqueued => {
                        stats.callback_enqueued = stats.callback_enqueued.saturating_add(1);
                    }
                    ChatosCallbackDeliveryStatus::Delivered
                    | ChatosCallbackDeliveryStatus::Skipped => {}
                }
            }
            match run.status {
                TaskRunStatus::Queued => {
                    stats.queued = stats.queued.saturating_add(1);
                    stats.active = stats.active.saturating_add(1);
                }
                TaskRunStatus::Running => {
                    stats.running = stats.running.saturating_add(1);
                    stats.active = stats.active.saturating_add(1);
                }
                TaskRunStatus::Succeeded => stats.succeeded = stats.succeeded.saturating_add(1),
                TaskRunStatus::Failed => stats.failed = stats.failed.saturating_add(1),
                TaskRunStatus::Cancelled => stats.cancelled = stats.cancelled.saturating_add(1),
                TaskRunStatus::Blocked => stats.blocked = stats.blocked.saturating_add(1),
            }
        }
        stats
    }

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

    pub(in crate::store) fn get_running_run_for_execution_lane(
        &self,
        execution_lane_key: &str,
        exclude_run_id: &str,
    ) -> Option<TaskRunRecord> {
        self.inner
            .read()
            .runs
            .values()
            .filter(|run| {
                run.id != exclude_run_id
                    && run.status == TaskRunStatus::Running
                    && run.execution_lane_key.as_deref() == Some(execution_lane_key)
            })
            .min_by(|left, right| {
                left.started_at
                    .cmp(&right.started_at)
                    .then_with(|| left.id.cmp(&right.id))
            })
            .cloned()
    }

    pub(in crate::store) fn subscribe_run_terminal(
        &self,
        subscription: RunTerminalSubscriptionRecord,
    ) -> Result<TaskRunRecord, String> {
        let mut data = self.inner.write();
        let run = data
            .runs
            .get(subscription.run_id.as_str())
            .cloned()
            .ok_or_else(|| format!("运行不存在: {}", subscription.run_id))?;
        if !task_run_status_is_terminal(run.status) {
            data.run_terminal_subscriptions
                .insert(subscription.id.clone(), subscription);
        }
        Ok(run)
    }

    pub(in crate::store) fn list_pending_run_terminal_subscriptions(
        &self,
        limit: usize,
    ) -> Vec<(TaskRunRecord, RunTerminalSubscriptionRecord)> {
        let data = self.inner.read();
        data.run_terminal_subscriptions
            .values()
            .filter_map(|subscription| {
                let run = data.runs.get(subscription.run_id.as_str())?;
                task_run_status_is_terminal(run.status).then(|| (run.clone(), subscription.clone()))
            })
            .take(limit.max(1))
            .collect()
    }

    pub(in crate::store) fn acknowledge_run_terminal_subscription(
        &self,
        subscription_id: &str,
    ) -> bool {
        self.inner
            .write()
            .run_terminal_subscriptions
            .remove(subscription_id)
            .is_some()
    }

    pub(in crate::store) fn save_run(
        &self,
        mut run: TaskRunRecord,
    ) -> Result<TaskRunRecord, String> {
        let mut data = self.inner.write();
        if let Some(current) = data.runs.get(&run.id) {
            merge_run_async_progress(&mut run, current);
        }
        let persisted = if let Some(claim_token) = run.claim_token.as_deref() {
            let Some(current) = data.runs.get(&run.id) else {
                return Err(lost_run_claim_error(&run.id));
            };
            if current.claim_token.as_deref() != Some(claim_token)
                || current.worker_id.as_deref() != run.worker_id.as_deref()
            {
                return Err(lost_run_claim_error(&run.id));
            }
            if current.cancel_requested {
                run.cancel_requested = true;
                run.cancel_event_pending |= current.cancel_event_pending;
            }
            prepare_run_for_claim_guarded_persist(run)
        } else {
            prepare_run_for_claim_guarded_persist(run)
        };
        data.runs.insert(persisted.id.clone(), persisted.clone());
        Ok(persisted)
    }

    pub(in crate::store) fn set_queued_runs_dispatch_paused(
        &self,
        task_ids: &[String],
        paused: bool,
    ) -> usize {
        let task_ids = task_ids.iter().map(String::as_str).collect::<BTreeSet<_>>();
        let mut data = self.inner.write();
        let mut updated = 0;
        for run in data.runs.values_mut() {
            if run.status != TaskRunStatus::Queued || !task_ids.contains(run.task_id.as_str()) {
                continue;
            }
            run.dispatch_paused = paused;
            run.dispatch_event_pending = !paused;
            run.updated_at = now_rfc3339();
            updated += 1;
        }
        updated
    }

    pub(in crate::store) fn list_pending_run_post_processes(
        &self,
        limit: usize,
    ) -> Vec<TaskRunRecord> {
        let data = self.inner.read();
        let mut runs = data
            .runs
            .values()
            .filter(|run| {
                task_run_status_is_terminal(run.status)
                    && run.post_process_event_pending
                    && !run.post_process_dead_lettered
            })
            .cloned()
            .collect::<Vec<_>>();
        runs.sort_by(|left, right| {
            left.updated_at
                .cmp(&right.updated_at)
                .then_with(|| left.id.cmp(&right.id))
        });
        runs.truncate(limit.max(1));
        runs
    }

    pub(in crate::store) fn acknowledge_run_post_process_event(&self, run_id: &str) -> bool {
        let mut data = self.inner.write();
        let Some(run) = data.runs.get_mut(run_id) else {
            return false;
        };
        if !run.post_process_event_pending
            || run.post_process_completed
            || run.post_process_dead_lettered
        {
            return false;
        }
        run.post_process_event_pending = false;
        run.post_process_event_enqueued = true;
        run.updated_at = now_rfc3339();
        true
    }

    pub(in crate::store) fn record_run_post_process_failure(
        &self,
        run_id: &str,
        error: &str,
    ) -> bool {
        let mut data = self.inner.write();
        let Some(run) = data.runs.get_mut(run_id) else {
            return false;
        };
        if run.post_process_completed || run.post_process_dead_lettered {
            return false;
        }
        run.post_process_attempt_count = run.post_process_attempt_count.saturating_add(1);
        run.post_process_last_error = Some(error.to_string());
        run.updated_at = now_rfc3339();
        true
    }

    pub(in crate::store) fn mark_run_memory_summary_processed(
        &self,
        run_id: &str,
        summary_job_run_id: Option<&str>,
    ) -> bool {
        let mut data = self.inner.write();
        let Some(run) = data.runs.get_mut(run_id) else {
            return false;
        };
        if run.post_process_dead_lettered {
            return false;
        }
        run.memory_summary_processed = true;
        if let Some(summary_job_run_id) = summary_job_run_id {
            run.summary_job_run_id = Some(summary_job_run_id.to_string());
        }
        run.updated_at = now_rfc3339();
        true
    }

    pub(in crate::store) fn mark_run_chatos_followup_processed(&self, run_id: &str) -> bool {
        let mut data = self.inner.write();
        let Some(run) = data.runs.get_mut(run_id) else {
            return false;
        };
        if run.post_process_dead_lettered {
            return false;
        }
        run.chatos_followup_processed = true;
        run.updated_at = now_rfc3339();
        true
    }

    pub(in crate::store) fn mark_run_post_process_completed(&self, run_id: &str) -> bool {
        let mut data = self.inner.write();
        let Some(run) = data.runs.get_mut(run_id) else {
            return false;
        };
        if run.post_process_dead_lettered {
            return false;
        }
        run.post_process_event_pending = false;
        run.post_process_event_enqueued = false;
        run.post_process_completed = true;
        run.post_process_last_error = None;
        run.updated_at = now_rfc3339();
        true
    }

    pub(in crate::store) fn mark_run_post_process_dead_lettered(
        &self,
        run_id: &str,
        error: &str,
    ) -> bool {
        let mut data = self.inner.write();
        let Some(run) = data.runs.get_mut(run_id) else {
            return false;
        };
        if run.post_process_completed {
            return false;
        }
        run.post_process_event_pending = false;
        run.post_process_event_enqueued = false;
        run.post_process_dead_lettered = true;
        run.post_process_last_error = Some(error.to_string());
        run.updated_at = now_rfc3339();
        true
    }

    pub(in crate::store) fn rearm_run_post_process_dead_letter(&self, run_id: &str) -> bool {
        let mut data = self.inner.write();
        let Some(run) = data.runs.get_mut(run_id) else {
            return false;
        };
        if !task_run_status_is_terminal(run.status)
            || run.post_process_completed
            || !run.post_process_dead_lettered
        {
            return false;
        }
        run.post_process_dead_lettered = false;
        run.post_process_attempt_count = 0;
        run.post_process_event_pending = true;
        run.post_process_event_enqueued = false;
        run.post_process_last_error = None;
        run.updated_at = now_rfc3339();
        true
    }

    pub(in crate::store) fn list_pending_chatos_callback_runs(
        &self,
        now: &str,
        limit: usize,
    ) -> Vec<TaskRunRecord> {
        let data = self.inner.read();
        let mut runs = data
            .runs
            .values()
            .filter(|run| {
                run.chatos_callback_delivery
                    .as_ref()
                    .is_some_and(|delivery| {
                        delivery.status == ChatosCallbackDeliveryStatus::Pending
                            && delivery
                                .next_attempt_at
                                .as_deref()
                                .is_none_or(|next_attempt_at| next_attempt_at <= now)
                    })
            })
            .cloned()
            .collect::<Vec<_>>();
        runs.sort_by(|left, right| left.updated_at.cmp(&right.updated_at));
        runs.truncate(limit);
        runs
    }

    pub(in crate::store) fn mark_cancel_requested(&self, run_id: &str) -> Option<TaskRunRecord> {
        let mut data = self.inner.write();
        data.cancel_requested_runs.insert(run_id.to_string());
        let run = data.runs.get_mut(run_id)?;
        run.cancel_requested = true;
        run.cancel_event_pending = run.status == TaskRunStatus::Running && run.worker_id.is_some();
        Some(run.clone())
    }

    pub(in crate::store) fn repair_stale_cancel_requested_runs(&self) -> usize {
        let mut data = self.inner.write();
        let stale_run_ids = data
            .runs
            .values_mut()
            .filter(|run| {
                run.cancel_requested
                    && !matches!(run.status, TaskRunStatus::Queued | TaskRunStatus::Running)
            })
            .map(|run| {
                run.cancel_requested = false;
                run.cancel_event_pending = false;
                run.updated_at = now_rfc3339();
                run.id.clone()
            })
            .collect::<Vec<_>>();
        for run_id in &stale_run_ids {
            data.cancel_requested_runs.remove(run_id);
        }
        stale_run_ids.len()
    }

    pub(in crate::store) fn list_pending_run_cancel_events(
        &self,
        limit: usize,
    ) -> Vec<TaskRunRecord> {
        let data = self.inner.read();
        let mut runs = data
            .runs
            .values()
            .filter(|run| {
                run.status == TaskRunStatus::Running
                    && run.cancel_requested
                    && run.cancel_event_pending
                    && run.worker_id.is_some()
            })
            .cloned()
            .collect::<Vec<_>>();
        runs.sort_by(|left, right| {
            left.updated_at
                .cmp(&right.updated_at)
                .then_with(|| left.id.cmp(&right.id))
        });
        runs.truncate(limit.max(1));
        runs
    }

    pub(in crate::store) fn acknowledge_run_cancel_event(&self, run_id: &str) -> bool {
        let mut data = self.inner.write();
        let Some(run) = data.runs.get_mut(run_id) else {
            return false;
        };
        if !run.cancel_event_pending {
            return false;
        }
        run.cancel_event_pending = false;
        true
    }

    pub(in crate::store) fn clear_cancel_requested(&self, run_id: &str) {
        let mut data = self.inner.write();
        data.cancel_requested_runs.remove(run_id);
        if let Some(run) = data.runs.get_mut(run_id) {
            run.cancel_requested = false;
            run.cancel_event_pending = false;
        }
    }

    pub(in crate::store) fn signal_local_run_abort(&self, run_id: &str) {
        self.inner
            .write()
            .cancel_requested_runs
            .insert(run_id.to_string());
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
