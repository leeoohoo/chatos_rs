// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::repositories::db::{db_error, decode_all, decode_optional, with_db};
use crate::services::ask_user_prompt_manager::normalizer::trimmed_non_empty;
use crate::services::ask_user_prompt_manager::types::AskUserPromptRecord;

pub async fn get_ask_user_prompt_record(
    prompt_id: &str,
) -> Result<Option<AskUserPromptRecord>, String> {
    let prompt_id =
        trimmed_non_empty(prompt_id).ok_or_else(|| "prompt_id is required".to_string())?;
    with_db(|pool| {
        Box::pin(async move {
            decode_optional(
                sqlx::query_scalar("SELECT data FROM ask_user_prompt_requests WHERE id=$1")
                    .bind(prompt_id)
                    .fetch_optional(pool)
                    .await
                    .map_err(db_error)?,
            )
        })
    })
    .await
}
pub async fn list_ask_user_prompt_history_records(
    conversation_id: &str,
    limit: usize,
    include_pending: bool,
) -> Result<Vec<AskUserPromptRecord>, String> {
    let conversation_id = trimmed_non_empty(conversation_id)
        .ok_or_else(|| "conversation_id is required".to_string())?;
    with_db(|pool|Box::pin(async move{decode_all(sqlx::query_scalar("SELECT data FROM ask_user_prompt_requests WHERE conversation_id=$1 AND ($2 OR status<>'pending') ORDER BY updated_at DESC,created_at DESC LIMIT $3").bind(conversation_id).bind(include_pending).bind(i64::try_from(limit.clamp(1,500)).unwrap_or(500)).fetch_all(pool).await.map_err(db_error)?)})).await
}
