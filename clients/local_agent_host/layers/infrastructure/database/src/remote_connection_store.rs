// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    ClientStorageError, IdempotentCommand, LocalRemoteConnectionStore, SqliteClientStorage,
    SqliteResultExt,
};
use async_trait::async_trait;
use chatos_local_agent_protocol::{
    LocalRemoteAuthenticationType, LocalRemoteConnection, LocalRemoteHostKeyPolicy,
};
use sqlx::{Row, SqliteConnection};

const REMOTE_CONNECTION_SELECT: &str =
    "SELECT connection_id, owner_user_id, name, host, port, username, authentication_type, \
     default_remote_path, host_key_policy, local_connector_device_id, \
     local_connector_workspace_id, jump_enabled, jump_connection_id, jump_host, jump_port, \
     jump_username, last_active_at_unix_ms, version, created_at_unix_ms, updated_at_unix_ms \
     FROM local_remote_connections";

#[async_trait]
impl LocalRemoteConnectionStore for SqliteClientStorage {
    async fn list_remote_connections(
        &self,
        owner_user_id: &str,
    ) -> Result<Vec<LocalRemoteConnection>, ClientStorageError> {
        let query = format!(
            "{REMOTE_CONNECTION_SELECT} WHERE owner_user_id = ? \
             ORDER BY updated_at_unix_ms DESC, connection_id"
        );
        sqlx::query(&query)
            .bind(owner_user_id)
            .fetch_all(&self.pool)
            .await
            .db()?
            .iter()
            .map(decode_connection)
            .collect()
    }

    async fn get_remote_connection(
        &self,
        owner_user_id: &str,
        connection_id: &str,
    ) -> Result<Option<LocalRemoteConnection>, ClientStorageError> {
        let query =
            format!("{REMOTE_CONNECTION_SELECT} WHERE owner_user_id = ? AND connection_id = ?");
        sqlx::query(&query)
            .bind(owner_user_id)
            .bind(connection_id)
            .fetch_optional(&self.pool)
            .await
            .db()?
            .as_ref()
            .map(decode_connection)
            .transpose()
    }

