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

#[derive(Clone)]
pub struct UiPromptService {
    store: AppStore,
    waiters: UiPromptWaiters,
}

#[derive(Clone, Default)]
struct UiPromptWaiters {
    inner: Arc<Mutex<HashMap<String, Arc<Notify>>>>,
}

impl UiPromptWaiters {
    fn register(&self, prompt_id: &str) -> Arc<Notify> {
        let mut inner = self.inner.lock();
        let notify = Arc::new(Notify::new());
        inner.insert(prompt_id.to_string(), notify.clone());
        notify
    }

    fn wake(&self, prompt_id: &str) {
        if let Some(notify) = self.inner.lock().get(prompt_id).cloned() {
            notify.notify_waiters();
        }
    }

    fn remove(&self, prompt_id: &str) {
        self.inner.lock().remove(prompt_id);
    }
}

impl UiPromptService {
    pub(crate) fn new(store: AppStore) -> Self {
        Self {
            store,
            waiters: UiPromptWaiters::default(),
        }
    }

    pub async fn list_prompts(
        &self,
        task_id: Option<&str>,
        run_id: Option<&str>,
        status: Option<UiPromptStatus>,
    ) -> Result<Vec<UiPromptRecord>, String> {
        self.store.list_ui_prompts(task_id, run_id, status).await
    }

    pub async fn get_prompt(&self, id: &str) -> Result<Option<UiPromptRecord>, String> {
        self.store.get_ui_prompt(id).await
    }

    pub async fn list_prompt_task_counts(
        &self,
        status: Option<UiPromptStatus>,
    ) -> Result<Vec<UiPromptTaskCountRecord>, String> {
        self.store.list_ui_prompt_task_counts(status).await
    }

    pub async fn list_prompts_page(
        &self,
        filters: PromptListFilters,
    ) -> Result<PaginatedResponse<UiPromptRecord>, String> {
        let filters = sanitize_prompt_list_filters(filters);
        self.store.list_ui_prompts_page(&filters).await
    }

    pub async fn submit_prompt(
        &self,
        id: &str,
        input: SubmitUiPromptRequest,
    ) -> Result<Option<UiPromptRecord>, String> {
        let Some(mut prompt) = self.store.get_ui_prompt(id).await? else {
            return Ok(None);
        };
        if prompt.status != UiPromptStatus::Pending {
            return Err(format!(
                "提示当前状态不允许提交: {}",
                status_label(prompt.status)
            ));
        }

        let response = UiPromptResponseSubmission {
            status: "submitted".to_string(),
            values: input.values,
            selection: input.selection,
            reason: normalized_optional(input.reason),
        };
        prompt.status = UiPromptStatus::Submitted;
        prompt.response = Some(response);
        prompt.updated_at = now_rfc3339();
        let saved = self.store.save_ui_prompt(prompt).await?;
        self.append_prompt_event(
            &saved,
            "ui_prompt_submitted",
            Some("已收到人工确认输入".to_string()),
            Some(prompt_event_payload(&saved)),
        )
        .await;
        self.waiters.wake(id);
        Ok(Some(saved))
    }

    pub async fn cancel_prompt(
        &self,
        id: &str,
        input: CancelUiPromptRequest,
    ) -> Result<Option<UiPromptRecord>, String> {
        let Some(mut prompt) = self.store.get_ui_prompt(id).await? else {
            return Ok(None);
        };
        if prompt.status != UiPromptStatus::Pending {
            return Err(format!(
                "提示当前状态不允许取消: {}",
                status_label(prompt.status)
            ));
        }
        if !prompt.allow_cancel {
            return Err("当前提示不允许取消".to_string());
        }

        let reason = normalized_optional(input.reason);
        prompt.status = UiPromptStatus::Cancelled;
        prompt.response = Some(UiPromptResponseSubmission {
            status: "cancelled".to_string(),
            values: None,
            selection: None,
            reason: reason.clone(),
        });
        prompt.updated_at = now_rfc3339();
        let saved = self.store.save_ui_prompt(prompt).await?;
        self.append_prompt_event(
            &saved,
            "ui_prompt_cancelled",
            Some("人工取消了提示".to_string()),
            Some(prompt_event_payload(&saved)),
        )
        .await;
        self.waiters.wake(id);
        Ok(Some(saved))
    }

    async fn append_prompt_event(
        &self,
        prompt: &UiPromptRecord,
        event_type: &str,
        message: Option<String>,
        payload: Option<Value>,
    ) {
        let Some(run_id) = prompt.run_id.as_deref() else {
            return;
        };
        if let Err(err) = self
            .store
            .append_run_event(TaskRunEventRecord::new(
                run_id.to_string(),
                event_type.to_string(),
                message,
                payload,
            ))
            .await
        {
            tracing::warn!(
                "failed to append ui prompt event for run {}: {}",
                run_id,
                err
            );
        }
    }

