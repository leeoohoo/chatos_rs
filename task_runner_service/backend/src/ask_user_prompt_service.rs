// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chatos_mcp::{
    AskUserDecision, AskUserPromptPayload, AskUserResponseSubmission, AskUserStore,
    AskUserStreamChunkCallback,
};
use chrono::{Duration as ChronoDuration, Utc};
use parking_lot::Mutex;
use serde_json::{json, Value};
use tokio::sync::Notify;

use crate::config::AppConfig;
use crate::models::{
    now_rfc3339, AskUserPromptPruneResult, AskUserPromptRecord, AskUserPromptStatus,
    AskUserPromptTaskCountRecord, CancelAskUserPromptRequest, PaginatedResponse, PromptListFilters,
    SubmitAskUserPromptRequest, TaskRunEventRecord,
};
use crate::platform_queue::TaskQueueTopology;
use crate::services::sanitize_prompt_list_filters;
use crate::store::AppStore;

mod chatos_callbacks;
mod execution;
mod prompt_ops;
mod support;
mod waiters;

#[derive(Clone)]
pub struct AskUserPromptService {
    store: AppStore,
    config: Option<AppConfig>,
    task_queue_topology: TaskQueueTopology,
    waiters: AskUserPromptWaiters,
}

#[derive(Clone, Default)]
struct AskUserPromptWaiters {
    inner: Arc<Mutex<HashMap<String, Arc<Notify>>>>,
}

impl AskUserPromptService {
    pub(crate) async fn create_prompt_without_wait(
        &self,
        payload: AskUserPromptPayload,
    ) -> Result<AskUserPromptRecord, String> {
        if let Some(existing) = self
            .store
            .get_ask_user_prompt(payload.prompt_id.as_str())
            .await?
        {
            return Ok(existing);
        }
        let (task_id, run_id) = self.resolve_context_ids(&payload).await?;
        if self.config.is_some() && run_id.is_none() {
            return Err("ask_user requires an active Run for Worker event routing".to_string());
        }
        let created_at = now_rfc3339();
        let expires_at = if payload.timeout_ms > 0 {
            Some(
                (Utc::now()
                    + ChronoDuration::milliseconds(payload.timeout_ms.min(i64::MAX as u64) as i64))
                .to_rfc3339(),
            )
        } else {
            None
        };
        let prompt =
            AskUserPromptRecord::from_payload(payload, task_id, run_id, created_at, expires_at);
        let saved = self.store.save_ask_user_prompt(prompt).await?;
        self.append_prompt_event(
            &saved,
            "ask_user_prompt_pending",
            Some("任务等待人工确认".to_string()),
            Some(support::prompt_event_payload(&saved)),
        )
        .await;
        self.try_send_chatos_ask_user_prompt_required(&saved).await;
        Ok(saved)
    }

    #[cfg(test)]
    pub(crate) fn new(store: AppStore) -> Self {
        Self {
            store,
            config: None,
            task_queue_topology: TaskQueueTopology::inline_defaults(),
            waiters: AskUserPromptWaiters::default(),
        }
    }

    pub(crate) fn new_with_config(
        store: AppStore,
        config: AppConfig,
        task_queue_topology: TaskQueueTopology,
    ) -> Self {
        Self {
            store,
            config: Some(config),
            task_queue_topology,
            waiters: AskUserPromptWaiters::default(),
        }
    }

    pub(crate) fn signal_prompt_resolved(&self, prompt_id: &str) {
        self.waiters.wake(prompt_id);
    }

    pub async fn prune_terminal_prompts_before(
        &self,
        cutoff: &str,
        candidate_limit: usize,
    ) -> Result<AskUserPromptPruneResult, String> {
        self.store
            .prune_terminal_ask_user_prompts_before(cutoff, candidate_limit)
            .await
    }

    pub(crate) async fn publish_resolution_event_if_needed(
        &self,
        prompt: &AskUserPromptRecord,
    ) -> Result<bool, String> {
        if !prompt.resolution_event_pending {
            return Ok(false);
        }
        let run_id = prompt
            .run_id
            .as_deref()
            .ok_or_else(|| format!("resolved prompt {} has no Run id", prompt.id))?;
        let run = self.store.get_run(run_id).await?.ok_or_else(|| {
            format!(
                "resolved prompt {} references missing Run {run_id}",
                prompt.id
            )
        })?;
        crate::worker_control_queue::publish_ask_user_resolved_event(
            &self.task_queue_topology,
            prompt.id.as_str(),
            &run,
        )
        .await?;
        if let Ok(config) =
            chatos_mcp_management_sdk::McpManagementClientConfig::from_env("task-runner").await
        {
            if let Ok(client) = chatos_mcp_management_sdk::McpManagementClient::new(config) {
                if let Err(error) = client
                    .notify_waiting_user_resolved(prompt.id.as_str())
                    .await
                {
                    tracing::warn!(
                        prompt_id = prompt.id.as_str(),
                        error = %error,
                        "notify MCP Management Ask User resolution failed; resolution remains pending for reconciliation"
                    );
                    return Ok(false);
                }
            }
        }
        self.store
            .acknowledge_ask_user_resolution_event(prompt.id.as_str())
            .await?;
        Ok(true)
    }