    async fn create_remote_connection(
        &self,
        command: &IdempotentCommand,
        connection: &LocalRemoteConnection,
    ) -> Result<LocalRemoteConnection, ClientStorageError> {
        let mut database = self.pool.acquire().await.db()?;
        Self::begin_immediate(&mut database).await?;
        let result = async {
            if let Some(replay) = Self::replay(&mut database, command).await? {
                return Ok(replay);
            }
            ensure_jump_exists(&mut database, connection).await?;
            sqlx::query(
                "INSERT INTO local_remote_connections(\
                   owner_user_id, connection_id, name, host, port, username, \
                   authentication_type, default_remote_path, host_key_policy, \
                   local_connector_device_id, local_connector_workspace_id, jump_enabled, \
                   jump_connection_id, jump_host, jump_port, jump_username, \
                   last_active_at_unix_ms, version, created_at_unix_ms, updated_at_unix_ms\
                 ) VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&connection.owner_user_id)
            .bind(&connection.connection_id)
            .bind(&connection.name)
            .bind(&connection.host)
            .bind(i64::from(connection.port))
            .bind(&connection.username)
            .bind(connection.authentication_type.as_str())
            .bind(&connection.default_remote_path)
            .bind(connection.host_key_policy.as_str())
            .bind(&connection.local_connector_device_id)
            .bind(&connection.local_connector_workspace_id)
            .bind(connection.jump_enabled)
            .bind(&connection.jump_connection_id)
            .bind(&connection.jump_host)
            .bind(connection.jump_port.map(i64::from))
            .bind(&connection.jump_username)
            .bind(connection.last_active_at_unix_ms)
            .bind(to_i64(connection.version, "remote connection version")?)
            .bind(connection.created_at_unix_ms)
            .bind(connection.updated_at_unix_ms)
            .execute(&mut *database)
            .await
            .db()?;
            Self::record_receipt(
                &mut database,
                command,
                connection,
                connection.updated_at_unix_ms,
            )
            .await?;
            Ok(connection.clone())
        }
        .await;
        Self::finish_write(&mut database, result).await
    }

    async fn update_remote_connection(
        &self,
        command: &IdempotentCommand,
        connection: &LocalRemoteConnection,
        expected_version: u64,
    ) -> Result<LocalRemoteConnection, ClientStorageError> {
        let mut database = self.pool.acquire().await.db()?;
        Self::begin_immediate(&mut database).await?;
        let result = async {
            if let Some(replay) = Self::replay(&mut database, command).await? {
                return Ok(replay);
            }
            ensure_jump_exists(&mut database, connection).await?;
            let updated = sqlx::query(
                "UPDATE local_remote_connections SET name = ?, host = ?, port = ?, \
                   username = ?, authentication_type = ?, default_remote_path = ?, \
                   host_key_policy = ?, local_connector_device_id = ?, \
                   local_connector_workspace_id = ?, jump_enabled = ?, jump_connection_id = ?, \
                   jump_host = ?, jump_port = ?, jump_username = ?, version = ?, \
                   updated_at_unix_ms = ? \
                 WHERE owner_user_id = ? AND connection_id = ? AND version = ?",
            )
            .bind(&connection.name)
            .bind(&connection.host)
            .bind(i64::from(connection.port))
            .bind(&connection.username)
            .bind(connection.authentication_type.as_str())
            .bind(&connection.default_remote_path)
            .bind(connection.host_key_policy.as_str())
            .bind(&connection.local_connector_device_id)
            .bind(&connection.local_connector_workspace_id)
            .bind(connection.jump_enabled)
            .bind(&connection.jump_connection_id)
            .bind(&connection.jump_host)
            .bind(connection.jump_port.map(i64::from))
            .bind(&connection.jump_username)
            .bind(to_i64(connection.version, "remote connection version")?)
            .bind(connection.updated_at_unix_ms)
            .bind(&connection.owner_user_id)
            .bind(&connection.connection_id)
            .bind(to_i64(
                expected_version,
                "expected remote connection version",
            )?)
            .execute(&mut *database)
            .await
            .db()?;
            if updated.rows_affected() != 1 {
                return Err(remote_connection_write_conflict(
                    &mut database,
                    &connection.owner_user_id,
                    &connection.connection_id,
                )
                .await?);
            }
            Self::record_receipt(
                &mut database,
                command,
                connection,
                connection.updated_at_unix_ms,
            )
            .await?;
            Ok(connection.clone())
        }
        .await;
        Self::finish_write(&mut database, result).await
    }

    async fn delete_remote_connection(
        &self,
        command: &IdempotentCommand,
        owner_user_id: &str,
        connection_id: &str,
        expected_version: u64,
        now_unix_ms: i64,
    ) -> Result<(), ClientStorageError> {
        let mut database = self.pool.acquire().await.db()?;
        Self::begin_immediate(&mut database).await?;
        let result = async {
            if let Some(replay) = Self::replay(&mut database, command).await? {
                return Ok(replay);
            }
            let deleted = sqlx::query(
                "DELETE FROM local_remote_connections \
                 WHERE owner_user_id = ? AND connection_id = ? AND version = ?",
            )
            .bind(owner_user_id)
            .bind(connection_id)
            .bind(to_i64(
                expected_version,
                "expected remote connection version",
            )?)
            .execute(&mut *database)
            .await
            .db()?;
            if deleted.rows_affected() != 1 {
                return Err(remote_connection_write_conflict(
                    &mut database,
                    owner_user_id,
                    connection_id,
                )
                .await?);
            }
            sqlx::query(
                "UPDATE local_remote_connections SET jump_connection_id = NULL, \
                 version = version + 1, updated_at_unix_ms = ? \
                 WHERE owner_user_id = ? AND jump_connection_id = ?",
            )
            .bind(now_unix_ms)
            .bind(owner_user_id)
            .bind(connection_id)
            .execute(&mut *database)
            .await
            .db()?;
            Self::record_receipt(&mut database, command, &(), now_unix_ms).await?;
            Ok(())
        }
        .await;
        Self::finish_write(&mut database, result).await
    }
}

async fn ensure_jump_exists(
    database: &mut SqliteConnection,
    connection: &LocalRemoteConnection,
) -> Result<(), ClientStorageError> {
    let Some(jump_id) = connection.jump_connection_id.as_deref() else {
        return Ok(());
    };
    if jump_id == connection.connection_id {
        return Err(ClientStorageError::Conflict(
            "remote connection cannot use itself as a jump connection".to_string(),
        ));
    }
    let exists: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM local_remote_connections \
         WHERE owner_user_id = ? AND connection_id = ?",
    )
    .bind(&connection.owner_user_id)
    .bind(jump_id)
    .fetch_one(&mut *database)
    .await
    .db()?;
    if exists != 1 {
        return Err(ClientStorageError::NotFound(jump_id.to_string()));
    }
    Ok(())
}

