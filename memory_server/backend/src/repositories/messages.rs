use sqlx::SqlitePool;
use uuid::Uuid;

use crate::models::{CreateMessageRequest, Message, MessageRow};

use super::now_rfc3339;

pub async fn create_message(
    pool: &SqlitePool,
    session_id: &str,
    req: CreateMessageRequest,
) -> Result<Message, String> {
    let id = Uuid::new_v4().to_string();
    let now = now_rfc3339();

    sqlx::query(
        "INSERT INTO messages (id, session_id, role, content, message_mode, message_source, tool_calls, tool_call_id, reasoning, metadata, summary_status, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending', ?)",
    )
    .bind(&id)
    .bind(session_id)
    .bind(req.role)
    .bind(req.content)
    .bind(req.message_mode)
    .bind(req.message_source)
    .bind(req.tool_calls.map(|v| v.to_string()))
    .bind(req.tool_call_id)
    .bind(req.reasoning)
    .bind(req.metadata.map(|v| v.to_string()))
    .bind(&now)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    get_message_by_id(pool, &id)
        .await?
        .ok_or_else(|| "created message not found".to_string())
}

#[derive(Debug, Clone)]
pub struct SyncMessageInput {
    pub message_id: String,
    pub role: String,
    pub content: String,
    pub message_mode: Option<String>,
    pub message_source: Option<String>,
    pub tool_calls_json: Option<String>,
    pub tool_call_id: Option<String>,
    pub reasoning: Option<String>,
    pub metadata_json: Option<String>,
    pub created_at: String,
}

pub async fn upsert_message_sync(
    pool: &SqlitePool,
    session_id: &str,
    input: SyncMessageInput,
) -> Result<Message, String> {
    sqlx::query(
        "INSERT INTO messages (id, session_id, role, content, message_mode, message_source, tool_calls, tool_call_id, reasoning, metadata, summary_status, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending', ?) ON CONFLICT(id) DO UPDATE SET session_id = excluded.session_id, role = excluded.role, content = excluded.content, message_mode = excluded.message_mode, message_source = excluded.message_source, tool_calls = excluded.tool_calls, tool_call_id = excluded.tool_call_id, reasoning = excluded.reasoning, metadata = excluded.metadata, created_at = excluded.created_at",
    )
    .bind(&input.message_id)
    .bind(session_id)
    .bind(input.role)
    .bind(input.content)
    .bind(input.message_mode)
    .bind(input.message_source)
    .bind(input.tool_calls_json)
    .bind(input.tool_call_id)
    .bind(input.reasoning)
    .bind(input.metadata_json)
    .bind(input.created_at)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    get_message_by_id(pool, input.message_id.as_str())
        .await?
        .ok_or_else(|| "upserted message not found".to_string())
}

pub async fn batch_create_messages(
    pool: &SqlitePool,
    session_id: &str,
    requests: Vec<CreateMessageRequest>,
) -> Result<Vec<Message>, String> {
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    let mut created_ids = Vec::with_capacity(requests.len());
    for req in requests {
        let id = Uuid::new_v4().to_string();
        let now = now_rfc3339();
        sqlx::query(
            "INSERT INTO messages (id, session_id, role, content, message_mode, message_source, tool_calls, tool_call_id, reasoning, metadata, summary_status, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending', ?)",
        )
        .bind(&id)
        .bind(session_id)
        .bind(req.role)
        .bind(req.content)
        .bind(req.message_mode)
        .bind(req.message_source)
        .bind(req.tool_calls.map(|v| v.to_string()))
        .bind(req.tool_call_id)
        .bind(req.reasoning)
        .bind(req.metadata.map(|v| v.to_string()))
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
        created_ids.push(id);
    }
    tx.commit().await.map_err(|e| e.to_string())?;

    let mut out = Vec::new();
    for id in created_ids {
        if let Some(item) = get_message_by_id(pool, id.as_str()).await? {
            out.push(item);
        }
    }
    Ok(out)
}

pub async fn get_message_by_id(pool: &SqlitePool, message_id: &str) -> Result<Option<Message>, String> {
    let row = sqlx::query_as::<_, MessageRow>("SELECT * FROM messages WHERE id = ?")
        .bind(message_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?;

    Ok(row.map(Into::into))
}

pub async fn delete_message_by_id(pool: &SqlitePool, message_id: &str) -> Result<bool, String> {
    let result = sqlx::query("DELETE FROM messages WHERE id = ?")
        .bind(message_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(result.rows_affected() > 0)
}

pub async fn delete_messages_by_session(
    pool: &SqlitePool,
    session_id: &str,
) -> Result<i64, String> {
    let result = sqlx::query("DELETE FROM messages WHERE session_id = ?")
        .bind(session_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(result.rows_affected() as i64)
}

pub async fn list_messages_by_session(
    pool: &SqlitePool,
    session_id: &str,
    limit: i64,
    offset: i64,
    asc: bool,
) -> Result<Vec<Message>, String> {
    let order = if asc { "ASC" } else { "DESC" };
    let sql = format!(
        "SELECT * FROM messages WHERE session_id = ? ORDER BY created_at {} LIMIT ? OFFSET ?",
        order
    );

    let rows = sqlx::query_as::<_, MessageRow>(&sql)
        .bind(session_id)
        .bind(limit.max(1).min(2000))
        .bind(offset.max(0))
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;

    Ok(rows.into_iter().map(Into::into).collect())
}

pub async fn list_pending_messages(
    pool: &SqlitePool,
    session_id: &str,
    limit: Option<i64>,
) -> Result<Vec<Message>, String> {
    let rows = if let Some(v) = limit {
        sqlx::query_as::<_, MessageRow>(
            "SELECT * FROM messages WHERE session_id = ? AND summary_status = 'pending' ORDER BY created_at ASC LIMIT ?",
        )
        .bind(session_id)
        .bind(v.max(1))
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?
    } else {
        sqlx::query_as::<_, MessageRow>(
            "SELECT * FROM messages WHERE session_id = ? AND summary_status = 'pending' ORDER BY created_at ASC",
        )
        .bind(session_id)
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?
    };

    Ok(rows.into_iter().map(Into::into).collect())
}

pub async fn mark_messages_summarized(
    pool: &SqlitePool,
    session_id: &str,
    message_ids: &[String],
    summary_id: &str,
) -> Result<usize, String> {
    if message_ids.is_empty() {
        return Ok(0);
    }

    let now = now_rfc3339();
    let placeholders = vec!["?"; message_ids.len()].join(", ");
    let sql = format!(
        "UPDATE messages SET summary_status = 'summarized', summary_id = ?, summarized_at = ? WHERE session_id = ? AND id IN ({})",
        placeholders
    );

    let mut q = sqlx::query(&sql)
        .bind(summary_id)
        .bind(&now)
        .bind(session_id);
    for id in message_ids {
        q = q.bind(id);
    }

    let result = q.execute(pool).await.map_err(|e| e.to_string())?;
    Ok(result.rows_affected() as usize)
}

pub async fn list_session_ids_with_pending_messages_by_user(
    pool: &SqlitePool,
    user_id: &str,
    limit: i64,
) -> Result<Vec<String>, String> {
    sqlx::query_scalar::<_, String>(
        "SELECT m.session_id FROM messages m JOIN sessions s ON s.id = m.session_id WHERE m.summary_status = 'pending' AND s.status = 'active' AND s.user_id = ? GROUP BY m.session_id ORDER BY MIN(m.created_at) ASC LIMIT ?",
    )
    .bind(user_id)
    .bind(limit.max(1).min(5000))
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())
}
