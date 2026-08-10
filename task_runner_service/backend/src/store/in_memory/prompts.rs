// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl InMemoryStore {
    pub(in crate::store) fn list_ask_user_prompts(
        &self,
        task_id: Option<&str>,
        run_id: Option<&str>,
        status: Option<AskUserPromptStatus>,
    ) -> Vec<AskUserPromptRecord> {
        let data = self.inner.read();
        let mut items = data
            .ask_user_prompts
            .values()
            .filter(|prompt| task_id.is_none_or(|value| prompt.task_id.as_deref() == Some(value)))
            .filter(|prompt| run_id.is_none_or(|value| prompt.run_id.as_deref() == Some(value)))
            .filter(|prompt| status.is_none_or(|value| prompt.status == value))
            .cloned()
            .collect::<Vec<_>>();
        items.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        items
    }

    pub(in crate::store) fn list_ask_user_prompts_page(
        &self,
        filters: &PromptListFilters,
    ) -> PaginatedResponse<AskUserPromptRecord> {
        let items = self.list_ask_user_prompts(
            filters.task_id.as_deref(),
            filters.run_id.as_deref(),
            filters.status,
        );
        let total = items.len();
        build_page_response(
            slice_page_items(
                items,
                filters.offset.unwrap_or(0),
                filters.limit.unwrap_or(DEFAULT_PAGE_LIMIT),
            ),
            total,
            filters.limit.unwrap_or(DEFAULT_PAGE_LIMIT),
            filters.offset.unwrap_or(0),
        )
    }

    pub(in crate::store) fn get_ask_user_prompt(&self, id: &str) -> Option<AskUserPromptRecord> {
        self.inner.read().ask_user_prompts.get(id).cloned()
    }

    pub(in crate::store) fn save_ask_user_prompt(
        &self,
        prompt: AskUserPromptRecord,
    ) -> AskUserPromptRecord {
        let mut data = self.inner.write();
        data.ask_user_prompts
            .insert(prompt.id.clone(), prompt.clone());
        prompt
    }

    pub(in crate::store) fn prune_terminal_ask_user_prompts_before(
        &self,
        cutoff: &str,
        candidate_limit: usize,
    ) -> AskUserPromptPruneResult {
        let mut data = self.inner.write();
        let mut eligible_prompt_ids = data
            .ask_user_prompts
            .values()
            .filter(|prompt| prompt.status != AskUserPromptStatus::Pending)
            .filter(|prompt| !prompt.resolution_event_pending)
            .filter(|prompt| prompt.updated_at.as_str() < cutoff)
            .filter(|prompt| {
                prompt
                    .run_id
                    .as_deref()
                    .and_then(|run_id| data.runs.get(run_id))
                    .is_some_and(|run| task_run_status_is_terminal(run.status))
            })
            .map(|prompt| (prompt.updated_at.clone(), prompt.id.clone()))
            .collect::<Vec<_>>();
        eligible_prompt_ids.sort();
        eligible_prompt_ids.truncate(candidate_limit);

        let mut deleted_prompts = 0_u64;
        for (_, prompt_id) in &eligible_prompt_ids {
            deleted_prompts +=
                u64::from(data.ask_user_prompts.remove(prompt_id.as_str()).is_some());
        }
        AskUserPromptPruneResult {
            eligible_prompts: eligible_prompt_ids.len(),
            deleted_prompts,
        }
    }

    pub(in crate::store) fn list_pending_ask_user_resolution_events(
        &self,
        limit: usize,
    ) -> Vec<AskUserPromptRecord> {
        self.inner
            .read()
            .ask_user_prompts
            .values()
            .filter(|prompt| {
                prompt.resolution_event_pending && prompt.status != AskUserPromptStatus::Pending
            })
            .take(limit.max(1))
            .cloned()
            .collect()
    }

    pub(in crate::store) fn acknowledge_ask_user_resolution_event(&self, prompt_id: &str) -> bool {
        let mut data = self.inner.write();
        let Some(prompt) = data.ask_user_prompts.get_mut(prompt_id) else {
            return false;
        };
        if !prompt.resolution_event_pending {
            return false;
        }
        prompt.resolution_event_pending = false;
        true
    }

    pub(in crate::store) fn list_ask_user_prompt_task_counts(
        &self,
        status: Option<AskUserPromptStatus>,
    ) -> Vec<AskUserPromptTaskCountRecord> {
        let data = self.inner.read();
        let mut counts = BTreeMap::<String, usize>::new();

        for prompt in data.ask_user_prompts.values() {
            if status.is_some_and(|value| prompt.status != value) {
                continue;
            }
            let Some(task_id) = prompt.task_id.as_deref() else {
                continue;
            };
            *counts.entry(task_id.to_string()).or_default() += 1;
        }

        let mut items = counts
            .into_iter()
            .map(|(task_id, count)| AskUserPromptTaskCountRecord { task_id, count })
            .collect::<Vec<_>>();
        items.sort_by(|left, right| {
            right
                .count
                .cmp(&left.count)
                .then(left.task_id.cmp(&right.task_id))
        });
        items
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> InMemoryStore {
        let (sender, _) = broadcast::channel(16);
        InMemoryStore::new(sender)
    }

    #[test]
    fn ask_user_prompt_retention_only_prunes_safe_terminal_records() {
        let store = test_store();
        store
            .save_run(run("run-terminal", TaskRunStatus::Succeeded))
            .expect("save terminal run");
        store
            .save_run(run("run-active", TaskRunStatus::Running))
            .expect("save active run");
        for prompt in [
            prompt(
                "expired-terminal",
                "run-terminal",
                AskUserPromptStatus::Submitted,
                false,
                "2026-06-01T00:00:00Z",
            ),
            prompt(
                "recent-terminal",
                "run-terminal",
                AskUserPromptStatus::Submitted,
                false,
                "2026-08-05T00:00:00Z",
            ),
            prompt(
                "pending-prompt",
                "run-terminal",
                AskUserPromptStatus::Pending,
                false,
                "2026-06-01T00:00:00Z",
            ),
            prompt(
                "pending-resolution-event",
                "run-terminal",
                AskUserPromptStatus::Cancelled,
                true,
                "2026-06-01T00:00:00Z",
            ),
            prompt(
                "active-run",
                "run-active",
                AskUserPromptStatus::Failed,
                false,
                "2026-06-01T00:00:00Z",
            ),
        ] {
            store.save_ask_user_prompt(prompt);
        }

        let result = store.prune_terminal_ask_user_prompts_before("2026-08-01T00:00:00Z", 100);

        assert_eq!(result.eligible_prompts, 1);
        assert_eq!(result.deleted_prompts, 1);
        assert!(store.get_ask_user_prompt("expired-terminal").is_none());
        for prompt_id in [
            "recent-terminal",
            "pending-prompt",
            "pending-resolution-event",
            "active-run",
        ] {
            assert!(store.get_ask_user_prompt(prompt_id).is_some());
        }
    }

    fn prompt(
        id: &str,
        run_id: &str,
        status: AskUserPromptStatus,
        resolution_event_pending: bool,
        updated_at: &str,
    ) -> AskUserPromptRecord {
        AskUserPromptRecord {
            id: id.to_string(),
            task_id: Some("task-1".to_string()),
            run_id: Some(run_id.to_string()),
            conversation_id: "conversation-1".to_string(),
            conversation_turn_id: "turn-1".to_string(),
            tool_call_id: Some("tool-call-1".to_string()),
            kind: "prompt_key_values".to_string(),
            title: "Approval".to_string(),
            message: "Continue?".to_string(),
            allow_cancel: true,
            timeout_ms: 60_000,
            payload: serde_json::json!({}),
            response: None,
            status,
            resolution_event_pending,
            created_at: updated_at.to_string(),
            updated_at: updated_at.to_string(),
            expires_at: None,
        }
    }

    fn run(id: &str, status: TaskRunStatus) -> TaskRunRecord {
        let now = now_rfc3339();
        TaskRunRecord {
            id: id.to_string(),
            task_id: "task-1".to_string(),
            execution_lane_key: None,
            model_config_id: "model-1".to_string(),
            memory_thread_id: "thread-1".to_string(),
            status,
            started_at: None,
            finished_at: task_run_status_is_terminal(status).then(|| now.clone()),
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
            dispatch_event_pending: false,
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
}
