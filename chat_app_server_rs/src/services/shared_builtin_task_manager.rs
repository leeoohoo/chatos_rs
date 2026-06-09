use async_trait::async_trait;
use serde::Serialize;
use serde_json::{json, Value};

use chatos_builtin_tools::{
    TaskDraft as SharedTaskDraft, TaskManagerStore, TaskOutcomeItem as SharedTaskOutcomeItem,
    TaskStreamChunkCallback, TaskUpdatePatch as SharedTaskUpdatePatch,
};

use crate::modules::conversation_runtime::task_board::refresh_task_board_runtime_outcome;
use crate::services::task_manager::{
    complete_task_by_id, create_task_review, create_tasks_for_turn, delete_task_by_id,
    list_tasks_for_context, update_task_by_id, wait_for_task_review_decision,
    TaskCreateReviewPayload, TaskDraft, TaskOutcomeItem, TaskReviewAction, TaskUpdatePatch,
    REVIEW_TIMEOUT_ERR,
};
use crate::utils::events::Events;

#[derive(Debug, Clone, Default)]
pub struct ChatosTaskManagerStore;

#[async_trait]
impl TaskManagerStore for ChatosTaskManagerStore {
    async fn create_tasks_for_turn(
        &self,
        conversation_id: &str,
        conversation_turn_id: &str,
        draft_tasks: Vec<SharedTaskDraft>,
    ) -> Result<Vec<Value>, String> {
        let tasks = create_tasks_for_turn(
            conversation_id,
            conversation_turn_id,
            shared_drafts_into_chatos(draft_tasks)?,
        )
        .await?;
        records_to_values(tasks)
    }

    async fn review_and_create_tasks(
        &self,
        conversation_id: &str,
        conversation_turn_id: &str,
        draft_tasks: Vec<SharedTaskDraft>,
        timeout_ms: u64,
        on_stream_chunk: Option<TaskStreamChunkCallback>,
    ) -> Result<Value, String> {
        let (review_payload, receiver) = create_task_review(
            conversation_id,
            conversation_turn_id,
            shared_drafts_into_chatos(draft_tasks)?,
            timeout_ms,
        )
        .await?;

        emit_review_required_event(on_stream_chunk.as_ref(), &review_payload);

        let decision = match wait_for_task_review_decision(
            review_payload.review_id.as_str(),
            receiver,
            review_payload.timeout_ms,
        )
        .await
        {
            Ok(value) => value,
            Err(err) if err == REVIEW_TIMEOUT_ERR => {
                return Ok(json!({
                    "confirmed": false,
                    "cancelled": true,
                    "reason": "review_timeout",
                }));
            }
            Err(err) => return Err(err),
        };

        match decision.action {
            TaskReviewAction::Confirm => {
                let tasks =
                    create_tasks_for_turn(conversation_id, conversation_turn_id, decision.tasks)
                        .await?;
                emit_task_board_updated_event(
                    conversation_id,
                    conversation_turn_id,
                    on_stream_chunk,
                )
                .await;
                Ok(json!({
                    "confirmed": true,
                    "cancelled": false,
                    "created_count": tasks.len(),
                    "tasks": records_to_values(tasks)?,
                    "conversation_id": conversation_id,
                    "conversation_turn_id": conversation_turn_id,
                }))
            }
            TaskReviewAction::Cancel => Ok(json!({
                "confirmed": false,
                "cancelled": true,
                "reason": decision.reason.unwrap_or_else(|| "user_cancelled".to_string()),
            })),
        }
    }

    async fn list_tasks_for_context(
        &self,
        conversation_id: &str,
        conversation_turn_id: Option<&str>,
        include_done: bool,
        limit: usize,
    ) -> Result<Vec<Value>, String> {
        let tasks =
            list_tasks_for_context(conversation_id, conversation_turn_id, include_done, limit)
                .await?;
        records_to_values(tasks)
    }

    async fn update_task_by_id(
        &self,
        conversation_id: &str,
        task_id: &str,
        patch: SharedTaskUpdatePatch,
    ) -> Result<Value, String> {
        let task =
            update_task_by_id(conversation_id, task_id, shared_patch_into_chatos(patch)?).await?;
        serde_json::to_value(task).map_err(|err| err.to_string())
    }

