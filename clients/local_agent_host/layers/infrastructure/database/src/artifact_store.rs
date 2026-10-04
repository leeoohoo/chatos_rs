// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    ClientStorageError, IdempotentCommand, LocalAgentArtifactStore, LocalAgentArtifactWrite,
    SqliteClientStorage, SqliteResultExt,
};
use async_trait::async_trait;
use chatos_local_agent_protocol::{LocalAgentArtifact, LocalAgentArtifactPage};
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::path::{Path, PathBuf};
use uuid::Uuid;

const ARTIFACT_SELECT: &str = "SELECT artifact_id, owner_user_id, name, mime_type, size, sha256, \
     created_at_unix_ms, updated_at_unix_ms FROM local_agent_artifacts";

#[async_trait]
impl LocalAgentArtifactStore for SqliteClientStorage {
    async fn create_artifact(
        &self,
        command: &IdempotentCommand,
        write: &LocalAgentArtifactWrite,
    ) -> Result<LocalAgentArtifact, ClientStorageError> {
        let mut database = self.pool.acquire().await.db()?;
        Self::begin_immediate(&mut database).await?;
        let mut created_path = None;
        let result = async {
            if let Some(replay) = Self::replay(&mut database, command).await? {
                return Ok(replay);
            }
            if let Some(row) = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{ARTIFACT_SELECT} WHERE owner_user_id = ? AND artifact_id = (\
                 SELECT artifact_id FROM local_agent_artifacts \
                 WHERE owner_user_id = ? AND idempotency_key = ?)"
            )))
            .bind(&write.artifact.owner_user_id)
            .bind(&write.artifact.owner_user_id)
            .bind(&write.idempotency_key)
            .fetch_optional(&mut *database)
            .await
            .db()?
            {
                let existing = decode_artifact(&row)?;
                if existing.sha256 != write.artifact.sha256
                    || existing.size != write.artifact.size
                    || existing.name != write.artifact.name
                    || existing.mime_type != write.artifact.mime_type
                {
                    return Err(ClientStorageError::Conflict(
                        "artifact idempotency key was reused with different content".to_string(),
                    ));
                }
                Self::record_receipt(
                    &mut database,
                    command,
                    &existing,
                    existing.updated_at_unix_ms,
                )
                .await?;
                return Ok(existing);
            }

            let relative_path = format!("{}.artifact", write.artifact.artifact_id);
            let final_path = safe_artifact_path(&self.artifact_root, &relative_path)?;
            write_private_file(&self.artifact_root, &final_path, &write.data).await?;
            created_path = Some(final_path);
            sqlx::query(
                "INSERT INTO local_agent_artifacts(\
                   owner_user_id, artifact_id, idempotency_key, name, mime_type, size, sha256, \
                   relative_path, created_at_unix_ms, updated_at_unix_ms\
                 ) VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&write.artifact.owner_user_id)
            .bind(&write.artifact.artifact_id)
            .bind(&write.idempotency_key)
            .bind(&write.artifact.name)
            .bind(&write.artifact.mime_type)
            .bind(to_i64(write.artifact.size, "artifact size")?)
            .bind(&write.artifact.sha256)
            .bind(&relative_path)
            .bind(write.artifact.created_at_unix_ms)
            .bind(write.artifact.updated_at_unix_ms)
            .execute(&mut *database)
            .await
            .db()?;
            Self::record_receipt(
                &mut database,
                command,
                &write.artifact,
                write.artifact.updated_at_unix_ms,
            )
            .await?;
            Ok(write.artifact.clone())
        }
        .await;
        let result = Self::finish_write(&mut database, result).await;
        if result.is_err() {
            if let Some(path) = created_path {
                let _ = tokio::fs::remove_file(path).await;
            }
        }
        result
    }

    async fn list_artifacts(
        &self,
        owner_user_id: &str,
        before_updated_at_unix_ms: Option<i64>,
        before_artifact_id: Option<&str>,
        limit: u32,
    ) -> Result<LocalAgentArtifactPage, ClientStorageError> {
        let fetch_limit = i64::from(limit) + 1;
        let rows = if let (Some(updated_at), Some(artifact_id)) =
            (before_updated_at_unix_ms, before_artifact_id)
        {
            sqlx::query(sqlx::AssertSqlSafe(format!(
                "{ARTIFACT_SELECT} WHERE owner_user_id = ? AND \
                 (updated_at_unix_ms < ? OR (updated_at_unix_ms = ? AND artifact_id < ?)) \
                 ORDER BY updated_at_unix_ms DESC, artifact_id DESC LIMIT ?"
            )))
            .bind(owner_user_id)
            .bind(updated_at)
            .bind(updated_at)
            .bind(artifact_id)
            .bind(fetch_limit)
            .fetch_all(&self.pool)
            .await
            .db()?
        } else {
            sqlx::query(sqlx::AssertSqlSafe(format!(
                "{ARTIFACT_SELECT} WHERE owner_user_id = ? \
                 ORDER BY updated_at_unix_ms DESC, artifact_id DESC LIMIT ?"
            )))
            .bind(owner_user_id)
            .bind(fetch_limit)
            .fetch_all(&self.pool)
            .await
            .db()?
        };
        let has_more = rows.len() > limit as usize;
        let artifacts = rows
            .iter()
            .take(limit as usize)
            .map(decode_artifact)
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = has_more.then(|| {
            let last = artifacts.last().expect("non-empty page with continuation");
            format!("{}:{}", last.updated_at_unix_ms, last.artifact_id)
        });
        Ok(LocalAgentArtifactPage {
            artifacts,
            next_cursor,
        })
    }

    async fn read_artifact_data(
        &self,
        owner_user_id: &str,
        artifact_id: &str,
    ) -> Result<Option<Vec<u8>>, ClientStorageError> {
        let row = sqlx::query(
            "SELECT relative_path, size, sha256 FROM local_agent_artifacts \
             WHERE owner_user_id = ? AND artifact_id = ?",
        )
        .bind(owner_user_id)
        .bind(artifact_id)
        .fetch_optional(&self.pool)
        .await
        .db()?;
        let Some(row) = row else { return Ok(None) };
        let relative_path: String = row.try_get("relative_path").db()?;
        let expected_size: i64 = row.try_get("size").db()?;
        let expected_sha256: String = row.try_get("sha256").db()?;
        let data = tokio::fs::read(safe_artifact_path(&self.artifact_root, &relative_path)?)
            .await
            .map_err(ClientStorageError::database)?;
        let digest = format!("{:x}", Sha256::digest(&data));
        if i64::try_from(data.len()).ok() != Some(expected_size) || digest != expected_sha256 {
            return Err(ClientStorageError::InvalidState(format!(
                "artifact content failed integrity verification: {artifact_id}"
            )));
        }
        Ok(Some(data))
    }

    async fn delete_artifact(
        &self,
        command: &IdempotentCommand,
        owner_user_id: &str,
        artifact_id: &str,
        now_unix_ms: i64,
    ) -> Result<(), ClientStorageError> {
        let mut database = self.pool.acquire().await.db()?;
        Self::begin_immediate(&mut database).await?;
        let result = async {
            if Self::replay::<()>(&mut database, command).await?.is_some() {
                return Ok(None);
            }
            let relative_path: Option<String> = sqlx::query_scalar(
                "SELECT relative_path FROM local_agent_artifacts \
                 WHERE owner_user_id = ? AND artifact_id = ?",
            )
            .bind(owner_user_id)
            .bind(artifact_id)
            .fetch_optional(&mut *database)
            .await
            .db()?;
            let relative_path = relative_path
                .ok_or_else(|| ClientStorageError::NotFound(artifact_id.to_string()))?;
            sqlx::query(
                "DELETE FROM local_agent_artifacts WHERE owner_user_id = ? AND artifact_id = ?",
            )
            .bind(owner_user_id)
            .bind(artifact_id)
            .execute(&mut *database)
            .await
            .db()?;
            Self::record_receipt(&mut database, command, &(), now_unix_ms).await?;
            Ok(Some(relative_path))
        }
        .await;
        let Some(relative_path) = Self::finish_write(&mut database, result).await? else {
            return Ok(());
        };
        let path = safe_artifact_path(&self.artifact_root, &relative_path)?;
        match tokio::fs::remove_file(path).await {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(ClientStorageError::database(error)),
        }
    }
}

