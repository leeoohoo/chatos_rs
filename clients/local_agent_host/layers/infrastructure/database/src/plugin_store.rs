// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    is_unique_violation, ClientStorageError, IdempotentCommand, LocalPluginInstallationStore,
    SqliteClientStorage, SqliteResultExt,
};
use async_trait::async_trait;
use chatos_local_agent_protocol::{LocalPluginInstallationRecord, LocalPluginInstallationSpec};
use sqlx::{sqlite::SqliteRow, Row, SqliteConnection};

const INSTALLATION_SELECT: &str =
    "SELECT installation_id, owner_user_id, plugin_id, release_id, release_digest, \
     component_id, component_revision, server_id, executable_path, args_json, \
     working_directory, environment_secret_refs_json, tool_prefix, allowed_tools_json, \
     enabled, version, created_at_unix_ms, updated_at_unix_ms \
     FROM local_plugin_installations";

#[async_trait]
impl LocalPluginInstallationStore for SqliteClientStorage {
    async fn put_plugin_installation(
        &self,
        command: &IdempotentCommand,
        installation: &LocalPluginInstallationSpec,
        expected_version: Option<u64>,
        now_unix_ms: i64,
    ) -> Result<LocalPluginInstallationRecord, ClientStorageError> {
        installation
            .validate()
            .map_err(ClientStorageError::InvalidState)?;
        let mut connection = self.pool.acquire().await.db()?;
        Self::begin_immediate(&mut connection).await?;
        let result = async {
            if let Some(replay) = Self::replay(&mut connection, command).await? {
                return Ok(replay);
            }
            let current_version = sqlx::query_scalar::<_, i64>(
                "SELECT version FROM local_plugin_installations WHERE installation_id = ?",
            )
            .bind(&installation.installation_id)
            .fetch_optional(&mut *connection)
            .await
            .db()?;
            match current_version {
                None => {
                    if expected_version.is_some() {
                        return Err(ClientStorageError::Conflict(format!(
                            "plugin installation does not exist: {}",
                            installation.installation_id
                        )));
                    }
                    insert_installation(&mut connection, installation, now_unix_ms).await?;
                }
                Some(current_version) => {
                    let expected = expected_version.ok_or_else(|| {
                        ClientStorageError::Conflict(format!(
                            "expected_version is required to update plugin installation: {}",
                            installation.installation_id
                        ))
                    })?;
                    if i64::try_from(expected).ok() != Some(current_version) {
                        return Err(ClientStorageError::Conflict(format!(
                            "plugin installation version changed: {}",
                            installation.installation_id
                        )));
                    }
                    update_installation(
                        &mut connection,
                        installation,
                        current_version,
                        now_unix_ms,
                    )
                    .await?;
                }
            }
            let stored = fetch_installation(&mut connection, &installation.installation_id)
                .await?
                .ok_or_else(|| {
                    ClientStorageError::NotFound(installation.installation_id.clone())
                })?;
            Self::record_receipt(&mut connection, command, &stored, now_unix_ms).await?;
            Ok(stored)
        }
        .await;
        Self::finish_write(&mut connection, result).await
    }

    async fn get_plugin_installation(
        &self,
        installation_id: &str,
    ) -> Result<Option<LocalPluginInstallationRecord>, ClientStorageError> {
        let mut connection = self.pool.acquire().await.db()?;
        fetch_installation(&mut connection, installation_id).await
    }

    async fn list_plugin_installations(
        &self,
        owner_user_id: &str,
        limit: u32,
    ) -> Result<Vec<LocalPluginInstallationRecord>, ClientStorageError> {
        if !(1..=500).contains(&limit) {
            return Err(ClientStorageError::InvalidState(
                "plugin installation limit must be between 1 and 500".to_string(),
            ));
        }
        let mut connection = self.pool.acquire().await.db()?;
        sqlx::query(&format!(
            "{INSTALLATION_SELECT} WHERE owner_user_id = ? \
             ORDER BY updated_at_unix_ms DESC, installation_id LIMIT ?"
        ))
        .bind(owner_user_id)
        .bind(i64::from(limit))
        .fetch_all(&mut *connection)
        .await
        .db()?
        .into_iter()
        .map(decode_installation)
        .collect()
    }

