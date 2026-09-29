// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    ClientStorageError, IdempotentCommand, LocalModelConfigSnapshot, LocalModelConfigSnapshotStore,
    SqliteClientStorage, SqliteResultExt,
};
use async_trait::async_trait;
use sqlx::Row;

const SNAPSHOT_SELECT: &str =
    "SELECT model_config_ref, model_config_revision, credential_ref, base_url, model, provider, \
     supports_responses, supports_images, instructions, temperature, max_output_tokens, \
     thinking_level, include_prompt_cache_retention, request_body_limit_bytes, \
     max_transient_retries, output_format_json FROM local_model_config_snapshots \
     WHERE model_config_ref = ? AND model_config_revision = ?";

#[async_trait]
impl LocalModelConfigSnapshotStore for SqliteClientStorage {
    async fn put_model_config_snapshot(
        &self,
        command: &IdempotentCommand,
        snapshot: &LocalModelConfigSnapshot,
        now_unix_ms: i64,
    ) -> Result<LocalModelConfigSnapshot, ClientStorageError> {
        snapshot
            .validate()
            .map_err(ClientStorageError::InvalidState)?;
        if now_unix_ms < 0 {
            return Err(ClientStorageError::InvalidState(
                "model config snapshot timestamp must not be negative".to_string(),
            ));
        }
        let mut connection = self.pool.acquire().await.db()?;
        Self::begin_immediate(&mut connection).await?;
        let result = async {
            if let Some(replay) = Self::replay(&mut connection, command).await? {
                return Ok(replay);
            }
            if let Some(current) = fetch_snapshot(
                &mut connection,
                &snapshot.model_config_ref,
                &snapshot.model_config_revision,
            )
            .await?
            {
                if current == *snapshot {
                    Self::record_receipt(&mut connection, command, &current, now_unix_ms).await?;
                    return Ok(current);
                }
                return Err(ClientStorageError::Conflict(format!(
                    "model config revision already contains different content: {}@{}",
                    snapshot.model_config_ref, snapshot.model_config_revision
                )));
            }
            insert_snapshot(&mut connection, snapshot, now_unix_ms).await?;
            Self::record_receipt(&mut connection, command, snapshot, now_unix_ms).await?;
            Ok(snapshot.clone())
        }
        .await;
        Self::finish_write(&mut connection, result).await
    }

    async fn get_model_config_snapshot(
        &self,
        model_config_ref: &str,
        model_config_revision: &str,
    ) -> Result<Option<LocalModelConfigSnapshot>, ClientStorageError> {
        let mut connection = self.pool.acquire().await.db()?;
        fetch_snapshot(&mut connection, model_config_ref, model_config_revision).await
    }
}