    async fn timeout_prompt(&self, id: &str) -> Result<UiPromptRecord, String> {
        let Some(mut prompt) = self.store.get_ui_prompt(id).await? else {
            return Err(format!("提示不存在: {id}"));
        };
        if prompt.status != UiPromptStatus::Pending {
            return Ok(prompt);
        }

        prompt.status = UiPromptStatus::TimedOut;
        prompt.response = Some(UiPromptResponseSubmission {
            status: "timed_out".to_string(),
            values: None,
            selection: None,
            reason: Some("prompt timed out".to_string()),
        });
        prompt.updated_at = now_rfc3339();
        let saved = self.store.save_ui_prompt(prompt).await?;
        self.append_prompt_event(
            &saved,
            "ui_prompt_timed_out",
            Some("人工确认提示已超时".to_string()),
            Some(prompt_event_payload(&saved)),
        )
        .await;
        Ok(saved)
    }

    async fn resolve_context_ids(
        &self,
        payload: &UiPromptPayload,
    ) -> Result<(Option<String>, Option<String>), String> {
        if let Some(run) = self.store.get_run(&payload.conversation_turn_id).await? {
            return Ok((Some(run.task_id), Some(run.id)));
        }
        let task_id = self
            .store
            .get_task(&payload.conversation_id)
            .await?
            .map(|task| task.id);
        Ok((task_id, None))
    }
}

#[async_trait]
impl UiPrompterStore for UiPromptService {
    async fn execute_prompt(
        &self,
        payload: UiPromptPayload,
        on_stream_chunk: Option<UiPromptStreamChunkCallback>,
    ) -> Result<UiPromptDecision, String> {
        let (task_id, run_id) = self.resolve_context_ids(&payload).await?;
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
        let prompt = UiPromptRecord::from_payload(payload, task_id, run_id, created_at, expires_at);
        let notify = self.waiters.register(&prompt.id);
        let timeout_ms = prompt.timeout_ms;
        let prompt_id = prompt.id.clone();
        self.store.save_ui_prompt(prompt.clone()).await?;
        self.append_prompt_event(
            &prompt,
            "ui_prompt_pending",
            Some("任务等待人工确认".to_string()),
            Some(prompt_event_payload(&prompt)),
        )
        .await;

        if let Some(callback) = on_stream_chunk {
            let title = if prompt.title.trim().is_empty() {
                prompt.kind.clone()
            } else {
                prompt.title.clone()
            };
            callback(format!("Task Runner 等待人工确认: {title} ({})", prompt.id));
        }

        let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
        loop {
            let Some(current) = self.store.get_ui_prompt(&prompt_id).await? else {
                self.waiters.remove(&prompt_id);
                return Err(format!("提示不存在: {prompt_id}"));
            };
            if current.status != UiPromptStatus::Pending {
                self.waiters.remove(&prompt_id);
                return Ok(prompt_to_decision(current));
            }

            tokio::select! {
                _ = notify.notified() => {}
                _ = tokio::time::sleep(PROMPT_STATUS_POLL_INTERVAL) => {}
                _ = tokio::time::sleep_until(deadline) => {
                    let timed_out = self.timeout_prompt(&prompt_id).await?;
                    self.waiters.remove(&prompt_id);
                    return Ok(prompt_to_decision(timed_out));
                }
            }
        }
    }
}

fn prompt_to_decision(prompt: UiPromptRecord) -> UiPromptDecision {
    let response = prompt
        .response
        .unwrap_or_else(|| UiPromptResponseSubmission {
            status: status_label(prompt.status).to_string(),
            values: None,
            selection: None,
            reason: None,
        });
    UiPromptDecision {
        status: response.status.clone(),
        response,
    }
}

fn prompt_event_payload(prompt: &UiPromptRecord) -> Value {
    json!({
        "prompt_id": prompt.id,
        "task_id": prompt.task_id,
        "run_id": prompt.run_id,
        "kind": prompt.kind,
        "title": prompt.title,
        "message": prompt.message,
        "status": status_label(prompt.status),
        "allow_cancel": prompt.allow_cancel,
        "timeout_ms": prompt.timeout_ms,
        "payload": prompt.payload,
        "response": prompt.response,
        "expires_at": prompt.expires_at,
    })
}

fn status_label(status: UiPromptStatus) -> &'static str {
    match status {
        UiPromptStatus::Pending => "pending",
        UiPromptStatus::Submitted => "submitted",
        UiPromptStatus::Cancelled => "cancelled",
        UiPromptStatus::TimedOut => "timed_out",
        UiPromptStatus::Failed => "failed",
    }
}

fn normalized_optional(value: Option<String>) -> Option<String> {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
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