    async fn remove_plugin_installation(
        &self,
        command: &IdempotentCommand,
        installation_id: &str,
        expected_version: u64,
        now_unix_ms: i64,
    ) -> Result<LocalPluginInstallationRecord, ClientStorageError> {
        let mut connection = self.pool.acquire().await.db()?;
        Self::begin_immediate(&mut connection).await?;
        let result = async {
            if let Some(replay) = Self::replay(&mut connection, command).await? {
                return Ok(replay);
            }
            let existing = fetch_installation(&mut connection, installation_id)
                .await?
                .ok_or_else(|| ClientStorageError::NotFound(installation_id.to_string()))?;
            if existing.version != expected_version {
                return Err(ClientStorageError::Conflict(format!(
                    "plugin installation version changed: {installation_id}"
                )));
            }
            let deleted = sqlx::query(
                "DELETE FROM local_plugin_installations WHERE installation_id = ? AND version = ?",
            )
            .bind(installation_id)
            .bind(i64::try_from(expected_version).map_err(|_| {
                ClientStorageError::InvalidState("plugin version exceeds i64".to_string())
            })?)
            .execute(&mut *connection)
            .await
            .db()?;
            if deleted.rows_affected() != 1 {
                return Err(ClientStorageError::Conflict(format!(
                    "plugin installation changed while removing: {installation_id}"
                )));
            }
            Self::record_receipt(&mut connection, command, &existing, now_unix_ms).await?;
            Ok(existing)
        }
        .await;
        Self::finish_write(&mut connection, result).await
    }
}

async fn insert_installation(
    connection: &mut SqliteConnection,
    installation: &LocalPluginInstallationSpec,
    now_unix_ms: i64,
) -> Result<(), ClientStorageError> {
    let result = sqlx::query(
        "INSERT INTO local_plugin_installations(\
         installation_id, owner_user_id, plugin_id, release_id, release_digest, component_id, \
         component_revision, server_id, executable_path, args_json, working_directory, \
         environment_secret_refs_json, tool_prefix, allowed_tools_json, enabled, version, \
         created_at_unix_ms, updated_at_unix_ms) \
         VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?)",
    )
    .bind(&installation.installation_id)
    .bind(&installation.owner_user_id)
    .bind(&installation.plugin_id)
    .bind(&installation.release_id)
    .bind(&installation.release_digest)
    .bind(&installation.component_id)
    .bind(&installation.component_revision)
    .bind(&installation.server_id)
    .bind(&installation.executable_path)
    .bind(serde_json::to_string(&installation.args)?)
    .bind(&installation.working_directory)
    .bind(serde_json::to_string(
        &installation.environment_secret_refs,
    )?)
    .bind(&installation.tool_prefix)
    .bind(
        installation
            .allowed_tools
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?,
    )
    .bind(installation.enabled)
    .bind(now_unix_ms)
    .bind(now_unix_ms)
    .execute(&mut *connection)
    .await;
    match result {
        Ok(_) => Ok(()),
        Err(error) if is_unique_violation(&error) => Err(ClientStorageError::Conflict(format!(
            "plugin installation or component already exists: {}",
            installation.installation_id
        ))),
        Err(error) => Err(ClientStorageError::database(error)),
    }
}

async fn update_installation(
    connection: &mut SqliteConnection,
    installation: &LocalPluginInstallationSpec,
    current_version: i64,
    now_unix_ms: i64,
) -> Result<(), ClientStorageError> {
    let result = sqlx::query(
        "UPDATE local_plugin_installations SET owner_user_id = ?, plugin_id = ?, \
         release_id = ?, release_digest = ?, component_id = ?, component_revision = ?, \
         server_id = ?, executable_path = ?, args_json = ?, working_directory = ?, \
         environment_secret_refs_json = ?, tool_prefix = ?, allowed_tools_json = ?, enabled = ?, \
         version = version + 1, updated_at_unix_ms = ? \
         WHERE installation_id = ? AND version = ?",
    )
    .bind(&installation.owner_user_id)
    .bind(&installation.plugin_id)
    .bind(&installation.release_id)
    .bind(&installation.release_digest)
    .bind(&installation.component_id)
    .bind(&installation.component_revision)
    .bind(&installation.server_id)
    .bind(&installation.executable_path)
    .bind(serde_json::to_string(&installation.args)?)
    .bind(&installation.working_directory)
    .bind(serde_json::to_string(
        &installation.environment_secret_refs,
    )?)
    .bind(&installation.tool_prefix)
    .bind(
        installation
            .allowed_tools
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?,
    )
    .bind(installation.enabled)
    .bind(now_unix_ms)
    .bind(&installation.installation_id)
    .bind(current_version)
    .execute(&mut *connection)
    .await;
    match result {
        Ok(result) if result.rows_affected() == 1 => Ok(()),
        Ok(_) => Err(ClientStorageError::Conflict(format!(
            "plugin installation changed while updating: {}",
            installation.installation_id
        ))),
        Err(error) if is_unique_violation(&error) => Err(ClientStorageError::Conflict(format!(
            "plugin component is already installed: {}/{}",
            installation.plugin_id, installation.component_id
        ))),
        Err(error) => Err(ClientStorageError::database(error)),
    }
}

