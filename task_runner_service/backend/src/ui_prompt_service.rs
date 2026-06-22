use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chatos_builtin_tools::{
    UiPromptDecision, UiPromptPayload, UiPromptResponseSubmission, UiPromptStreamChunkCallback,
    UiPrompterStore,
};
use chrono::{Duration as ChronoDuration, Utc};
use parking_lot::Mutex;
use serde_json::{json, Value};
use tokio::sync::Notify;

use crate::models::{
    now_rfc3339, CancelUiPromptRequest, PaginatedResponse, PromptListFilters,
    SubmitUiPromptRequest, TaskRunEventRecord, UiPromptRecord, UiPromptStatus,
    UiPromptTaskCountRecord,
};
use crate::services::sanitize_prompt_list_filters;
use crate::store::AppStore;

const PROMPT_STATUS_POLL_INTERVAL: Duration = Duration::from_millis(250);

mod execution;
mod prompt_ops;
mod support;
mod waiters;

#[derive(Clone)]
pub struct UiPromptService {
    store: AppStore,
    waiters: UiPromptWaiters,
}

#[derive(Clone, Default)]
struct UiPromptWaiters {
    inner: Arc<Mutex<HashMap<String, Arc<Notify>>>>,
}

impl UiPromptService {
    pub(crate) fn new(store: AppStore) -> Self {
        Self {
            store,
            waiters: UiPromptWaiters::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use tokio::sync::broadcast;

    use super::*;
    use crate::models::SubmitUiPromptRequest;
    use crate::store::InMemoryStore;
    use chatos_builtin_tools::UiPrompterStore;

    #[tokio::test]
    async fn execute_prompt_detects_submission_from_another_service_instance() {
        let (run_event_sender, _) = broadcast::channel(8);
        let store = AppStore::InMemory(InMemoryStore::new(run_event_sender));
        let waiting_service = UiPromptService::new(store.clone());
        let submitting_service = UiPromptService::new(store);

        let payload = UiPromptPayload {
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
                SubmitUiPromptRequest {
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