fn decode_artifact(
    row: &sqlx::sqlite::SqliteRow,
) -> Result<LocalAgentArtifact, ClientStorageError> {
    let size: i64 = row.try_get("size").db()?;
    Ok(LocalAgentArtifact {
        artifact_id: row.try_get("artifact_id").db()?,
        owner_user_id: row.try_get("owner_user_id").db()?,
        name: row.try_get("name").db()?,
        mime_type: row.try_get("mime_type").db()?,
        size: u64::try_from(size).map_err(|_| {
            ClientStorageError::InvalidState("artifact size is negative".to_string())
        })?,
        sha256: row.try_get("sha256").db()?,
        created_at_unix_ms: row.try_get("created_at_unix_ms").db()?,
        updated_at_unix_ms: row.try_get("updated_at_unix_ms").db()?,
    })
}

fn safe_artifact_path(root: &Path, relative_path: &str) -> Result<PathBuf, ClientStorageError> {
    if relative_path.is_empty()
        || relative_path.contains('/')
        || relative_path.contains('\\')
        || relative_path == "."
        || relative_path == ".."
    {
        return Err(ClientStorageError::InvalidState(
            "invalid artifact content path".to_string(),
        ));
    }
    Ok(root.join(relative_path))
}

async fn write_private_file(
    root: &Path,
    final_path: &Path,
    data: &[u8],
) -> Result<(), ClientStorageError> {
    let temporary_path = root.join(format!(".{}.tmp", Uuid::new_v4()));
    tokio::fs::write(&temporary_path, data)
        .await
        .map_err(ClientStorageError::database)?;
    if let Err(error) = restrict_file_permissions(&temporary_path).await {
        let _ = tokio::fs::remove_file(&temporary_path).await;
        return Err(error);
    }
    if let Err(error) = tokio::fs::rename(&temporary_path, final_path).await {
        let _ = tokio::fs::remove_file(&temporary_path).await;
        return Err(ClientStorageError::database(error));
    }
    Ok(())
}

#[cfg(unix)]
async fn restrict_file_permissions(path: &Path) -> Result<(), ClientStorageError> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = tokio::fs::metadata(path)
        .await
        .map_err(ClientStorageError::database)?
        .permissions();
    permissions.set_mode(0o600);
    tokio::fs::set_permissions(path, permissions)
        .await
        .map_err(ClientStorageError::database)
}

#[cfg(not(unix))]
async fn restrict_file_permissions(_path: &Path) -> Result<(), ClientStorageError> {
    Ok(())
}

fn to_i64(value: u64, field: &str) -> Result<i64, ClientStorageError> {
    i64::try_from(value)
        .map_err(|_| ClientStorageError::InvalidState(format!("{field} exceeds SQLite range")))
}
