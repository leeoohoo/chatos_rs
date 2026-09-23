// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::session_runtime_settings::SessionRuntimeSettings;
use crate::repositories::db::{db_error, decode_optional, json, timestamp, with_db};

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
}
pub async fn get_session_runtime_settings(
    session_id: &str,
    user_id: &str,
) -> Result<Option<SessionRuntimeSettings>, String> {
    with_db(|pool| {
        Box::pin(async move {
            decode_optional(
                sqlx::query_scalar(
                    "SELECT data FROM session_runtime_settings WHERE session_id=$1 AND user_id=$2",
                )
                .bind(session_id)
                .bind(user_id)
                .fetch_optional(pool)
                .await
                .map_err(db_error)?,
            )
        })
    })
    .await
}
pub async fn purge_removed_task_settings() -> Result<u64, String> {
    with_db(|pool|Box::pin(async move{sqlx::query("UPDATE session_runtime_settings SET data=data-'auto_create_task',updated_at=now() WHERE data ? 'auto_create_task'").execute(pool).await.map(|result|result.rows_affected()).map_err(db_error)})).await
}
pub async fn upsert_session_runtime_settings(
    settings: &SessionRuntimeSettings,
) -> Result<SessionRuntimeSettings, String> {
    let now = crate::core::time::now_rfc3339();
    let mut next = settings.clone();
    next.selected_model_id = normalize_optional_text(next.selected_model_id);
    next.selected_model_name = normalize_optional_text(next.selected_model_name);
    next.selected_thinking_level = normalize_optional_text(next.selected_thinking_level);
    next.remote_connection_id = normalize_optional_text(next.remote_connection_id);
    next.workspace_root = normalize_optional_text(next.workspace_root);
    if next.created_at.trim().is_empty() {
        next.created_at = now.clone();
    }
    next.updated_at = now;
    let stored = next.clone();
    with_db(|pool|Box::pin(async move{sqlx::query("INSERT INTO session_runtime_settings(session_id,user_id,updated_at,data) VALUES($1,$2,$3,$4) ON CONFLICT(session_id) DO UPDATE SET user_id=EXCLUDED.user_id,updated_at=EXCLUDED.updated_at,data=EXCLUDED.data").bind(&stored.session_id).bind(&stored.user_id).bind(timestamp(&stored.updated_at)?).bind(json(&stored)?).execute(pool).await.map(|_|()).map_err(db_error)})).await?;
    Ok(next)
}