async fn remote_connection_write_conflict(
    database: &mut SqliteConnection,
    owner_user_id: &str,
    connection_id: &str,
) -> Result<ClientStorageError, ClientStorageError> {
    let exists: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM local_remote_connections \
         WHERE owner_user_id = ? AND connection_id = ?",
    )
    .bind(owner_user_id)
    .bind(connection_id)
    .fetch_one(&mut *database)
    .await
    .db()?;
    Ok(if exists == 0 {
        ClientStorageError::NotFound(connection_id.to_string())
    } else {
        ClientStorageError::Conflict(format!(
            "remote connection version changed: {connection_id}"
        ))
    })
}

fn decode_connection(
    row: &sqlx::sqlite::SqliteRow,
) -> Result<LocalRemoteConnection, ClientStorageError> {
    let authentication_type: String = row.try_get("authentication_type").db()?;
    let host_key_policy: String = row.try_get("host_key_policy").db()?;
    Ok(LocalRemoteConnection {
        connection_id: row.try_get("connection_id").db()?,
        owner_user_id: row.try_get("owner_user_id").db()?,
        name: row.try_get("name").db()?,
        host: row.try_get("host").db()?,
        port: from_i64(row.try_get("port").db()?, "remote connection port")?,
        username: row.try_get("username").db()?,
        authentication_type: match authentication_type.as_str() {
            "private_key" => LocalRemoteAuthenticationType::PrivateKey,
            "private_key_cert" => LocalRemoteAuthenticationType::PrivateKeyCert,
            "password" => LocalRemoteAuthenticationType::Password,
            _ => {
                return Err(ClientStorageError::InvalidState(format!(
                    "unknown remote authentication type: {authentication_type}"
                )))
            }
        },
        has_password: false,
        has_private_key_path: false,
        has_certificate_path: false,
        default_remote_path: row.try_get("default_remote_path").db()?,
        host_key_policy: match host_key_policy.as_str() {
            "strict" => LocalRemoteHostKeyPolicy::Strict,
            "accept_new" => LocalRemoteHostKeyPolicy::AcceptNew,
            _ => {
                return Err(ClientStorageError::InvalidState(format!(
                    "unknown remote host key policy: {host_key_policy}"
                )))
            }
        },
        local_connector_device_id: row.try_get("local_connector_device_id").db()?,
        local_connector_workspace_id: row.try_get("local_connector_workspace_id").db()?,
        jump_enabled: row.try_get("jump_enabled").db()?,
        jump_connection_id: row.try_get("jump_connection_id").db()?,
        jump_host: row.try_get("jump_host").db()?,
        jump_port: row
            .try_get::<Option<i64>, _>("jump_port")
            .db()?
            .map(|value| from_i64(value, "remote jump port"))
            .transpose()?,
        jump_username: row.try_get("jump_username").db()?,
        has_jump_private_key_path: false,
        has_jump_certificate_path: false,
        has_jump_password: false,
        last_active_at_unix_ms: row.try_get("last_active_at_unix_ms").db()?,
        version: u64::try_from(row.try_get::<i64, _>("version").db()?).map_err(|_| {
            ClientStorageError::InvalidState("remote connection version is invalid".to_string())
        })?,
        created_at_unix_ms: row.try_get("created_at_unix_ms").db()?,
        updated_at_unix_ms: row.try_get("updated_at_unix_ms").db()?,
    })
}

fn to_i64(value: u64, name: &str) -> Result<i64, ClientStorageError> {
    i64::try_from(value)
        .map_err(|_| ClientStorageError::InvalidState(format!("{name} exceeds i64")))
}

fn from_i64(value: i64, name: &str) -> Result<u32, ClientStorageError> {
    u32::try_from(value).map_err(|_| ClientStorageError::InvalidState(format!("{name} is invalid")))
}
