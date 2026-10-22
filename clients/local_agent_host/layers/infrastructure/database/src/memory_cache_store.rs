// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    ClientStorageError, LocalMemoryContextCacheEntry, LocalMemoryContextCacheStore,
    SqliteClientStorage, SqliteResultExt,
};
use async_trait::async_trait;
use serde_json::Value;

#[async_trait]
impl LocalMemoryContextCacheStore for SqliteClientStorage {
    async fn put_memory_context_cache(
        &self,
        cache_key: &str,
        tenant_id: &str,
        source_id: &str,
        thread_id: &str,
        response: &Value,
        refreshed_at_unix_ms: i64,
    ) -> Result<(), ClientStorageError> {
        validate(cache_key, tenant_id, source_id, thread_id, response)?;
        let mut connection = self.pool.acquire().await.db()?;
        let message_high_water =
            conversation_message_high_water(&mut connection, thread_id).await?;
        let payload = serde_json::json!({
            "schema_version": 1,
            "response": response,
            "local_message_high_water": message_high_water,
        });
        sqlx::query(
            "INSERT INTO local_memory_context_cache(\
             cache_key, tenant_id, source_id, thread_id, response_json, refreshed_at_unix_ms) \
             VALUES(?, ?, ?, ?, ?, ?) ON CONFLICT(cache_key) DO UPDATE SET \
             tenant_id = excluded.tenant_id, source_id = excluded.source_id, \
             thread_id = excluded.thread_id, response_json = excluded.response_json, \
             refreshed_at_unix_ms = excluded.refreshed_at_unix_ms",
        )
        .bind(cache_key)
        .bind(tenant_id)
        .bind(source_id)
        .bind(thread_id)
        .bind(serde_json::to_string(&payload)?)
        .bind(refreshed_at_unix_ms)
        .execute(&mut *connection)
        .await
        .db()?;
        Ok(())
    }

    async fn get_memory_context_cache(
        &self,
        cache_key: &str,
    ) -> Result<Option<LocalMemoryContextCacheEntry>, ClientStorageError> {
        if cache_key.is_empty() || cache_key.len() > 65_536 {
            return Err(ClientStorageError::InvalidState(
                "Memory context cache key must contain 1..=65536 bytes".to_string(),
            ));
        }
        let mut connection = self.pool.acquire().await.db()?;
        let row = sqlx::query(
            "SELECT response_json, refreshed_at_unix_ms, thread_id FROM local_memory_context_cache \
             WHERE cache_key = ?",
        )
        .bind(cache_key)
        .fetch_optional(&mut *connection)
        .await
        .db()?;
        let Some(row) = row else { return Ok(None) };
        use sqlx::Row;
        let payload: Value =
            serde_json::from_str(&row.try_get::<String, _>("response_json").db()?)?;
        let thread_id: String = row.try_get("thread_id").db()?;
        let current_message_high_water =
            conversation_message_high_water(&mut connection, &thread_id).await?;
        let (response, cached_message_high_water) =
            if payload.get("schema_version").and_then(Value::as_u64) == Some(1) {
                (
                    payload.get("response").cloned().ok_or_else(|| {
                        ClientStorageError::InvalidState(
                            "Memory context cache envelope is missing response".to_string(),
                        )
                    })?,
                    payload
                        .get("local_message_high_water")
                        .and_then(Value::as_u64)
                        .unwrap_or(0),
                )
            } else {
                // Legacy cache rows have no trustworthy local high-water mark. Keep
                // the payload decodable but force freshness validation to reject it
                // whenever this conversation already contains any local message.
                (payload, 0)
            };
        Ok(Some(LocalMemoryContextCacheEntry {
            response,
            refreshed_at_unix_ms: row.try_get("refreshed_at_unix_ms").db()?,
            cached_message_high_water,
            current_message_high_water,
        }))
    }
}

async fn conversation_message_high_water(
    connection: &mut sqlx::SqliteConnection,
    conversation_id: &str,
) -> Result<u64, ClientStorageError> {
    let value = sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(MAX(ordinal), 0) FROM local_conversation_messages \
         WHERE conversation_id = ?",
    )
    .bind(conversation_id)
    .fetch_one(&mut *connection)
    .await
    .db()?;
    u64::try_from(value).map_err(|_| {
        ClientStorageError::InvalidState("conversation message ordinal is negative".to_string())
    })
}

fn validate(
    cache_key: &str,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    response: &Value,
) -> Result<(), ClientStorageError> {
    if cache_key.is_empty() || cache_key.len() > 65_536 {
        return Err(ClientStorageError::InvalidState(
            "Memory context cache key must contain 1..=65536 bytes".to_string(),
        ));
    }
    for (label, value) in [
        ("tenant_id", tenant_id),
        ("source_id", source_id),
        ("thread_id", thread_id),
    ] {
        if value.trim().is_empty() || value.len() > 512 {
            return Err(ClientStorageError::InvalidState(format!(
                "Memory context {label} must contain 1..=512 bytes"
            )));
        }
    }
    if serde_json::to_vec(response)?.len() > 1_048_576 {
        return Err(ClientStorageError::InvalidState(
            "Memory context cache response exceeds 1 MiB".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn cache_upserts_and_survives_exact_key_reads() {
        let storage = SqliteClientStorage::connect_memory()
            .await
            .expect("storage");
        storage
            .put_memory_context_cache(
                "scope-1",
                "user-1",
                "local_agent",
                "conversation-1",
                &json!({"thread_id": "conversation-1", "blocks": []}),
                1_000,
            )
            .await
            .expect("put");
        let cached = storage
            .get_memory_context_cache("scope-1")
            .await
            .expect("get")
            .expect("cached");
        assert_eq!(cached.response["thread_id"], "conversation-1");
        assert_eq!(cached.refreshed_at_unix_ms, 1_000);
        assert_eq!(cached.cached_message_high_water, 0);
        assert_eq!(cached.current_message_high_water, 0);
        assert!(storage
            .get_memory_context_cache("scope-2")
            .await
            .expect("missing")
            .is_none());
    }
}
