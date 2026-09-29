// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    ClientStorageError, LocalMemoryContextCacheStore, SqliteClientStorage, SqliteResultExt,
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
        .bind(serde_json::to_string(response)?)
        .bind(refreshed_at_unix_ms)
        .execute(&mut *connection)
        .await
        .db()?;
        Ok(())
    }

    async fn get_memory_context_cache(
        &self,
        cache_key: &str,
    ) -> Result<Option<Value>, ClientStorageError> {
        if cache_key.is_empty() || cache_key.len() > 65_536 {
            return Err(ClientStorageError::InvalidState(
                "Memory context cache key must contain 1..=65536 bytes".to_string(),
            ));
        }
        let mut connection = self.pool.acquire().await.db()?;
        let response = sqlx::query_scalar::<_, String>(
            "SELECT response_json FROM local_memory_context_cache WHERE cache_key = ?",
        )
        .bind(cache_key)
        .fetch_optional(&mut *connection)
        .await
        .db()?;
        response
            .map(|value| serde_json::from_str(&value).map_err(Into::into))
            .transpose()
    }
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
        assert_eq!(
            storage
                .get_memory_context_cache("scope-1")
                .await
                .expect("get")
                .expect("cached")["thread_id"],
            "conversation-1"
        );
        assert!(storage
            .get_memory_context_cache("scope-2")
            .await
            .expect("missing")
            .is_none());
    }
}
