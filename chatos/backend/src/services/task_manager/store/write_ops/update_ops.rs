// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::super::get_task_by_id;
use super::state_rules::{
    apply_terminal_state_defaults, merged_task_record, validate_terminal_task_state,
};
use crate::repositories::db::{db_error, json, timestamp, with_db};
use crate::services::task_manager::normalizer::trimmed_non_empty;
use crate::services::task_manager::types::{
    normalize_task_update_patch, task_update_patch_is_empty, TaskRecord, TaskUpdatePatch,
    TASK_NOT_FOUND_ERR,
};

pub(super) async fn update_task_by_id_impl(
    conversation_id: &str,
    task_id: &str,
    patch: TaskUpdatePatch,
) -> Result<TaskRecord, String> {
    let conversation_id = trimmed_non_empty(conversation_id)
        .ok_or_else(|| "conversation_id is required".to_string())?;
    let task_id = trimmed_non_empty(task_id).ok_or_else(|| "task_id is required".to_string())?;
    let mut patch = normalize_task_update_patch(patch)?;
    if task_update_patch_is_empty(&patch) {
        return Err("at least one task field is required".to_string());
    }
    let current = get_task_by_id(conversation_id, task_id).await?;
    apply_terminal_state_defaults(&mut patch);
    let mut next = merged_task_record(current, &patch);
    validate_terminal_task_state(&next)?;
    next.updated_at = crate::core::time::now_rfc3339();
    with_db(|pool|Box::pin(async move{let result=sqlx::query("UPDATE task_manager_tasks SET status=$1,updated_at=$2,data=$3 WHERE conversation_id=$4 AND id=$5").bind(&next.status).bind(timestamp(&next.updated_at)?).bind(json(&next)?).bind(conversation_id).bind(task_id).execute(pool).await.map_err(db_error)?;if result.rows_affected()==0{return Err(TASK_NOT_FOUND_ERR.to_string())}Ok(next)})).await
}
pub(super) async fn delete_task_by_id_impl(
    conversation_id: &str,
    task_id: &str,
) -> Result<bool, String> {
    let conversation_id = trimmed_non_empty(conversation_id)
        .ok_or_else(|| "conversation_id is required".to_string())?;
    let task_id = trimmed_non_empty(task_id).ok_or_else(|| "task_id is required".to_string())?;
    with_db(|pool| {
        Box::pin(async move {
            sqlx::query("DELETE FROM task_manager_tasks WHERE conversation_id=$1 AND id=$2")
                .bind(conversation_id)
                .bind(task_id)
                .execute(pool)
                .await
                .map(|r| r.rows_affected() > 0)
                .map_err(db_error)
        })
    })
    .await
}
