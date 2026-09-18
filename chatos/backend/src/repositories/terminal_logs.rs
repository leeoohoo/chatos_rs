// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::terminal_log::TerminalLog;
use crate::repositories::db::{db_error, decode_all, json, timestamp, with_db};

pub async fn create_terminal_log(log: &TerminalLog) -> Result<String, String> {
    with_db(|pool| {
        Box::pin(async move {
            sqlx::query(
                "INSERT INTO terminal_logs(id,terminal_id,created_at,data) VALUES($1,$2,$3,$4)",
            )
            .bind(&log.id)
            .bind(&log.terminal_id)
            .bind(timestamp(&log.created_at)?)
            .bind(json(log)?)
            .execute(pool)
            .await
            .map_err(db_error)?;
            Ok(log.id.clone())
        })
    })
    .await
}
pub async fn list_terminal_logs_recent(
    terminal_id: &str,
    limit: i64,
) -> Result<Vec<TerminalLog>, String> {
    with_db(|pool|Box::pin(async move{let mut items=decode_all(sqlx::query_scalar("SELECT data FROM terminal_logs WHERE terminal_id=$1 ORDER BY created_at DESC LIMIT $2").bind(terminal_id).bind(limit.max(1)).fetch_all(pool).await.map_err(db_error)?)?;items.reverse();Ok(items)})).await
}
pub async fn list_terminal_logs_before(
    terminal_id: &str,
    before_created_at: &str,
    limit: i64,
) -> Result<Vec<TerminalLog>, String> {
    with_db(|pool|Box::pin(async move{let mut items=decode_all(sqlx::query_scalar("SELECT data FROM terminal_logs WHERE terminal_id=$1 AND created_at<$2 ORDER BY created_at DESC LIMIT $3").bind(terminal_id).bind(timestamp(before_created_at)?).bind(limit.max(1)).fetch_all(pool).await.map_err(db_error)?)?;items.reverse();Ok(items)})).await
}
pub async fn delete_terminal_logs(terminal_id: &str) -> Result<(), String> {
    with_db(|pool| {
        Box::pin(async move {
            sqlx::query("DELETE FROM terminal_logs WHERE terminal_id=$1")
                .bind(terminal_id)
                .execute(pool)
                .await
                .map(|_| ())
                .map_err(db_error)
        })
    })
    .await
}
