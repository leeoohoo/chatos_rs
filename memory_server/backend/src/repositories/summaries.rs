use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::models::{CreateSummaryInput, SessionSummary};

use super::now_rfc3339;

pub async fn create_summary(
    pool: &SqlitePool,
    input: CreateSummaryInput,
) -> Result<SessionSummary, String> {
    let id = Uuid::new_v4().to_string();
    let now = now_rfc3339();

    sqlx::query(
        "INSERT INTO session_summaries_v2 (id, session_id, summary_text, summary_model, trigger_type, source_start_message_id, source_end_message_id, source_message_count, source_estimated_tokens, status, error_message, level, rollup_status, rollup_summary_id, rolled_up_at, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending', NULL, NULL, ?, ?)",
    )
    .bind(&id)
    .bind(input.session_id)
    .bind(input.summary_text)
    .bind(input.summary_model)
    .bind(input.trigger_type)
    .bind(input.source_start_message_id)
    .bind(input.source_end_message_id)
    .bind(input.source_message_count)
    .bind(input.source_estimated_tokens)
    .bind(input.status)
    .bind(input.error_message)
    .bind(input.level)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    get_summary_by_id(pool, &id)
        .await?
        .ok_or_else(|| "created summary not found".to_string())
}

pub async fn get_summary_by_id(pool: &SqlitePool, summary_id: &str) -> Result<Option<SessionSummary>, String> {
    sqlx::query_as::<_, SessionSummary>("SELECT * FROM session_summaries_v2 WHERE id = ?")
        .bind(summary_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())
}

pub async fn list_summaries(
    pool: &SqlitePool,
    session_id: &str,
    level: Option<i64>,
    status: Option<&str>,
    rollup_status: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<SessionSummary>, String> {
    let mut sql = "SELECT * FROM session_summaries_v2 WHERE session_id = ?".to_string();
    if level.is_some() {
        sql.push_str(" AND level = ?");
    }
    if status.is_some() {
        sql.push_str(" AND status = ?");
    }
    if rollup_status.is_some() {
        sql.push_str(" AND rollup_status = ?");
    }
    sql.push_str(" ORDER BY created_at DESC LIMIT ? OFFSET ?");

    let mut q = sqlx::query_as::<_, SessionSummary>(&sql).bind(session_id);
    if let Some(v) = level {
        q = q.bind(v);
    }
    if let Some(v) = status {
        q = q.bind(v);
    }
    if let Some(v) = rollup_status {
        q = q.bind(v);
    }

    q.bind(limit.max(1).min(500))
        .bind(offset.max(0))
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())
}

pub async fn list_all_summaries_by_session(
    pool: &SqlitePool,
    session_id: &str,
) -> Result<Vec<SessionSummary>, String> {
    sqlx::query_as::<_, SessionSummary>(
        "SELECT * FROM session_summaries_v2 WHERE session_id = ? ORDER BY created_at ASC",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())
}

pub async fn list_summary_level_stats(
    pool: &SqlitePool,
    session_id: &str,
) -> Result<Vec<(i64, i64, i64)>, String> {
    let rows = sqlx::query(
        "SELECT level, COUNT(*) as total, SUM(CASE WHEN rollup_status = 'pending' THEN 1 ELSE 0 END) as pending_count FROM session_summaries_v2 WHERE session_id = ? GROUP BY level ORDER BY level ASC",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    let mut out = Vec::new();
    for row in rows {
        let level: i64 = row.try_get("level").unwrap_or(0);
        let total: i64 = row.try_get("total").unwrap_or(0);
        let pending: i64 = row.try_get("pending_count").unwrap_or(0);
        out.push((level, total, pending));
    }
    Ok(out)
}

pub async fn list_done_pending_rollup_summaries_by_level_no_limit(
    pool: &SqlitePool,
    session_id: &str,
    level: i64,
) -> Result<Vec<SessionSummary>, String> {
    sqlx::query_as::<_, SessionSummary>(
        "SELECT * FROM session_summaries_v2 WHERE session_id = ? AND level = ? AND status = 'done' AND rollup_status = 'pending' ORDER BY created_at ASC",
    )
    .bind(session_id)
    .bind(level)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())
}

pub async fn list_session_ids_with_pending_rollup_by_user(
    pool: &SqlitePool,
    user_id: &str,
    max_level: i64,
    limit: i64,
) -> Result<Vec<String>, String> {
    sqlx::query_scalar::<_, String>(
        "SELECT ss.session_id FROM session_summaries_v2 ss JOIN sessions s ON s.id = ss.session_id WHERE ss.status = 'done' AND ss.rollup_status = 'pending' AND ss.level <= ? AND s.user_id = ? AND s.status = 'active' GROUP BY ss.session_id ORDER BY MIN(ss.created_at) ASC LIMIT ?",
    )
    .bind(max_level.max(0))
    .bind(user_id)
    .bind(limit.max(1).min(5000))
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())
}

pub async fn mark_summaries_rolled_up(
    pool: &SqlitePool,
    summary_ids: &[String],
    rollup_summary_id: &str,
) -> Result<usize, String> {
    if summary_ids.is_empty() {
        return Ok(0);
    }

    let now = now_rfc3339();
    let placeholders = vec!["?"; summary_ids.len()].join(", ");
    let sql = format!(
        "UPDATE session_summaries_v2 SET rollup_status = 'summarized', rollup_summary_id = ?, rolled_up_at = ?, updated_at = ? WHERE id IN ({}) AND rollup_status = 'pending'",
        placeholders
    );

    let mut q = sqlx::query(&sql)
        .bind(rollup_summary_id)
        .bind(&now)
        .bind(&now);
    for id in summary_ids {
        q = q.bind(id);
    }

    let result = q.execute(pool).await.map_err(|e| e.to_string())?;
    Ok(result.rows_affected() as usize)
}

pub async fn delete_summary(
    pool: &SqlitePool,
    session_id: &str,
    summary_id: &str,
) -> Result<bool, String> {
    let result = sqlx::query("DELETE FROM session_summaries_v2 WHERE session_id = ? AND id = ?")
        .bind(session_id)
        .bind(summary_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(result.rows_affected() > 0)
}
