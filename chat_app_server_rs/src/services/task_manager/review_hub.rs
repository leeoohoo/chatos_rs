use std::collections::HashMap;
use std::time::Duration;

use once_cell::sync::Lazy;
use tokio::sync::{Mutex, oneshot};
use uuid::Uuid;

use crate::services::realtime::{publish_task_board_updated, resolve_conversation_scope};

use super::normalizer::{normalize_task_drafts, trimmed_non_empty};
#[cfg(test)]
use super::types::{REVIEW_NOT_FOUND_ERR, TaskReviewAction};
use super::types::{
    REVIEW_TIMEOUT_ERR, REVIEW_TIMEOUT_MS_DEFAULT, TaskCreateReviewPayload, TaskDraft,
    TaskReviewDecision,
};

#[derive(Debug)]
struct PendingReviewEntry {
    _payload: TaskCreateReviewPayload,
    _sender: oneshot::Sender<TaskReviewDecision>,
}

#[derive(Debug, Default)]
struct TaskReviewHub {
    pending: Mutex<HashMap<String, PendingReviewEntry>>,
}

impl TaskReviewHub {
    async fn register(
        &self,
        payload: TaskCreateReviewPayload,
    ) -> oneshot::Receiver<TaskReviewDecision> {
        let review_id = payload.review_id.clone();
        let (sender, receiver) = oneshot::channel();
        let mut pending = self.pending.lock().await;
        pending.insert(
            review_id,
            PendingReviewEntry {
                _payload: payload,
                _sender: sender,
            },
        );
        receiver
    }

    #[cfg(test)]
    async fn resolve(
        &self,
        review_id: &str,
        action: TaskReviewAction,
        tasks: Option<Vec<TaskDraft>>,
        reason: Option<String>,
    ) -> Result<TaskCreateReviewPayload, String> {
        let entry = {
            let mut pending = self.pending.lock().await;
            pending.remove(review_id)
        }
        .ok_or_else(|| REVIEW_NOT_FOUND_ERR.to_string())?;

        let resolved_tasks = match action {
            TaskReviewAction::Confirm => {
                let source_tasks = tasks.unwrap_or_else(|| entry._payload.draft_tasks.clone());
                let normalized = normalize_task_drafts(source_tasks)?;
                if normalized.is_empty() {
                    return Err("tasks is required for confirm action".to_string());
                }
                normalized
            }
            TaskReviewAction::Cancel => Vec::new(),
        };

        entry
            ._sender
            .send(TaskReviewDecision {
                action,
                tasks: resolved_tasks,
                reason,
            })
            .map_err(|_| "review_listener_closed".to_string())?;

        Ok(entry._payload)
    }

    async fn remove(&self, review_id: &str) {
        let mut pending = self.pending.lock().await;
        pending.remove(review_id);
    }
}

static TASK_REVIEW_HUB: Lazy<TaskReviewHub> = Lazy::new(TaskReviewHub::default);

pub async fn create_task_review(
    conversation_id: &str,
    conversation_turn_id: &str,
    draft_tasks: Vec<TaskDraft>,
    timeout_ms: u64,
) -> Result<
    (
        TaskCreateReviewPayload,
        oneshot::Receiver<TaskReviewDecision>,
    ),
    String,
> {
    let conversation_id = trimmed_non_empty(conversation_id)
        .ok_or_else(|| "conversation_id is required for task review".to_string())?
        .to_string();
    let conversation_turn_id = trimmed_non_empty(conversation_turn_id)
        .ok_or_else(|| "conversation_turn_id is required for task review".to_string())?
        .to_string();

    let draft_tasks = normalize_task_drafts(draft_tasks)?;
    if draft_tasks.is_empty() {
        return Err("at least one draft task is required".to_string());
    }

    let timeout_ms = timeout_ms.clamp(10_000, REVIEW_TIMEOUT_MS_DEFAULT);
    let payload = TaskCreateReviewPayload {
        review_id: format!("rev_{}", Uuid::new_v4().simple()),
        conversation_id,
        conversation_turn_id,
        draft_tasks,
        timeout_ms,
    };
    let publish_payload = payload.clone();
    let receiver = TASK_REVIEW_HUB.register(payload.clone()).await;
    publish_review_required_event(&publish_payload).await;
    Ok((payload, receiver))
}

pub async fn wait_for_task_review_decision(
    review_id: &str,
    receiver: oneshot::Receiver<TaskReviewDecision>,
    timeout_ms: u64,
) -> Result<TaskReviewDecision, String> {
    let bounded_timeout = timeout_ms.clamp(1_000, REVIEW_TIMEOUT_MS_DEFAULT);
    match tokio::time::timeout(Duration::from_millis(bounded_timeout), receiver).await {
        Ok(Ok(decision)) => Ok(decision),
        Ok(Err(_)) => Err("review_listener_closed".to_string()),
        Err(_) => {
            TASK_REVIEW_HUB.remove(review_id).await;
            Err(REVIEW_TIMEOUT_ERR.to_string())
        }
    }
}

#[cfg(test)]
pub async fn submit_task_review_decision(
    review_id: &str,
    action: TaskReviewAction,
    tasks: Option<Vec<TaskDraft>>,
    reason: Option<String>,
) -> Result<TaskCreateReviewPayload, String> {
    let review_id =
        trimmed_non_empty(review_id).ok_or_else(|| "review_id is required".to_string())?;
    let payload = TASK_REVIEW_HUB
        .resolve(review_id, action, tasks, reason)
        .await?;
    publish_review_resolved_event(&payload, action).await;
    Ok(payload)
}

async fn publish_review_required_event(payload: &TaskCreateReviewPayload) {
    let Ok(scope) = resolve_conversation_scope(payload.conversation_id.as_str()).await else {
        return;
    };
    let Some(user_id) = scope.user_id.as_deref() else {
        return;
    };
    publish_task_board_updated(
        user_id,
        payload.conversation_id.as_str(),
        Some(payload.conversation_turn_id.as_str()),
        Some(payload.review_id.as_str()),
        None,
        "review_required",
        None,
        Some(payload.draft_tasks.clone()),
        Some(payload.timeout_ms),
    );
}

#[cfg(test)]
async fn publish_review_resolved_event(
    payload: &TaskCreateReviewPayload,
    action: TaskReviewAction,
) {
    let Ok(scope) = resolve_conversation_scope(payload.conversation_id.as_str()).await else {
        return;
    };
    let Some(user_id) = scope.user_id.as_deref() else {
        return;
    };
    publish_task_board_updated(
        user_id,
        payload.conversation_id.as_str(),
        Some(payload.conversation_turn_id.as_str()),
        Some(payload.review_id.as_str()),
        None,
        match action {
            TaskReviewAction::Confirm => "review_confirmed",
            TaskReviewAction::Cancel => "review_cancelled",
        },
        None,
        None,
        None,
    );
}