async fn insert_snapshot(
    connection: &mut sqlx::SqliteConnection,
    snapshot: &LocalModelConfigSnapshot,
    now_unix_ms: i64,
) -> Result<(), ClientStorageError> {
    sqlx::query(
        "INSERT INTO local_model_config_snapshots(\
         model_config_ref, model_config_revision, credential_ref, base_url, model, provider, \
         supports_responses, supports_images, instructions, temperature, max_output_tokens, \
         thinking_level, include_prompt_cache_retention, request_body_limit_bytes, \
         max_transient_retries, output_format_json, created_at_unix_ms) \
         VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&snapshot.model_config_ref)
    .bind(&snapshot.model_config_revision)
    .bind(&snapshot.credential_ref)
    .bind(&snapshot.base_url)
    .bind(&snapshot.model)
    .bind(&snapshot.provider)
    .bind(i64::from(snapshot.supports_responses))
    .bind(snapshot.supports_images.map(i64::from))
    .bind(&snapshot.instructions)
    .bind(snapshot.temperature)
    .bind(snapshot.max_output_tokens)
    .bind(&snapshot.thinking_level)
    .bind(i64::from(snapshot.include_prompt_cache_retention))
    .bind(snapshot.request_body_limit_bytes.map(|value| value as i64))
    .bind(snapshot.max_transient_retries.map(i64::from))
    .bind(
        snapshot
            .output_format
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?,
    )
    .bind(now_unix_ms)
    .execute(&mut *connection)
    .await
    .db()?;
    Ok(())
}

async fn fetch_snapshot(
    connection: &mut sqlx::SqliteConnection,
    model_config_ref: &str,
    model_config_revision: &str,
) -> Result<Option<LocalModelConfigSnapshot>, ClientStorageError> {
    let row = sqlx::query(SNAPSHOT_SELECT)
        .bind(model_config_ref)
        .bind(model_config_revision)
        .fetch_optional(&mut *connection)
        .await
        .db()?;
    row.map(|row| decode_snapshot(&row)).transpose()
}

fn decode_snapshot(
    row: &sqlx::sqlite::SqliteRow,
) -> Result<LocalModelConfigSnapshot, ClientStorageError> {
    let supports_responses: i64 = row.try_get("supports_responses").db()?;
    let supports_images: Option<i64> = row.try_get("supports_images").db()?;
    let retention: i64 = row.try_get("include_prompt_cache_retention").db()?;
    let request_limit: Option<i64> = row.try_get("request_body_limit_bytes").db()?;
    let retries: Option<i64> = row.try_get("max_transient_retries").db()?;
    let output_format: Option<String> = row.try_get("output_format_json").db()?;
    let snapshot = LocalModelConfigSnapshot {
        model_config_ref: row.try_get("model_config_ref").db()?,
        model_config_revision: row.try_get("model_config_revision").db()?,
        credential_ref: row.try_get("credential_ref").db()?,
        base_url: row.try_get("base_url").db()?,
        model: row.try_get("model").db()?,
        provider: row.try_get("provider").db()?,
        supports_responses: decode_bool("supports_responses", supports_responses)?,
        supports_images: supports_images
            .map(|value| decode_bool("supports_images", value))
            .transpose()?,
        instructions: row.try_get("instructions").db()?,
        temperature: row.try_get("temperature").db()?,
        max_output_tokens: row.try_get("max_output_tokens").db()?,
        thinking_level: row.try_get("thinking_level").db()?,
        include_prompt_cache_retention: decode_bool("include_prompt_cache_retention", retention)?,
        request_body_limit_bytes: request_limit
            .map(|value| {
                u64::try_from(value).map_err(|_| {
                    ClientStorageError::InvalidState(
                        "request_body_limit_bytes is negative".to_string(),
                    )
                })
            })
            .transpose()?,
        max_transient_retries: retries
            .map(|value| {
                u32::try_from(value).map_err(|_| {
                    ClientStorageError::InvalidState(
                        "max_transient_retries is outside u32 range".to_string(),
                    )
                })
            })
            .transpose()?,
        output_format: output_format
            .map(|value| serde_json::from_str(&value))
            .transpose()?,
    };
    snapshot
        .validate()
        .map_err(ClientStorageError::InvalidState)?;
    Ok(snapshot)
}

fn decode_bool(name: &str, value: i64) -> Result<bool, ClientStorageError> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(ClientStorageError::InvalidState(format!(
            "{name} is not a SQLite boolean"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn command(id: &str, fingerprint: &str) -> IdempotentCommand {
        IdempotentCommand {
            command_id: id.to_string(),
            request_fingerprint: fingerprint.to_string(),
        }
    }

    fn snapshot(model: &str) -> LocalModelConfigSnapshot {
        LocalModelConfigSnapshot {
            model_config_ref: "default".to_string(),
            model_config_revision: "revision-1".to_string(),
            credential_ref: "keychain:model/default".to_string(),
            base_url: "https://api.example.test/v1".to_string(),
            model: model.to_string(),
            provider: "openai".to_string(),
            supports_responses: true,
            supports_images: Some(true),
            instructions: Some("be concise".to_string()),
            temperature: Some(0.2),
            max_output_tokens: Some(4096),
            thinking_level: Some("high".to_string()),
            include_prompt_cache_retention: true,
            request_body_limit_bytes: Some(1024 * 1024),
            max_transient_retries: Some(5),
            output_format: None,
        }
    }

    #[tokio::test]
    async fn immutable_model_snapshot_survives_reopen_without_an_api_key_column() {
        let database_path = std::env::temp_dir().join(format!(
            "chatos-local-model-config-{}.sqlite",
            Uuid::new_v4()
        ));
        let storage = SqliteClientStorage::connect_file(&database_path)
            .await
            .expect("storage");
        storage
            .put_model_config_snapshot(&command("put-1", "model-a"), &snapshot("model-a"), 1_000)
            .await
            .expect("put snapshot");
        storage
            .put_model_config_snapshot(&command("put-2", "model-a"), &snapshot("model-a"), 2_000)
            .await
            .expect("idempotent put");
        let columns: Vec<String> = sqlx::query("PRAGMA table_info(local_model_config_snapshots)")
            .fetch_all(&storage.pool)
            .await
            .expect("table info")
            .into_iter()
            .map(|row| row.get("name"))
            .collect();
        assert!(!columns.iter().any(|name| name == "api_key"));
        storage.pool.close().await;
        drop(storage);

        let reopened = SqliteClientStorage::connect_file(&database_path)
            .await
            .expect("reopen storage");
        assert_eq!(
            reopened
                .get_model_config_snapshot("default", "revision-1")
                .await
                .expect("get snapshot"),
            Some(snapshot("model-a"))
        );
        let error = reopened
            .put_model_config_snapshot(&command("put-3", "model-b"), &snapshot("model-b"), 3_000)
            .await
            .expect_err("revision reuse must fail");
        assert!(matches!(error, ClientStorageError::Conflict(_)));
        reopened.pool.close().await;
        drop(reopened);
        for path in [
            database_path.clone(),
            database_path.with_extension("sqlite-wal"),
            database_path.with_extension("sqlite-shm"),
        ] {
            if let Err(error) = std::fs::remove_file(path) {
                assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
            }
        }
    }
}
