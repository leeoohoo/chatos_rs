// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::terminal::Terminal;
use crate::repositories::db::{db_error, decode_all, decode_optional, json, timestamp, with_db};

pub async fn list_terminals_by_kind(
    user_id: Option<String>,
    kind: &str,
) -> Result<Vec<Terminal>, String> {
    with_db(|pool| Box::pin(async move { decode_all(sqlx::query_scalar("SELECT data FROM terminals WHERE ($1::text IS NULL OR user_id=$1) AND kind=$2 ORDER BY created_at DESC").bind(user_id).bind(kind).fetch_all(pool).await.map_err(db_error)?) })).await
}
pub async fn get_terminal_by_id(id: &str) -> Result<Option<Terminal>, String> {
    with_db(|pool| {
        Box::pin(async move {
            decode_optional(
                sqlx::query_scalar("SELECT data FROM terminals WHERE id=$1")
                    .bind(id)
                    .fetch_optional(pool)
                    .await
                    .map_err(db_error)?,
            )
        })
    })
    .await
}
pub async fn create_terminal(terminal: &Terminal) -> Result<String, String> {
    let mut stored = terminal.clone();
    let now = crate::core::time::now_rfc3339();
    stored.created_at = now.clone();
    stored.updated_at = now.clone();
    stored.last_active_at = now;
    with_db(|pool| Box::pin(async move { sqlx::query("INSERT INTO terminals(id,user_id,project_id,kind,status,created_at,updated_at,last_active_at,data) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)").bind(&stored.id).bind(&stored.user_id).bind(&stored.project_id).bind(&stored.kind).bind(&stored.status).bind(timestamp(&stored.created_at)?).bind(timestamp(&stored.updated_at)?).bind(timestamp(&stored.last_active_at)?).bind(json(&stored)?).execute(pool).await.map_err(db_error)?; Ok(stored.id) })).await
}
pub async fn update_terminal_status(
    id: &str,
    status: Option<String>,
    last_active_at: Option<String>,
    process_id: Option<i64>,
) -> Result<(), String> {
    let mut stored = get_terminal_by_id(id)
        .await?
        .ok_or_else(|| "terminal not found".to_string())?;
    if let Some(value) = status {
        stored.status = value;
    }
    if let Some(value) = process_id {
        stored.process_id = Some(value);
    }
    stored.updated_at = crate::core::time::now_rfc3339();
    stored.last_active_at = last_active_at.unwrap_or_else(|| stored.updated_at.clone());
    with_db(|pool|Box::pin(async move{sqlx::query("UPDATE terminals SET status=$1,updated_at=$2,last_active_at=$3,data=$4 WHERE id=$5").bind(&stored.status).bind(timestamp(&stored.updated_at)?).bind(timestamp(&stored.last_active_at)?).bind(json(&stored)?).bind(id).execute(pool).await.map(|_|()).map_err(db_error)})).await
}
pub async fn touch_terminal(id: &str) -> Result<(), String> {
    update_terminal_status(id, None, Some(crate::core::time::now_rfc3339()), None).await
}
pub async fn delete_terminal(id: &str) -> Result<(), String> {
    with_db(|pool| {
        Box::pin(async move {
            sqlx::query("DELETE FROM terminals WHERE id=$1")
                .bind(id)
                .execute(pool)
                .await
                .map(|_| ())
                .map_err(db_error)
        })
    })
    .await
}