async fn fetch_installation(
    connection: &mut SqliteConnection,
    installation_id: &str,
) -> Result<Option<LocalPluginInstallationRecord>, ClientStorageError> {
    sqlx::query(&format!("{INSTALLATION_SELECT} WHERE installation_id = ?"))
        .bind(installation_id)
        .fetch_optional(&mut *connection)
        .await
        .db()?
        .map(decode_installation)
        .transpose()
}

fn decode_installation(
    row: SqliteRow,
) -> Result<LocalPluginInstallationRecord, ClientStorageError> {
    let args: String = row.try_get("args_json").db()?;
    let secret_refs: String = row.try_get("environment_secret_refs_json").db()?;
    let allowed_tools: Option<String> = row.try_get("allowed_tools_json").db()?;
    Ok(LocalPluginInstallationRecord {
        spec: LocalPluginInstallationSpec {
            installation_id: row.try_get("installation_id").db()?,
            owner_user_id: row.try_get("owner_user_id").db()?,
            plugin_id: row.try_get("plugin_id").db()?,
            release_id: row.try_get("release_id").db()?,
            release_digest: row.try_get("release_digest").db()?,
            component_id: row.try_get("component_id").db()?,
            component_revision: row.try_get("component_revision").db()?,
            server_id: row.try_get("server_id").db()?,
            executable_path: row.try_get("executable_path").db()?,
            args: serde_json::from_str(&args)?,
            working_directory: row.try_get("working_directory").db()?,
            environment_secret_refs: serde_json::from_str(&secret_refs)?,
            tool_prefix: row.try_get("tool_prefix").db()?,
            allowed_tools: allowed_tools
                .map(|value| serde_json::from_str(&value))
                .transpose()?,
            enabled: row.try_get("enabled").db()?,
        },
        version: u64::try_from(row.try_get::<i64, _>("version").db()?).map_err(|_| {
            ClientStorageError::InvalidState("invalid plugin installation version".to_string())
        })?,
        created_at_unix_ms: row.try_get("created_at_unix_ms").db()?,
        updated_at_unix_ms: row.try_get("updated_at_unix_ms").db()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn command(id: &str) -> IdempotentCommand {
        IdempotentCommand {
            command_id: id.to_string(),
            request_fingerprint: id.to_string(),
        }
    }

    fn installation(release_id: &str) -> LocalPluginInstallationSpec {
        LocalPluginInstallationSpec {
            installation_id: "install-1".to_string(),
            owner_user_id: "user-1".to_string(),
            plugin_id: "plugin-1".to_string(),
            release_id: release_id.to_string(),
            release_digest: "sha256:abc".to_string(),
            component_id: "mcp-1".to_string(),
            component_revision: "revision-1".to_string(),
            server_id: "files".to_string(),
            executable_path: "/plugins/files/server".to_string(),
            args: vec!["--stdio".to_string()],
            working_directory: None,
            environment_secret_refs: BTreeMap::from([(
                "API_TOKEN".to_string(),
                "keychain:plugin-1/token".to_string(),
            )]),
            tool_prefix: Some("files".to_string()),
            allowed_tools: Some(vec!["read_file".to_string()]),
            enabled: true,
        }
    }

    #[tokio::test]
    async fn installation_lifecycle_is_cas_protected_and_idempotent() {
        let storage = SqliteClientStorage::connect_memory()
            .await
            .expect("storage");
        let created = storage
            .put_plugin_installation(&command("put-1"), &installation("release-1"), None, 10)
            .await
            .expect("create");
        assert_eq!(created.version, 1);
        let replay = storage
            .put_plugin_installation(&command("put-1"), &installation("release-1"), None, 10)
            .await
            .expect("replay");
        assert_eq!(replay, created);
        assert!(storage
            .put_plugin_installation(
                &command("put-stale"),
                &installation("release-2"),
                Some(9),
                20,
            )
            .await
            .is_err());
        let updated = storage
            .put_plugin_installation(&command("put-2"), &installation("release-2"), Some(1), 20)
            .await
            .expect("update");
        assert_eq!(updated.version, 2);
        assert_eq!(updated.spec.release_id, "release-2");
        assert_eq!(
            storage
                .list_plugin_installations("user-1", 10)
                .await
                .expect("list"),
            vec![updated.clone()]
        );
        let removed = storage
            .remove_plugin_installation(&command("remove-1"), "install-1", 2, 30)
            .await
            .expect("remove");
        assert_eq!(removed, updated);
        assert!(storage
            .get_plugin_installation("install-1")
            .await
            .expect("get")
            .is_none());
    }
}
