// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    ClientStorageError, IdempotentCommand, LocalConversationRuntimeSettingsStore,
    SqliteClientStorage, SqliteResultExt,
};
use async_trait::async_trait;
use chatos_local_agent_protocol::{
    LocalConversationRuntimeSettings, PutConversationRuntimeSettingsCommand,
};
use sqlx::{Row, SqliteConnection};

const SETTINGS_SELECT: &str = "SELECT owner_user_id, conversation_id, selected_model_config_ref, \
     selected_model_config_revision, selected_thinking_level, remote_connection_id, \
     reasoning_enabled, version, updated_at_unix_ms \
     FROM local_conversation_runtime_settings \
     WHERE owner_user_id = ? AND conversation_id = ?";

#[async_trait]
impl LocalConversationRuntimeSettingsStore for SqliteClientStorage {
    async fn get_conversation_runtime_settings(
        &self,
        owner_user_id: &str,
        conversation_id: &str,
    ) -> Result<Option<LocalConversationRuntimeSettings>, ClientStorageError> {
        let mut connection = self.pool.acquire().await.db()?;
        fetch_settings(&mut connection, owner_user_id, conversation_id).await
    }

    async fn put_conversation_runtime_settings(
        &self,
        command: &IdempotentCommand,
        settings: &PutConversationRuntimeSettingsCommand,
        now_unix_ms: i64,
    ) -> Result<LocalConversationRuntimeSettings, ClientStorageError> {
        settings
            .validate()
            .map_err(ClientStorageError::InvalidState)?;
        if now_unix_ms < 0 {
            return Err(ClientStorageError::InvalidState(
                "conversation runtime settings timestamp must not be negative".to_string(),
            ));
        }
        let mut connection = self.pool.acquire().await.db()?;
        Self::begin_immediate(&mut connection).await?;
        let result = async {
            if let Some(replay) = Self::replay(&mut connection, command).await? {
                return Ok(replay);
            }
            let current = fetch_settings(
                &mut connection,
                &settings.owner_user_id,
                &settings.conversation_id,
            )
            .await?;
            match current {
                None => {
                    if settings.expected_version.is_some() {
                        return Err(ClientStorageError::Conflict(format!(
                            "conversation runtime settings do not exist: {}",
                            settings.conversation_id
                        )));
                    }
                    insert_settings(&mut connection, settings, now_unix_ms).await?;
                }
                Some(current) => {
                    let expected = settings.expected_version.ok_or_else(|| {
                        ClientStorageError::Conflict(format!(
                            "expected_version is required to update conversation runtime settings: {}",
                            settings.conversation_id
                        ))
                    })?;
                    if expected != current.version {
                        return Err(ClientStorageError::Conflict(format!(
                            "conversation runtime settings version changed: {}",
                            settings.conversation_id
                        )));
                    }
                    update_settings(&mut connection, settings, current.version, now_unix_ms)
                        .await?;
                }
            }
            let stored = fetch_settings(
                &mut connection,
                &settings.owner_user_id,
                &settings.conversation_id,
            )
            .await?
            .ok_or_else(|| ClientStorageError::NotFound(settings.conversation_id.clone()))?;
            Self::record_receipt(&mut connection, command, &stored, now_unix_ms).await?;
            Ok(stored)
        }
        .await;
        Self::finish_write(&mut connection, result).await
    }
}

async fn insert_settings(
    connection: &mut SqliteConnection,
    settings: &PutConversationRuntimeSettingsCommand,
    now_unix_ms: i64,
) -> Result<(), ClientStorageError> {
    sqlx::query(
        "INSERT INTO local_conversation_runtime_settings(\
         owner_user_id, conversation_id, selected_model_config_ref, \
         selected_model_config_revision, selected_thinking_level, remote_connection_id, \
         reasoning_enabled, version, updated_at_unix_ms) \
         VALUES(?, ?, ?, ?, ?, ?, ?, 1, ?)",
    )
    .bind(&settings.owner_user_id)
    .bind(&settings.conversation_id)
    .bind(&settings.selected_model_config_ref)
    .bind(&settings.selected_model_config_revision)
    .bind(&settings.selected_thinking_level)
    .bind(&settings.remote_connection_id)
    .bind(i64::from(settings.reasoning_enabled))
    .bind(now_unix_ms)
    .execute(&mut *connection)
    .await
    .db()?;
    Ok(())
}

async fn update_settings(
    connection: &mut SqliteConnection,
    settings: &PutConversationRuntimeSettingsCommand,
    current_version: u64,
    now_unix_ms: i64,
) -> Result<(), ClientStorageError> {
    let current_version = i64::try_from(current_version).map_err(|_| {
        ClientStorageError::InvalidState("runtime settings version exceeds i64".to_string())
    })?;
    let changed = sqlx::query(
        "UPDATE local_conversation_runtime_settings SET \
         selected_model_config_ref = ?, selected_model_config_revision = ?, \
         selected_thinking_level = ?, remote_connection_id = ?, reasoning_enabled = ?, \
         version = version + 1, updated_at_unix_ms = ? \
         WHERE owner_user_id = ? AND conversation_id = ? AND version = ?",
    )
    .bind(&settings.selected_model_config_ref)
    .bind(&settings.selected_model_config_revision)
    .bind(&settings.selected_thinking_level)
    .bind(&settings.remote_connection_id)
    .bind(i64::from(settings.reasoning_enabled))
    .bind(now_unix_ms)
    .bind(&settings.owner_user_id)
    .bind(&settings.conversation_id)
    .bind(current_version)
    .execute(&mut *connection)
    .await
    .db()?;
    if changed.rows_affected() != 1 {
        return Err(ClientStorageError::Conflict(format!(
            "conversation runtime settings changed while updating: {}",
            settings.conversation_id
        )));
    }
    Ok(())
}

async fn fetch_settings(
    connection: &mut SqliteConnection,
    owner_user_id: &str,
    conversation_id: &str,
) -> Result<Option<LocalConversationRuntimeSettings>, ClientStorageError> {
    let row = sqlx::query(SETTINGS_SELECT)
        .bind(owner_user_id)
        .bind(conversation_id)
        .fetch_optional(&mut *connection)
        .await
        .db()?;
    row.map(|row| {
        let reasoning_enabled: i64 = row.try_get("reasoning_enabled").db()?;
        let version: i64 = row.try_get("version").db()?;
        Ok(LocalConversationRuntimeSettings {
            owner_user_id: row.try_get("owner_user_id").db()?,
            conversation_id: row.try_get("conversation_id").db()?,
            selected_model_config_ref: row.try_get("selected_model_config_ref").db()?,
            selected_model_config_revision: row.try_get("selected_model_config_revision").db()?,
            selected_thinking_level: row.try_get("selected_thinking_level").db()?,
            remote_connection_id: row.try_get("remote_connection_id").db()?,
            reasoning_enabled: match reasoning_enabled {
                0 => false,
                1 => true,
                _ => {
                    return Err(ClientStorageError::InvalidState(
                        "reasoning_enabled is not a SQLite boolean".to_string(),
                    ))
                }
            },
            version: u64::try_from(version).map_err(|_| {
                ClientStorageError::InvalidState(
                    "conversation runtime settings version is invalid".to_string(),
                )
            })?,
            updated_at_unix_ms: row.try_get("updated_at_unix_ms").db()?,
        })
    })
    .transpose()
}
