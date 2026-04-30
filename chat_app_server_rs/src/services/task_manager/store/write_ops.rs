#[path = "write_ops/state_rules.rs"]
mod state_rules;
#[path = "write_ops/update_ops.rs"]
mod update_ops;

use crate::services::realtime::{publish_task_board_updated, resolve_conversation_scope};
use crate::services::task_manager::types::{TaskRecord, TaskUpdatePatch};

pub async fn update_task_by_id(
    conversation_id: &str,
    task_id: &str,
    patch: TaskUpdatePatch,
) -> Result<TaskRecord, String> {
    let updated = update_ops::update_task_by_id_impl(conversation_id, task_id, patch).await?;
    publish_task_mutation_event(
        updated.conversation_id.as_str(),
        Some(updated.conversation_turn_id.as_str()),
        Some(updated.id.as_str()),
        "task_updated",
        Some(updated.clone()),
    )
    .await;
    Ok(updated)
}

pub async fn complete_task_by_id(
    conversation_id: &str,
    task_id: &str,
    patch: Option<TaskUpdatePatch>,
) -> Result<TaskRecord, String> {
    let mut patch = patch.unwrap_or_default();
    patch.status = Some("done".to_string());
    update_task_by_id(conversation_id, task_id, patch).await
}

pub async fn delete_task_by_id(conversation_id: &str, task_id: &str) -> Result<bool, String> {
    let deleted = update_ops::delete_task_by_id_impl(conversation_id, task_id).await?;
    if deleted {
        publish_task_mutation_event(conversation_id, None, Some(task_id), "task_deleted", None)
            .await;
    }
    Ok(deleted)
}

async fn publish_task_mutation_event(
    conversation_id: &str,
    conversation_turn_id: Option<&str>,
    task_id: Option<&str>,
    action: &str,
    task: Option<TaskRecord>,
) {
    let Ok(scope) = resolve_conversation_scope(conversation_id).await else {
        return;
    };
    let Some(user_id) = scope.user_id.as_deref() else {
        return;
    };
    publish_task_board_updated(
        user_id,
        conversation_id,
        conversation_turn_id,
        None,
        task_id,
        action,
        task,
        None,
        None,
    );
}
