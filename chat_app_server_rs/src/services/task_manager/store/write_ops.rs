#[path = "write_ops/state_rules.rs"]
mod state_rules;
#[path = "write_ops/update_ops.rs"]
mod update_ops;

use crate::services::task_manager::types::{TaskRecord, TaskUpdatePatch};

pub async fn update_task_by_id(
    conversation_id: &str,
    task_id: &str,
    patch: TaskUpdatePatch,
) -> Result<TaskRecord, String> {
    update_ops::update_task_by_id_impl(conversation_id, task_id, patch).await
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
    update_ops::delete_task_by_id_impl(conversation_id, task_id).await
}
