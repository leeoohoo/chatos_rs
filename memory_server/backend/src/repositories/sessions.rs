use sqlx::SqlitePool;
use uuid::Uuid;

use crate::models::{CreateSessionRequest, Session, UpdateSessionRequest};

use super::now_rfc3339;

pub async fn create_session(
    pool: &SqlitePool,
    req: CreateSessionRequest,
) -> Result<Session, String> {
    let now = now_rfc3339();
    let id = Uuid::new_v4().to_string();

    sqlx::query(
        "INSERT INTO sessions (id, user_id, project_id, title, status, created_at, updated_at) VALUES (?, ?, ?, ?, 'active', ?, ?)",
    )
    .bind(&id)
    .bind(req.user_id)
    .bind(req.project_id)
    .bind(req.title)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    get_session_by_id(pool, &id)
        .await?
        .ok_or_else(|| "created session not found".to_string())
}

pub async fn upsert_session_sync(
    pool: &SqlitePool,
    session_id: &str,
    user_id: &str,
    project_id: Option<String>,
    title: Option<String>,
    status: Option<String>,
    created_at: Option<String>,
    updated_at: Option<String>,
) -> Result<Session, String> {
    let now = now_rfc3339();
    let created_at = created_at.unwrap_or_else(|| now.clone());
    let updated_at = updated_at.unwrap_or_else(|| now.clone());
    let title = title.unwrap_or_else(|| "Untitled".to_string());
    let status = status.unwrap_or_else(|| "active".to_string());
    let archived_at = if status == "archived" {
        Some(updated_at.clone())
    } else {
        None
    };

    sqlx::query(
        "INSERT INTO sessions (id, user_id, project_id, title, status, created_at, updated_at, archived_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET user_id = excluded.user_id, project_id = excluded.project_id, title = excluded.title, status = excluded.status, updated_at = excluded.updated_at, archived_at = excluded.archived_at",
    )
    .bind(session_id)
    .bind(user_id)
    .bind(project_id)
    .bind(title)
    .bind(status)
    .bind(created_at)
    .bind(updated_at)
    .bind(archived_at)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    get_session_by_id(pool, session_id)
        .await?
        .ok_or_else(|| "upserted session not found".to_string())
}

pub async fn list_sessions(
    pool: &SqlitePool,
    user_id: Option<&str>,
    project_id: Option<&str>,
    status: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<Session>, String> {
    let limit = limit.max(1).min(200);
    let offset = offset.max(0);

    let mut sql =
        "SELECT * FROM sessions WHERE 1=1".to_string();

    if user_id.is_some() {
        sql.push_str(" AND user_id = ?");
    }
    if project_id.is_some() {
        sql.push_str(" AND project_id = ?");
    }
    if status.is_some() {
        sql.push_str(" AND status = ?");
    }
    sql.push_str(" ORDER BY created_at DESC LIMIT ? OFFSET ?");

    let mut query = sqlx::query_as::<_, Session>(&sql);
    if let Some(v) = user_id {
        query = query.bind(v);
    }
    if let Some(v) = project_id {
        query = query.bind(v);
    }
    if let Some(v) = status {
        query = query.bind(v);
    }

    query
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())
}

pub async fn delete_session(pool: &SqlitePool, session_id: &str) -> Result<bool, String> {
    let now = now_rfc3339();
    let result = sqlx::query(
        "UPDATE sessions SET status = 'archived', archived_at = ?, updated_at = ? WHERE id = ? AND status != 'archived'",
    )
    .bind(&now)
    .bind(&now)
    .bind(session_id)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(result.rows_affected() > 0)
}

pub async fn get_session_by_id(pool: &SqlitePool, session_id: &str) -> Result<Option<Session>, String> {
    sqlx::query_as::<_, Session>("SELECT * FROM sessions WHERE id = ?")
        .bind(session_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())
}

pub async fn update_session(
    pool: &SqlitePool,
    session_id: &str,
    req: UpdateSessionRequest,
) -> Result<Option<Session>, String> {
    let current = get_session_by_id(pool, session_id).await?;
    let Some(current) = current else {
        return Ok(None);
    };

    let now = now_rfc3339();
    let title = req.title.or(current.title);
    let status = req.status.unwrap_or(current.status);
    let archived_at = if status == "archived" {
        Some(now.clone())
    } else {
        current.archived_at
    };

    sqlx::query("UPDATE sessions SET title = ?, status = ?, archived_at = ?, updated_at = ? WHERE id = ?")
        .bind(title)
        .bind(status)
        .bind(archived_at)
        .bind(&now)
        .bind(session_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

    get_session_by_id(pool, session_id).await
}

pub async fn list_active_user_ids(pool: &SqlitePool, limit: i64) -> Result<Vec<String>, String> {
    sqlx::query_scalar::<_, String>(
        "SELECT user_id FROM sessions WHERE status = 'active' GROUP BY user_id ORDER BY MAX(updated_at) DESC LIMIT ?",
    )
    .bind(limit.max(1).min(2000))
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())
}