    async fn complete_task_by_id(
        &self,
        conversation_id: &str,
        task_id: &str,
        patch: Option<SharedTaskUpdatePatch>,
    ) -> Result<Value, String> {
        let patch = patch.map(shared_patch_into_chatos).transpose()?;
        let task = complete_task_by_id(conversation_id, task_id, patch).await?;
        serde_json::to_value(task).map_err(|err| err.to_string())
    }

    async fn delete_task_by_id(
        &self,
        conversation_id: &str,
        task_id: &str,
    ) -> Result<bool, String> {
        delete_task_by_id(conversation_id, task_id).await
    }

    async fn task_board_updated_event(
        &self,
        conversation_id: &str,
        conversation_turn_id: &str,
    ) -> Option<Value> {
        refresh_task_board_runtime_outcome(conversation_id, conversation_turn_id)
            .await
            .map(|outcome| outcome.updated_event)
    }
}

fn shared_drafts_into_chatos(drafts: Vec<SharedTaskDraft>) -> Result<Vec<TaskDraft>, String> {
    drafts.into_iter().map(shared_draft_into_chatos).collect()
}

fn shared_draft_into_chatos(draft: SharedTaskDraft) -> Result<TaskDraft, String> {
    Ok(TaskDraft {
        title: draft.title,
        details: draft.details,
        priority: draft.priority,
        status: draft.status,
        tags: draft.tags,
        due_at: draft.due_at,
        outcome_summary: draft.outcome_summary,
        outcome_items: shared_outcome_items_into_chatos(draft.outcome_items),
        resume_hint: draft.resume_hint,
        blocker_reason: draft.blocker_reason,
        blocker_needs: draft.blocker_needs,
        blocker_kind: draft.blocker_kind,
    })
}

fn shared_patch_into_chatos(patch: SharedTaskUpdatePatch) -> Result<TaskUpdatePatch, String> {
    Ok(TaskUpdatePatch {
        title: patch.title,
        details: patch.details,
        priority: patch.priority,
        status: patch.status,
        tags: patch.tags,
        due_at: patch.due_at,
        outcome_summary: patch.outcome_summary,
        outcome_items: patch.outcome_items.map(shared_outcome_items_into_chatos),
        resume_hint: patch.resume_hint,
        blocker_reason: patch.blocker_reason,
        blocker_needs: patch.blocker_needs,
        blocker_kind: patch.blocker_kind,
        completed_at: patch.completed_at,
        last_outcome_at: patch.last_outcome_at,
    })
}

fn shared_outcome_items_into_chatos(items: Vec<SharedTaskOutcomeItem>) -> Vec<TaskOutcomeItem> {
    items
        .into_iter()
        .map(|item| TaskOutcomeItem {
            kind: item.kind,
            text: item.text,
            importance: item.importance,
            refs: item.refs,
        })
        .collect()
}

fn records_to_values<T>(records: Vec<T>) -> Result<Vec<Value>, String>
where
    T: Serialize,
{
    records
        .into_iter()
        .map(|record| serde_json::to_value(record).map_err(|err| err.to_string()))
        .collect()
}

fn emit_review_required_event(
    on_stream_chunk: Option<&TaskStreamChunkCallback>,
    payload: &TaskCreateReviewPayload,
) {
    let Some(callback) = on_stream_chunk else {
        return;
    };
    let event_payload = json!({
        "event": Events::TASK_CREATE_REVIEW_REQUIRED,
        "data": payload,
    });
    if let Ok(serialized) = serde_json::to_string(&event_payload) {
        callback(serialized);
    }
}

async fn emit_task_board_updated_event(
    conversation_id: &str,
    conversation_turn_id: &str,
    on_stream_chunk: Option<TaskStreamChunkCallback>,
) {
    let Some(callback) = on_stream_chunk else {
        return;
    };
    let Some(outcome) =
        refresh_task_board_runtime_outcome(conversation_id, conversation_turn_id).await
    else {
        return;
    };
    if let Ok(serialized) = serde_json::to_string(&outcome.updated_event) {
        callback(serialized);
    }
}
