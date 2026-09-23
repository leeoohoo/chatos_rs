// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::repositories::db::{db_error, decode_all, decode_optional, with_db};
use crate::services::task_manager::normalizer::trimmed_non_empty;
use crate::services::task_manager::types::TaskRecord;

pub async fn get_task_by_id(conversation_id: &str, task_id: &str) -> Result<TaskRecord, String> {
    let conversation_id = trimmed_non_empty(conversation_id)
        .ok_or_else(|| "conversation_id is required".to_string())?;
    let task_id = trimmed_non_empty(task_id).ok_or_else(|| "task_id is required".to_string())?;
    with_db(|pool| {
        Box::pin(async move {
            decode_optional(
                sqlx::query_scalar(
                    "SELECT data FROM task_manager_tasks WHERE conversation_id=$1 AND id=$2",
                )
                .bind(conversation_id)
                .bind(task_id)
                .fetch_optional(pool)
                .await
                .map_err(db_error)?,
            )?
            .ok_or_else(|| crate::services::task_manager::TASK_NOT_FOUND_ERR.to_string())
        })
    })
    .await
}
pub async fn list_tasks_for_context(
    conversation_id: &str,
    conversation_turn_id: Option<&str>,
    include_done: bool,
    limit: usize,
) -> Result<Vec<TaskRecord>, String> {
    let conversation_id = trimmed_non_empty(conversation_id)
        .ok_or_else(|| "conversation_id is required".to_string())?;
    let turn = conversation_turn_id.and_then(trimmed_non_empty);
    with_db(|pool|Box::pin(async move{decode_all(sqlx::query_scalar("SELECT data FROM task_manager_tasks WHERE conversation_id=$1 AND ($2::text IS NULL OR conversation_turn_id=$2) AND ($3 OR status<>'done') ORDER BY created_at LIMIT $4").bind(conversation_id).bind(turn).bind(include_done).bind(i64::try_from(limit.clamp(1,200)).unwrap_or(200)).fetch_all(pool).await.map_err(db_error)?)})).await
}