    pub(crate) async fn publish_pending_resolution_events(
        &self,
        limit: usize,
    ) -> Result<usize, String> {
        let pending = self
            .store
            .list_pending_ask_user_resolution_events(limit)
            .await?;
        let mut published = 0usize;
        for prompt in pending {
            if self.publish_resolution_event_if_needed(&prompt).await? {
                published += 1;
            }
        }
        Ok(published)
    }
}

#[cfg(test)]
mod tests {
    use tokio::sync::broadcast;

    use super::*;
    use crate::models::SubmitAskUserPromptRequest;
    use crate::store::InMemoryStore;
    use chatos_mcp::AskUserStore;

    #[tokio::test]
    async fn execute_prompt_consumes_resolution_event_from_another_service_instance() {
        let (run_event_sender, _) = broadcast::channel(8);
        let store = AppStore::InMemory(InMemoryStore::new(run_event_sender));
        let waiting_service = AskUserPromptService::new(store.clone());
        let submitting_service = AskUserPromptService::new(store);

        let payload = AskUserPromptPayload {
            prompt_id: "prompt_cross_instance".to_string(),
            conversation_id: "task_1".to_string(),
            conversation_turn_id: "run_1".to_string(),
            tool_call_id: None,
            kind: "prompt_key_values".to_string(),
            title: "Need approval".to_string(),
            message: "continue?".to_string(),
            allow_cancel: true,
            timeout_ms: 2_000,
            payload: json!({
                "fields": [
                    {
                        "key": "answer",
                        "label": "Answer"
                    }
                ]
            }),
        };

        let handle = tokio::spawn({
            let service = waiting_service.clone();
            async move { service.execute_prompt(payload, None).await }
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        let saved = submitting_service
            .submit_prompt(
                "prompt_cross_instance",
                SubmitAskUserPromptRequest {
                    values: Some(json!({ "answer": "yes" })),
                    selection: None,
                    reason: None,
                },
            )
            .await
            .expect("submit prompt should succeed");
        assert!(saved.is_some());
        waiting_service.signal_prompt_resolved("prompt_cross_instance");

        let decision = tokio::time::timeout(Duration::from_secs(2), handle)
            .await
            .expect("execute prompt should complete without waiting for timeout")
            .expect("join should succeed")
            .expect("execute prompt should succeed");

        assert_eq!(decision.status, "submitted");
        assert_eq!(decision.response.values, Some(json!({ "answer": "yes" })));
    }

    #[tokio::test]
    async fn resolved_prompt_outbox_is_acknowledgeable() {
        let (run_event_sender, _) = broadcast::channel(8);
        let store = AppStore::InMemory(InMemoryStore::new(run_event_sender));
        let payload = AskUserPromptPayload {
            prompt_id: "prompt_outbox".to_string(),
            conversation_id: "task_1".to_string(),
            conversation_turn_id: "run_1".to_string(),
            tool_call_id: None,
            kind: "prompt_key_values".to_string(),
            title: "Need approval".to_string(),
            message: "continue?".to_string(),
            allow_cancel: true,
            timeout_ms: 2_000,
            payload: json!({}),
        };
        let mut prompt = AskUserPromptRecord::from_payload(
            payload,
            Some("task_1".to_string()),
            Some("run_1".to_string()),
            now_rfc3339(),
            None,
        );
        prompt.status = AskUserPromptStatus::Submitted;
        prompt.resolution_event_pending = true;
        store
            .save_ask_user_prompt(prompt)
            .await
            .expect("save resolved prompt");

        assert_eq!(
            store
                .list_pending_ask_user_resolution_events(10)
                .await
                .expect("list pending prompt events")
                .len(),
            1
        );
        assert!(store
            .acknowledge_ask_user_resolution_event("prompt_outbox")
            .await
            .expect("ack prompt event"));
        assert!(store
            .list_pending_ask_user_resolution_events(10)
            .await
            .expect("list acknowledged prompt events")
            .is_empty());
    }

    #[tokio::test]
    async fn run_cancellation_forces_pending_prompt_to_cancel_and_wakes_waiter() {
        let (run_event_sender, _) = broadcast::channel(8);
        let store = AppStore::InMemory(InMemoryStore::new(run_event_sender));
        let service = AskUserPromptService::new(store.clone());
        let payload = AskUserPromptPayload {
            prompt_id: "prompt_run_cancel".to_string(),
            conversation_id: "task_1".to_string(),
            conversation_turn_id: "run_1".to_string(),
            tool_call_id: None,
            kind: "prompt_key_values".to_string(),
            title: "Need approval".to_string(),
            message: "continue?".to_string(),
            allow_cancel: false,
            timeout_ms: 10_000,
            payload: json!({}),
        };
        let prompt = AskUserPromptRecord::from_payload(
            payload,
            Some("task_1".to_string()),
            Some("run_1".to_string()),
            now_rfc3339(),
            None,
        );
        store
            .save_ask_user_prompt(prompt)
            .await
            .expect("save pending prompt");

        assert_eq!(
            service
                .cancel_pending_prompts_for_run("run_1", "user stopped the run")
                .await
                .expect("cancel pending prompts"),
            1
        );
        let saved = store
            .get_ask_user_prompt("prompt_run_cancel")
            .await
            .expect("load prompt")
            .expect("prompt");
        assert_eq!(saved.status, AskUserPromptStatus::Cancelled);
        assert_eq!(
            saved.response.and_then(|response| response.reason),
            Some("user stopped the run".to_string())
        );
    }
}
