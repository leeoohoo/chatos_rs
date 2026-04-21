use sqlx::SqlitePool;

use crate::models::session::Session;
use crate::services::memory_server_client;

pub async fn ensure_sqlite_session_present(
    pool: &SqlitePool,
    conversation_id: &str,
) -> Result<(), String> {
    let conversation_id = conversation_id.trim();
    if conversation_id.is_empty() {
        return Err("conversation_id is required".to_string());
    }

    let exists = sqlx::query_scalar::<_, i64>("SELECT 1 FROM sessions WHERE id = ? LIMIT 1")
        .bind(conversation_id)
        .fetch_optional(pool)
        .await
        .map_err(|err| err.to_string())?
        .is_some();

    if exists {
        return Ok(());
    }

    let session = memory_server_client::get_session_by_id(conversation_id)
        .await
        .map_err(|err| {
            format!(
                "load conversation {} from memory server failed: {}",
                conversation_id, err
            )
        })?
        .ok_or_else(|| format!("conversation {} was not found", conversation_id))?;

    upsert_sqlite_session(pool, &session).await
}

async fn upsert_sqlite_session(pool: &SqlitePool, session: &Session) -> Result<(), String> {
    let metadata_json = session
        .metadata
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|err| format!("serialize session metadata failed: {err}"))?;

    sqlx::query(
        "INSERT INTO sessions (id, title, description, metadata, user_id, project_id, status, archived_at, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET title = excluded.title, description = excluded.description, metadata = excluded.metadata, user_id = excluded.user_id, project_id = excluded.project_id, status = excluded.status, archived_at = excluded.archived_at, created_at = excluded.created_at, updated_at = excluded.updated_at",
    )
    .bind(&session.id)
    .bind(&session.title)
    .bind(&session.description)
    .bind(metadata_json)
    .bind(&session.user_id)
    .bind(&session.project_id)
    .bind(&session.status)
    .bind(&session.archived_at)
    .bind(&session.created_at)
    .bind(&session.updated_at)
    .execute(pool)
    .await
    .map_err(|err| err.to_string())?;

    Ok(())
}
