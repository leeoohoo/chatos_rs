// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

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
            stats.terminal_cleanup_outbox_pending +=
                usize::from(run.terminal_cleanup_event_pending);
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

    pub(in crate::store) fn claim_next_queued_run(
        &self,
        worker_id: &str,
        claim_token: &str,
        claim_until: &str,
    ) -> Option<TaskRunRecord> {
        let mut data = self.inner.write();
        let active_execution_lanes = data
            .runs
            .values()
            .filter(|run| run.status == TaskRunStatus::Running)
            .filter_map(|run| run.execution_lane_key.clone())
            .collect::<BTreeSet<_>>();
        let run_id = data
            .runs
            .values()
            .filter(|run| run.status == TaskRunStatus::Queued && !run.dispatch_paused)
            .filter(|run| {
                run.execution_lane_key
                    .as_deref()
                    .is_none_or(|lane| !active_execution_lanes.contains(lane))
            })
            .min_by(|left, right| {
                left.created_at
                    .cmp(&right.created_at)
                    .then_with(|| left.id.cmp(&right.id))
            })
            .map(|run| run.id.clone())?;
        let run = data.runs.get_mut(&run_id)?;
        run.status = TaskRunStatus::Running;
        run.dispatch_event_pending = false;
        run.worker_id = Some(worker_id.to_string());
        run.claim_token = Some(claim_token.to_string());
        run.claim_until = Some(claim_until.to_string());
        run.attempt += 1;
        let attempt_started_at = now_rfc3339();
        run.begin_attempt(claim_token, attempt_started_at.as_str());
        run.finished_at = None;
        run.result_summary = None;
        run.error_message = None;
        if run.started_at.is_none() {
            run.started_at = Some(attempt_started_at);
        }
        run.updated_at = now_rfc3339();
        Some(run.clone())
    }

    pub(in crate::store) fn has_queued_run_waiting_for_execution(&self) -> bool {
        self.inner
            .read()
            .runs
            .values()
            .any(|run| run.status == TaskRunStatus::Queued && !run.dispatch_paused)
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

    pub(in crate::store) fn list_pending_run_dispatches(&self, limit: usize) -> Vec<TaskRunRecord> {
        let data = self.inner.read();
        let mut runs = data
            .runs
            .values()
            .filter(|run| {
                run.status == TaskRunStatus::Queued
                    && !run.dispatch_paused
                    && run.dispatch_event_pending
            })
            .cloned()
            .collect::<Vec<_>>();
        runs.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.id.cmp(&right.id))
        });
        runs.truncate(limit.max(1));
        runs
    }

    pub(in crate::store) fn acknowledge_run_dispatch_event(&self, run_id: &str) -> bool {
        let mut data = self.inner.write();
        let Some(run) = data.runs.get_mut(run_id) else {
            return false;
        };
        if run.status != TaskRunStatus::Queued || !run.dispatch_event_pending {
            return false;
        }
        run.dispatch_event_pending = false;
        true
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
                run.status == TaskRunStatus::Succeeded
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
        if run.status != TaskRunStatus::Succeeded
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

    pub(in crate::store) fn list_pending_terminal_cleanups(
        &self,
        limit: usize,
    ) -> Vec<TaskRunRecord> {
        let data = self.inner.read();
        let mut runs = data
            .runs
            .values()
            .filter(|run| run.terminal_cleanup_event_pending && run.worker_id.is_some())
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

    pub(in crate::store) fn acknowledge_terminal_cleanup_event(&self, run_id: &str) -> bool {
        let mut data = self.inner.write();
        let Some(run) = data.runs.get_mut(run_id) else {
            return false;
        };
        if !run.terminal_cleanup_event_pending || run.terminal_cleanup_completed {
            return false;
        }
        run.terminal_cleanup_event_pending = false;
        run.terminal_cleanup_event_enqueued = true;
        run.updated_at = now_rfc3339();
        true
    }

    pub(in crate::store) fn retry_terminal_cleanup(&self, run_id: &str, error: &str) -> bool {
        let mut data = self.inner.write();
        let Some(run) = data.runs.get_mut(run_id) else {
            return false;
        };
        if run.terminal_cleanup_completed {
            return false;
        }
        run.terminal_cleanup_event_pending = true;
        run.terminal_cleanup_event_enqueued = false;
        run.terminal_cleanup_attempt_count = run.terminal_cleanup_attempt_count.saturating_add(1);
        run.terminal_cleanup_last_error = Some(error.to_string());
        run.updated_at = now_rfc3339();
        true
    }

    pub(in crate::store) fn mark_terminal_cleanup_completed(&self, run_id: &str) -> bool {
        let mut data = self.inner.write();
        let Some(run) = data.runs.get_mut(run_id) else {
            return false;
        };
        run.terminal_cleanup_event_pending = false;
        run.terminal_cleanup_event_enqueued = false;
        run.terminal_cleanup_completed = true;
        run.terminal_cleanup_last_error = None;
        run.updated_at = now_rfc3339();
        true
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

    pub(in crate::store) fn reconcile_expired_run_claims(
        &self,
        expired_before: &str,
        reconciled_at: &str,
        max_attempts: i64,
    ) -> Vec<TaskRunRecord> {
        let mut data = self.inner.write();
        let cancel_requested_runs = data.cancel_requested_runs.clone();
        let mut terminal_run_ids = Vec::new();
        let mut reconciled_runs = Vec::new();
        for run in data.runs.values_mut() {
            if run.status != TaskRunStatus::Running {
                continue;
            }
            let expired = run
                .claim_until
                .as_deref()
                .is_some_and(|claim_until| claim_until <= expired_before);
            if !expired {
                continue;
            }
            let was_cancel_requested =
                run.cancel_requested || cancel_requested_runs.contains(run.id.as_str());
            let attempt_status = if was_cancel_requested {
                TaskRunAttemptStatus::Cancelled
            } else if run.attempt < max_attempts.max(1) {
                TaskRunAttemptStatus::Interrupted
            } else {
                TaskRunAttemptStatus::Failed
            };
            run.finish_current_attempt(attempt_status, reconciled_at);
            if was_cancel_requested {
                run.status = TaskRunStatus::Cancelled;
                run.result_summary =
                    Some("任务取消请求已生效；运行节点心跳过期后按取消收尾".to_string());
                run.error_message = None;
                run.finished_at = Some(reconciled_at.to_string());
                ensure_terminal_callback_pending(run);
                terminal_run_ids.push(run.id.clone());
            } else if run.attempt < max_attempts.max(1) {
                run.status = TaskRunStatus::Queued;
                run.dispatch_event_pending = !run.dispatch_paused;
                run.finished_at = None;
                run.result_summary = Some("任务运行节点中断，已自动重新排队恢复".to_string());
                run.error_message = None;
                run.usage = None;
                run.report = None;
                run.summary_job_run_id = None;
                run.chatos_callback_delivery = None;
            } else {
                run.status = TaskRunStatus::Failed;
                run.result_summary = Some(format!(
                    "任务运行节点连续中断，达到 {max_attempts} 次尝试上限后标记为失败"
                ));
                run.error_message = Some("worker claim expired".to_string());
                run.finished_at = Some(reconciled_at.to_string());
                ensure_terminal_callback_pending(run);
                terminal_run_ids.push(run.id.clone());
            }
            run.updated_at = reconciled_at.to_string();
            run.cancel_requested = false;
            run.claim_token = None;
            run.claim_until = None;
            reconciled_runs.push(run.clone());
        }
        for run_id in terminal_run_ids {
            data.cancel_requested_runs.remove(run_id.as_str());
        }
        reconciled_runs
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

    pub(in crate::store) fn list_run_events(&self, run_id: &str) -> Vec<TaskRunEventRecord> {
        self.inner
            .read()
            .run_events
            .get(run_id)
            .cloned()
            .unwrap_or_default()
    }

    pub(in crate::store) fn get_run_event(
        &self,
        run_id: &str,
        event_id: &str,
    ) -> Option<TaskRunEventRecord> {
        self.inner
            .read()
            .run_events
            .get(run_id)
            .and_then(|events| events.iter().find(|event| event.id == event_id))
            .cloned()
    }

    pub(in crate::store) fn list_run_events_after(
        &self,
        run_id: &str,
        after_created_at: Option<&str>,
        after_id: Option<&str>,
        limit: usize,
    ) -> Vec<TaskRunEventRecord> {
        let events = self
            .inner
            .read()
            .run_events
            .get(run_id)
            .cloned()
            .unwrap_or_default();
        let mut items = events
            .into_iter()
            .filter(|event| run_event_is_after_cursor(event, after_created_at, after_id))
            .collect::<Vec<_>>();
        items.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then(left.id.cmp(&right.id))
        });
        items.truncate(limit);
        items
    }

    pub(in crate::store) fn latest_run_event_cursor(
        &self,
        run_id: &str,
    ) -> Option<(String, String)> {
        self.inner.read().run_events.get(run_id).and_then(|events| {
            events
                .iter()
                .max_by(|left, right| {
                    left.created_at
                        .cmp(&right.created_at)
                        .then(left.id.cmp(&right.id))
                })
                .map(|event| (event.created_at.clone(), event.id.clone()))
        })
    }

    pub(in crate::store) fn prune_terminal_run_events_before(
        &self,
        cutoff: &str,
        candidate_limit: usize,
    ) -> RunEventPruneResult {
        let mut data = self.inner.write();
        let mut eligible_run_ids = data
            .run_events
            .iter()
            .filter(|(run_id, events)| {
                events
                    .iter()
                    .any(|event| event.created_at.as_str() < cutoff)
                    && data
                        .runs
                        .get(run_id.as_str())
                        .is_some_and(|run| task_run_status_is_terminal(run.status))
            })
            .map(|(run_id, _)| run_id.clone())
            .collect::<Vec<_>>();
        eligible_run_ids.sort();
        eligible_run_ids.truncate(candidate_limit);

        let mut deleted_events = 0_u64;
        for run_id in &eligible_run_ids {
            let remove_entry = if let Some(events) = data.run_events.get_mut(run_id) {
                let previous_len = events.len();
                events.retain(|event| event.created_at.as_str() >= cutoff);
                deleted_events =
                    deleted_events.saturating_add(previous_len.saturating_sub(events.len()) as u64);
                events.is_empty()
            } else {
                false
            };
            if remove_entry {
                data.run_events.remove(run_id);
            }
        }

        RunEventPruneResult {
            eligible_runs: eligible_run_ids.len(),
            deleted_events,
        }
    }

    pub(in crate::store) fn append_run_event(&self, event: TaskRunEventRecord) {
        let mut data = self.inner.write();
        data.run_events
            .entry(event.run_id.clone())
            .or_default()
            .push(event);
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

    pub(in crate::store) fn clear_local_run_abort(&self, run_id: &str) {
        self.inner.write().cancel_requested_runs.remove(run_id);
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

fn run_event_is_after_cursor(
    event: &TaskRunEventRecord,
    after_created_at: Option<&str>,
    after_id: Option<&str>,
) -> bool {
    match (after_created_at, after_id) {
        (Some(created_at), Some(id)) => {
            event.created_at.as_str() > created_at
                || (event.created_at.as_str() == created_at && event.id.as_str() > id)
        }
        _ => true,
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
            execution_lane_key: None,
            model_config_id: "model-1".to_string(),
            memory_thread_id: "thread-1".to_string(),
            status: TaskRunStatus::Queued,
            started_at: None,
            finished_at: None,
            input_snapshot: serde_json::json!({}),
            plugin_snapshots: Vec::new(),
            context_snapshot: None,
            result_summary: None,
            error_message: None,
            usage: None,
            report: None,
            cancel_requested: false,
            cancel_event_pending: false,
            dispatch_paused: false,
            dispatch_event_pending: true,
            post_process_event_pending: false,
            post_process_event_enqueued: false,
            post_process_completed: false,
            post_process_dead_lettered: false,
            post_process_attempt_count: 0,
            post_process_last_error: None,
            memory_summary_processed: false,
            chatos_followup_processed: false,
            terminal_cleanup_event_pending: false,
            terminal_cleanup_event_enqueued: false,
            terminal_cleanup_completed: false,
            terminal_cleanup_attempt_count: 0,
            terminal_cleanup_last_error: None,
            summary_job_run_id: None,
            worker_id: None,
            claim_token: None,
            claim_until: None,
            attempt: 0,
            attempts: Vec::new(),
            chatos_callback_delivery: None,
            created_at: now.clone(),
            updated_at: now,
        }
    }

    #[test]
    fn execution_stats_count_runs_and_pending_outboxes_without_cloning_records() {
        let store = test_store();
        store.save_run(queued_run()).expect("save queued run");

        let mut running = queued_run();
        running.id = "run-2".to_string();
        running.task_id = "task-2".to_string();
        running.status = TaskRunStatus::Running;
        running.cancel_requested = true;
        running.cancel_event_pending = true;
        running.worker_id = Some("worker-1".to_string());
        store.save_run(running).expect("save running run");

        let mut succeeded = queued_run();
        succeeded.id = "run-3".to_string();
        succeeded.task_id = "task-3".to_string();
        succeeded.status = TaskRunStatus::Succeeded;
        succeeded.dispatch_event_pending = false;
        succeeded.post_process_event_pending = true;
        succeeded.terminal_cleanup_event_pending = true;
        store.save_run(succeeded).expect("save succeeded run");

        let stats = store.run_execution_stats();

        assert_eq!(stats.total, 3);
        assert_eq!(stats.active, 2);
        assert_eq!(stats.queued, 1);
        assert_eq!(stats.running, 1);
        assert_eq!(stats.succeeded, 1);
        assert_eq!(stats.dispatch_outbox_pending, 1);
        assert_eq!(stats.cancellation_outbox_pending, 1);
        assert_eq!(stats.post_process_outbox_pending, 1);
        assert_eq!(stats.terminal_cleanup_outbox_pending, 1);
    }

    #[test]
    fn stale_cancel_repair_updates_terminal_runs_in_place() {
        let store = test_store();
        let mut terminal = queued_run();
        terminal.status = TaskRunStatus::Succeeded;
        store.save_run(terminal).expect("save terminal run");
        store
            .mark_cancel_requested("run-1")
            .expect("mark terminal cancellation");

        let mut running = queued_run();
        running.id = "run-2".to_string();
        running.task_id = "task-2".to_string();
        running.status = TaskRunStatus::Running;
        running.worker_id = Some("worker-1".to_string());
        store.save_run(running).expect("save running run");
        store
            .mark_cancel_requested("run-2")
            .expect("mark running cancellation");

        assert_eq!(store.repair_stale_cancel_requested_runs(), 1);
        assert!(
            !store
                .get_run("run-1")
                .expect("terminal run")
                .cancel_requested
        );
        assert!(
            store
                .get_run("run-2")
                .expect("running run")
                .cancel_requested
        );
    }

    #[test]
    fn execution_lane_allows_only_one_running_project_task() {
        let store = test_store();
        let mut first = queued_run();
        first.execution_lane_key = Some("project:one".to_string());
        store.save_run(first).expect("save first run");

        let mut second = queued_run();
        second.id = "run-2".to_string();
        second.task_id = "task-2".to_string();
        second.execution_lane_key = Some("project:one".to_string());
        store.save_run(second).expect("save second run");

        let mut other_project = queued_run();
        other_project.id = "run-3".to_string();
        other_project.task_id = "task-3".to_string();
        other_project.execution_lane_key = Some("project:two".to_string());
        store
            .save_run(other_project)
            .expect("save other project run");

        let claimed_first = store
            .claim_next_queued_run("worker-1", "claim-1", "2999-01-01T00:00:00Z")
            .expect("claim first lane");
        assert_eq!(claimed_first.id, "run-1");

        let claimed_other = store
            .claim_next_queued_run("worker-2", "claim-3", "2999-01-01T00:00:00Z")
            .expect("claim other project");
        assert_eq!(claimed_other.id, "run-3");

        let mut finished_first = claimed_first;
        finished_first.status = TaskRunStatus::Succeeded;
        finished_first.updated_at = now_rfc3339();
        store.save_run(finished_first).expect("finish first lane");

        let claimed_second = store
            .claim_next_queued_run("worker-3", "claim-4", "2999-01-01T00:00:00Z")
            .expect("claim released lane");
        assert_eq!(claimed_second.id, "run-2");
    }

    fn run_event(id: &str, created_at: &str) -> TaskRunEventRecord {
        TaskRunEventRecord {
            id: id.to_string(),
            run_id: "run-1".to_string(),
            event_type: "task.log".to_string(),
            message: Some(id.to_string()),
            payload: None,
            created_at: created_at.to_string(),
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
        assert_eq!(saved.attempts.len(), 1);
        assert_eq!(saved.attempts[0].status, TaskRunAttemptStatus::Succeeded);
        assert!(saved.attempts[0].finished_at.is_some());
        assert_eq!(saved.worker_id.as_deref(), Some("worker-1"));
        assert!(saved.claim_token.is_none());
        assert!(saved.claim_until.is_none());
        assert_eq!(
            saved
                .chatos_callback_delivery
                .as_ref()
                .map(|state| state.status),
            Some(ChatosCallbackDeliveryStatus::Pending)
        );
        let persisted = store.get_run("run-1").expect("persisted run");
        assert!(persisted.claim_token.is_none());
        assert!(persisted.claim_until.is_none());
    }

    #[test]
    fn paused_queued_run_is_not_claimed_until_resumed() {
        let store = test_store();
        store.save_run(queued_run()).expect("save queued run");
        assert_eq!(
            store.set_queued_runs_dispatch_paused(&["task-1".to_string()], true),
            1
        );
        assert!(store
            .claim_next_queued_run("worker-1", "claim-1", "2999-01-01T00:00:00Z")
            .is_none());

        assert_eq!(
            store.set_queued_runs_dispatch_paused(&["task-1".to_string()], false),
            1
        );
        assert!(store
            .claim_next_queued_run("worker-1", "claim-2", "2999-01-01T00:00:00Z")
            .is_some());
    }

    #[test]
    fn paused_queued_run_is_not_waiting_or_claimable() {
        let store = test_store();
        store.save_run(queued_run()).expect("save queued run");
        assert!(store.has_queued_run_waiting_for_execution());
        assert_eq!(
            store.set_queued_runs_dispatch_paused(&["task-1".to_string()], true),
            1
        );

        assert!(!store.has_queued_run_waiting_for_execution());
        assert!(store
            .claim_next_queued_run("worker-1", "claim-1", "2999-01-01T00:00:00Z")
            .is_none());
    }

    #[test]
    fn queued_run_dispatch_outbox_is_acknowledgeable() {
        let store = test_store();
        store.save_run(queued_run()).expect("save queued run");

        let pending = store.list_pending_run_dispatches(10);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, "run-1");
        assert!(store.acknowledge_run_dispatch_event("run-1"));
        assert!(store.list_pending_run_dispatches(10).is_empty());
        assert!(!store.acknowledge_run_dispatch_event("run-1"));
    }

    #[test]
    fn successful_run_post_process_outbox_is_monotonic_across_stale_saves() {
        let store = test_store();
        let mut run = queued_run();
        run.status = TaskRunStatus::Succeeded;
        run.finished_at = Some(now_rfc3339());
        let saved = store.save_run(run).expect("save successful run");
        assert!(saved.post_process_event_pending);
        assert!(!saved.post_process_event_enqueued);

        let mut stale = saved.clone();
        assert!(store.acknowledge_run_post_process_event(saved.id.as_str()));
        stale.result_summary = Some("late callback save".to_string());
        let merged = store.save_run(stale).expect("merge stale terminal save");
        assert!(!merged.post_process_event_pending);
        assert!(merged.post_process_event_enqueued);

        assert!(store.mark_run_memory_summary_processed(saved.id.as_str(), Some("job-1")));
        assert!(store.mark_run_chatos_followup_processed(saved.id.as_str()));
        assert!(store.mark_run_post_process_completed(saved.id.as_str()));
        let merged = store.save_run(saved).expect("preserve completed progress");
        assert!(merged.post_process_completed);
        assert!(merged.memory_summary_processed);
        assert!(merged.chatos_followup_processed);
        assert_eq!(merged.summary_job_run_id.as_deref(), Some("job-1"));
        assert!(store.list_pending_run_post_processes(10).is_empty());
    }

    #[test]
    fn terminal_cleanup_failure_returns_event_to_outbox_until_completed() {
        let store = test_store();
        let mut run = queued_run();
        run.status = TaskRunStatus::Failed;
        run.worker_id = Some("worker-1".to_string());
        run.terminal_cleanup_event_pending = true;
        store.save_run(run).expect("save terminal cleanup request");

        assert_eq!(store.list_pending_terminal_cleanups(10).len(), 1);
        assert!(store.acknowledge_terminal_cleanup_event("run-1"));
        assert!(store.list_pending_terminal_cleanups(10).is_empty());
        assert!(store.retry_terminal_cleanup("run-1", "temporary failure"));
        assert_eq!(store.list_pending_terminal_cleanups(10).len(), 1);
        assert!(store.mark_terminal_cleanup_completed("run-1"));
        assert!(store.list_pending_terminal_cleanups(10).is_empty());
        let run = store.get_run("run-1").expect("stored run");
        assert!(run.terminal_cleanup_completed);
        assert_eq!(run.terminal_cleanup_attempt_count, 1);
        assert!(run.terminal_cleanup_last_error.is_none());
    }

    #[test]
    fn dead_lettered_post_process_is_not_rearmed_by_later_run_saves() {
        let store = test_store();
        let mut run = queued_run();
        run.status = TaskRunStatus::Succeeded;
        let saved = store.save_run(run).expect("save successful run");
        assert!(store.acknowledge_run_post_process_event(saved.id.as_str()));
        assert!(store.record_run_post_process_failure(saved.id.as_str(), "poison event"));
        assert!(store.mark_run_post_process_dead_lettered(saved.id.as_str(), "poison event"));

        let merged = store.save_run(saved).expect("merge stale terminal save");
        assert!(merged.post_process_dead_lettered);
        assert!(!merged.post_process_event_pending);
        assert!(!merged.post_process_event_enqueued);
        assert!(store.list_pending_run_post_processes(10).is_empty());
    }

    #[test]
    fn dead_lettered_post_process_can_only_be_explicitly_rearmed() {
        let store = test_store();
        let mut run = queued_run();
        run.status = TaskRunStatus::Succeeded;
        run.post_process_dead_lettered = true;
        run.post_process_attempt_count = 8;
        run.post_process_last_error = Some("poison event".to_string());
        let run_id = run.id.clone();
        store.save_run(run).expect("save dead-lettered run");

        assert!(store.rearm_run_post_process_dead_letter(run_id.as_str()));
        let replay = store.get_run(run_id.as_str()).expect("rearmed run");
        assert!(!replay.post_process_dead_lettered);
        assert_eq!(replay.post_process_attempt_count, 0);
        assert!(replay.post_process_event_pending);
        assert!(!replay.post_process_event_enqueued);
        assert!(replay.post_process_last_error.is_none());
        assert!(!store.rearm_run_post_process_dead_letter(run_id.as_str()));
    }

    #[test]
    fn running_run_cancel_outbox_is_acknowledgeable() {
        let store = test_store();
        store.save_run(queued_run()).expect("save queued run");
        store
            .claim_next_queued_run("worker-1", "claim-1", "2999-01-01T00:00:00Z")
            .expect("claim run");

        let cancelled = store
            .mark_cancel_requested("run-1")
            .expect("mark cancel requested");
        assert!(cancelled.cancel_event_pending);
        assert_eq!(store.list_pending_run_cancel_events(10).len(), 1);
        assert!(store.acknowledge_run_cancel_event("run-1"));
        assert!(store.list_pending_run_cancel_events(10).is_empty());
    }

    #[test]
    fn terminal_run_subscription_becomes_publishable_only_after_terminal_state() {
        let store = test_store();
        let run = store.save_run(queued_run()).expect("save queued run");
        let subscription =
            RunTerminalSubscriptionRecord::new(run.id.as_str(), "parent-run-1", "worker-1");
        store
            .subscribe_run_terminal(subscription.clone())
            .expect("subscribe terminal event");
        assert!(store.list_pending_run_terminal_subscriptions(10).is_empty());

        let mut completed = run;
        completed.status = TaskRunStatus::Succeeded;
        store.save_run(completed).expect("save terminal run");

        let pending = store.list_pending_run_terminal_subscriptions(10);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].1, subscription);
        assert!(store.acknowledge_run_terminal_subscription(subscription.id.as_str()));
        assert!(store.list_pending_run_terminal_subscriptions(10).is_empty());
    }

    #[test]
    fn stale_worker_cannot_save_after_claim_expires() {
        let store = test_store();
        store.save_run(queued_run()).expect("save queued run");
        let mut stale = store
            .claim_next_queued_run("worker-1", "claim-1", "2000-01-01T00:00:00Z")
            .expect("claim run");
        stale.attempt = 3;
        stale = store.save_run(stale).expect("persist exhausted attempts");

        let failed_runs =
            store.reconcile_expired_run_claims("2001-01-01T00:00:00Z", "2001-01-01T00:01:00Z", 3);
        assert_eq!(failed_runs.len(), 1);
        assert_eq!(failed_runs[0].id, "run-1");
        assert_eq!(
            failed_runs[0].finished_at.as_deref(),
            Some("2001-01-01T00:01:00Z")
        );
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

    #[test]
    fn expired_claim_is_requeued_before_attempt_limit() {
        let store = test_store();
        store.save_run(queued_run()).expect("save queued run");
        let first_claim = store
            .claim_next_queued_run("worker-1", "claim-1", "2000-01-01T00:00:00Z")
            .expect("claim run");
        let original_started_at = first_claim.started_at.clone().expect("first start time");
        assert_eq!(first_claim.attempts.len(), 1);
        assert_eq!(first_claim.attempts[0].attempt_id, "claim-1");
        assert_eq!(
            first_claim.attempts[0].status,
            TaskRunAttemptStatus::Running
        );

        let reconciled =
            store.reconcile_expired_run_claims("2001-01-01T00:00:00Z", "2001-01-01T00:01:00Z", 3);

        assert_eq!(reconciled.len(), 1);
        assert_eq!(reconciled[0].status, TaskRunStatus::Queued);
        assert_eq!(reconciled[0].attempt, 1);
        assert_eq!(reconciled[0].attempts.len(), 1);
        assert_eq!(
            reconciled[0].attempts[0].status,
            TaskRunAttemptStatus::Interrupted
        );
        assert_eq!(
            reconciled[0].attempts[0].finished_at.as_deref(),
            Some("2001-01-01T00:01:00Z")
        );
        assert_eq!(
            reconciled[0].started_at.as_deref(),
            Some(original_started_at.as_str())
        );
        assert!(reconciled[0].finished_at.is_none());
        assert!(reconciled[0].claim_token.is_none());
        assert!(reconciled[0].claim_until.is_none());
        assert!(reconciled[0].chatos_callback_delivery.is_none());
        let reclaimed = store
            .claim_next_queued_run("worker-2", "claim-2", "2002-01-01T00:00:00Z")
            .expect("reclaim recovered run");
        assert_eq!(reclaimed.attempt, 2);
        assert_eq!(reclaimed.attempts.len(), 2);
        assert_eq!(reclaimed.attempts[1].attempt_id, "claim-2");
        assert_eq!(reclaimed.attempts[1].sequence, 2);
        assert_eq!(
            reclaimed.attempts[1].recovery_reason.as_deref(),
            Some("worker_claim_expired")
        );
        assert_eq!(reclaimed.attempts[1].status, TaskRunAttemptStatus::Running);
        assert_eq!(
            reclaimed.started_at.as_deref(),
            Some(original_started_at.as_str())
        );
        assert!(reclaimed.result_summary.is_none());
    }

    #[test]
    fn expired_cancel_requested_claim_becomes_cancelled() {
        let store = test_store();
        store.save_run(queued_run()).expect("save queued run");
        store
            .claim_next_queued_run("worker-1", "claim-1", "2000-01-01T00:00:00Z")
            .expect("claim run");
        store
            .mark_cancel_requested("run-1")
            .expect("mark cancel requested");

        let terminal_runs =
            store.reconcile_expired_run_claims("2001-01-01T00:00:00Z", "2001-01-01T00:01:00Z", 3);

        assert_eq!(terminal_runs.len(), 1);
        assert_eq!(terminal_runs[0].status, TaskRunStatus::Cancelled);
        assert_eq!(
            terminal_runs[0].result_summary.as_deref(),
            Some("任务取消请求已生效；运行节点心跳过期后按取消收尾")
        );
        assert_eq!(terminal_runs[0].error_message, None);
        assert_eq!(
            terminal_runs[0]
                .chatos_callback_delivery
                .as_ref()
                .map(|delivery| delivery.event.as_str()),
            Some("task.cancelled")
        );
        assert!(!store.is_cancel_requested("run-1"));
    }

    #[test]
    fn claim_is_not_failed_before_expiry_cutoff() {
        let store = test_store();
        store.save_run(queued_run()).expect("save queued run");
        store
            .claim_next_queued_run("worker-1", "claim-1", "2001-01-01T00:00:00Z")
            .expect("claim run");

        assert!(store
            .reconcile_expired_run_claims("2000-12-31T23:59:59Z", "2001-01-01T00:01:00Z", 3,)
            .is_empty());
        assert_eq!(
            store.get_run("run-1").expect("persisted run").status,
            TaskRunStatus::Running
        );
    }

    #[test]
    fn local_abort_signal_does_not_mutate_persisted_cancel_flag() {
        let store = test_store();
        store.save_run(queued_run()).expect("save queued run");

        store.signal_local_run_abort("run-1");
        assert!(store.is_cancel_requested("run-1"));
        assert!(!store.get_run("run-1").expect("run").cancel_requested);

        store.clear_local_run_abort("run-1");
        assert!(!store.is_cancel_requested("run-1"));
    }

    #[test]
    fn list_run_events_after_returns_incremental_suffix() {
        let store = test_store();
        store.append_run_event(run_event("evt-1", "2026-08-03T10:00:00Z"));
        store.append_run_event(run_event("evt-2", "2026-08-03T10:00:00Z"));
        store.append_run_event(run_event("evt-3", "2026-08-03T10:00:01Z"));

        let events =
            store.list_run_events_after("run-1", Some("2026-08-03T10:00:00Z"), Some("evt-1"), 10);

        assert_eq!(
            events
                .iter()
                .map(|event| event.id.as_str())
                .collect::<Vec<_>>(),
            vec!["evt-2", "evt-3"]
        );
    }

    #[test]
    fn list_run_events_after_respects_limit() {
        let store = test_store();
        store.append_run_event(run_event("evt-1", "2026-08-03T10:00:00Z"));
        store.append_run_event(run_event("evt-2", "2026-08-03T10:00:01Z"));

        let events = store.list_run_events_after("run-1", None, None, 1);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id, "evt-1");
    }

    #[test]
    fn run_event_retention_only_prunes_expired_events_for_terminal_runs() {
        let store = test_store();
        let mut terminal = queued_run();
        terminal.status = TaskRunStatus::Succeeded;
        terminal.finished_at = Some("2026-07-01T00:00:00Z".to_string());
        store.save_run(terminal).expect("save terminal run");
        store.append_run_event(run_event("old-terminal", "2026-07-01T00:00:00Z"));
        store.append_run_event(run_event("new-terminal", "2026-08-02T00:00:00Z"));

        let mut active = queued_run();
        active.id = "run-2".to_string();
        active.task_id = "task-2".to_string();
        active.status = TaskRunStatus::Running;
        store.save_run(active).expect("save active run");
        let mut active_event = run_event("old-active", "2026-07-01T00:00:00Z");
        active_event.run_id = "run-2".to_string();
        store.append_run_event(active_event);

        let result = store.prune_terminal_run_events_before("2026-08-01T00:00:00Z", 100);

        assert_eq!(result.eligible_runs, 1);
        assert_eq!(result.deleted_events, 1);
        assert_eq!(
            store
                .list_run_events("run-1")
                .into_iter()
                .map(|event| event.id)
                .collect::<Vec<_>>(),
            vec!["new-terminal".to_string()]
        );
        assert_eq!(store.list_run_events("run-2").len(), 1);
    }

    #[test]
    fn latest_run_event_cursor_uses_persisted_sort_order() {
        let store = test_store();
        store.append_run_event(run_event("evt-2", "2026-08-03T10:00:00Z"));
        store.append_run_event(run_event("evt-1", "2026-08-03T10:00:01Z"));

        assert_eq!(
            store.latest_run_event_cursor("run-1"),
            Some(("2026-08-03T10:00:01Z".to_string(), "evt-1".to_string()))
        );
    }
}
