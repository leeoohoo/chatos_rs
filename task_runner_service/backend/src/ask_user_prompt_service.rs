use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chatos_builtin_tools::{
    AskUserDecision, AskUserPromptPayload, AskUserResponseSubmission, AskUserStore,
    AskUserStreamChunkCallback,
};
use chrono::{Duration as ChronoDuration, Utc};
use parking_lot::Mutex;
use serde_json::{json, Value};
use tokio::sync::Notify;

use crate::config::AppConfig;
use crate::models::{
    now_rfc3339, AskUserPromptRecord, AskUserPromptStatus, AskUserPromptTaskCountRecord,
    CancelAskUserPromptRequest, PaginatedResponse, PromptListFilters, SubmitAskUserPromptRequest,
    TaskRunEventRecord,
};
use crate::services::sanitize_prompt_list_filters;
use crate::store::AppStore;

const PROMPT_STATUS_POLL_INTERVAL: Duration = Duration::from_millis(250);

mod chatos_callbacks;
mod execution;
mod prompt_ops;
mod support;
mod waiters;

#[derive(Clone)]
pub struct AskUserPromptService {
    store: AppStore,
    config: Option<AppConfig>,
    waiters: AskUserPromptWaiters,
}

#[derive(Clone, Default)]
struct AskUserPromptWaiters {
    inner: Arc<Mutex<HashMap<String, Arc<Notify>>>>,
}

impl AskUserPromptService {
    #[cfg(test)]
    pub(crate) fn new(store: AppStore) -> Self {
        Self {
            store,
            config: None,
            waiters: AskUserPromptWaiters::default(),
        }
    }

    pub(crate) fn new_with_config(store: AppStore, config: AppConfig) -> Self {
        Self {
            store,
            config: Some(config),
            waiters: AskUserPromptWaiters::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use tokio::sync::broadcast;

    use super::*;
    use crate::models::SubmitAskUserPromptRequest;
    use crate::store::InMemoryStore;
    use chatos_builtin_tools::AskUserStore;

    #[tokio::test]
    async fn execute_prompt_detects_submission_from_another_service_instance() {
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

        let decision = tokio::time::timeout(Duration::from_secs(2), handle)
            .await
            .expect("execute prompt should complete without waiting for timeout")
            .expect("join should succeed")
            .expect("execute prompt should succeed");

        assert_eq!(decision.status, "submitted");
        assert_eq!(decision.response.values, Some(json!({ "answer": "yes" })));
    }
}
