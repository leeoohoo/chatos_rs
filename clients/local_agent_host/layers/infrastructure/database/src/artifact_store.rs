// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    ClientStorageError, IdempotentCommand, LocalAgentArtifactStore, LocalAgentArtifactWrite,
    SqliteClientStorage, SqliteResultExt,
};
use async_trait::async_trait;
use chatos_local_agent_protocol::{
    LocalAgentArtifact, LocalAgentArtifactFile, LocalAgentArtifactPage,
};
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::{
    fs::{Metadata, OpenOptions},
    io::Read,
    path::{Path, PathBuf},
};
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
        {
            let mut database = self.pool.acquire().await.db()?;
            if let Some(replay) = Self::replay(&mut database, command).await? {
                discard_staged_file(&self.artifact_staging_root, &write.staging_token).await;
                return Ok(replay);
            }
        }
        if let Some(existing) = existing_artifact(self, write).await? {
            discard_staged_file(&self.artifact_staging_root, &write.staging_token).await;
            return validate_idempotent_artifact(existing, write);
        }

        let mut prepared_path = Some(
            prepare_staged_artifact(
                self.artifact_staging_root.clone(),
                self.artifact_root.clone(),
                write.clone(),
            )
            .await?,
        );
        let mut database = match self.acquire_write().await.db() {
            Ok(database) => database,
            Err(error) => {
                if let Some(path) = prepared_path.take() {
                    let _ = tokio::fs::remove_file(path).await;
                }
                return Err(error);
            }
        };
        if let Err(error) = Self::begin_immediate(&mut database).await {
            if let Some(path) = prepared_path.take() {
                let _ = tokio::fs::remove_file(path).await;
            }
            return Err(error);
        }
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
                let existing = validate_idempotent_artifact(existing, write)?;
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
            if tokio::fs::symlink_metadata(&final_path).await.is_ok() {
                return Err(ClientStorageError::Conflict(
                    "artifact content path already exists".to_string(),
                ));
            }
            let source_path = prepared_path.as_ref().ok_or_else(|| {
                ClientStorageError::InvalidState("artifact staging file was consumed".to_string())
            })?;
            tokio::fs::rename(source_path, &final_path)
                .await
                .map_err(ClientStorageError::database)?;
            prepared_path = None;
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
        if let Some(path) = prepared_path {
            let _ = tokio::fs::remove_file(path).await;
        }
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

    async fn read_artifact_file(
        &self,
        owner_user_id: &str,
        artifact_id: &str,
    ) -> Result<Option<LocalAgentArtifactFile>, ClientStorageError> {
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
        if relative_path != format!("{artifact_id}.artifact") {
            return Err(ClientStorageError::InvalidState(format!(
                "artifact file identity failed verification: {artifact_id}"
            )));
        }
        let size: i64 = row.try_get("size").db()?;
        let size = u64::try_from(size).map_err(|_| {
            ClientStorageError::InvalidState("artifact size is negative".to_string())
        })?;
        let sha256: String = row.try_get("sha256").db()?;
        let path = safe_artifact_path(&self.artifact_root, &relative_path)?;
        let metadata = tokio::fs::symlink_metadata(path)
            .await
            .map_err(ClientStorageError::database)?;
        if !metadata.file_type().is_file() || metadata.len() != size {
            return Err(ClientStorageError::InvalidState(format!(
                "artifact file metadata failed verification: {artifact_id}"
            )));
        }
        Ok(Some(LocalAgentArtifactFile {
            artifact_id: artifact_id.to_string(),
            relative_path,
            size,
            sha256,
        }))
    }

    async fn delete_artifact(
        &self,
        command: &IdempotentCommand,
        owner_user_id: &str,
        artifact_id: &str,
        now_unix_ms: i64,
    ) -> Result<(), ClientStorageError> {
        let mut database = self.acquire_write().await.db()?;
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

async fn existing_artifact(
    storage: &SqliteClientStorage,
    write: &LocalAgentArtifactWrite,
) -> Result<Option<LocalAgentArtifact>, ClientStorageError> {
    let row = sqlx::query(sqlx::AssertSqlSafe(format!(
        "{ARTIFACT_SELECT} WHERE owner_user_id = ? AND artifact_id = (\
         SELECT artifact_id FROM local_agent_artifacts \
         WHERE owner_user_id = ? AND idempotency_key = ?)"
    )))
    .bind(&write.artifact.owner_user_id)
    .bind(&write.artifact.owner_user_id)
    .bind(&write.idempotency_key)
    .fetch_optional(&storage.pool)
    .await
    .db()?;
    row.as_ref().map(decode_artifact).transpose()
}

fn validate_idempotent_artifact(
    existing: LocalAgentArtifact,
    write: &LocalAgentArtifactWrite,
) -> Result<LocalAgentArtifact, ClientStorageError> {
    if existing.sha256 != write.artifact.sha256
        || existing.size != write.artifact.size
        || existing.name != write.artifact.name
        || existing.mime_type != write.artifact.mime_type
    {
        return Err(ClientStorageError::Conflict(
            "artifact idempotency key was reused with different content".to_string(),
        ));
    }
    Ok(existing)
}

async fn discard_staged_file(root: &Path, token: &str) {
    if let Ok(path) = safe_staging_path(root, token) {
        let _ = tokio::fs::remove_file(path).await;
    }
}

fn safe_staging_path(root: &Path, token: &str) -> Result<PathBuf, ClientStorageError> {
    if token.len() != 36
        || !token.bytes().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => byte == b'-',
            _ => byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte),
        })
    {
        return Err(ClientStorageError::InvalidState(
            "invalid artifact staging token".to_string(),
        ));
    }
    Ok(root.join(format!("{token}.staged")))
}

async fn prepare_staged_artifact(
    staging_root: PathBuf,
    artifact_root: PathBuf,
    write: LocalAgentArtifactWrite,
) -> Result<PathBuf, ClientStorageError> {
    tokio::task::spawn_blocking(move || {
        prepare_staged_artifact_sync(&staging_root, &artifact_root, &write)
    })
    .await
    .map_err(ClientStorageError::database)?
}

fn prepare_staged_artifact_sync(
    staging_root: &Path,
    artifact_root: &Path,
    write: &LocalAgentArtifactWrite,
) -> Result<PathBuf, ClientStorageError> {
    let staged_path = safe_staging_path(staging_root, &write.staging_token)?;
    let path_metadata =
        std::fs::symlink_metadata(&staged_path).map_err(ClientStorageError::database)?;
    if !path_metadata.file_type().is_file() || path_metadata.file_type().is_symlink() {
        return Err(invalid_staged_artifact());
    }

    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    let mut file = options
        .open(&staged_path)
        .map_err(ClientStorageError::database)?;
    let initial = file.metadata().map_err(ClientStorageError::database)?;
    if !initial.is_file()
        || initial.len() != write.artifact.size
        || !same_file_identity(&path_metadata, &initial)
        || !has_single_link(&initial)
        || !has_private_permissions(&initial)
    {
        return Err(invalid_staged_artifact());
    }

    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 256 * 1024];
    let mut utf8_pending = Vec::with_capacity(3);
    let mut utf8_scratch = Vec::with_capacity(buffer.len() + 3);
    let mut total = 0_u64;
    loop {
        let count = match file.read(&mut buffer) {
            Ok(count) => count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(ClientStorageError::database(error)),
        };
        if count == 0 {
            break;
        }
        total = total
            .checked_add(count as u64)
            .ok_or_else(invalid_staged_artifact)?;
        if total > write.artifact.size {
            return Err(invalid_staged_artifact());
        }
        hasher.update(&buffer[..count]);
        validate_utf8_chunk(&mut utf8_pending, &mut utf8_scratch, &buffer[..count])?;
    }
    if total != write.artifact.size || !utf8_pending.is_empty() {
        return Err(invalid_staged_artifact());
    }
    let digest = format!("{:x}", hasher.finalize());
    if digest != write.artifact.sha256 {
        return Err(invalid_staged_artifact());
    }
    let final_metadata = file.metadata().map_err(ClientStorageError::database)?;
    let final_path_metadata =
        std::fs::symlink_metadata(&staged_path).map_err(ClientStorageError::database)?;
    if !same_file_identity(&initial, &final_metadata)
        || !same_file_identity(&final_metadata, &final_path_metadata)
        || !has_single_link(&final_metadata)
    {
        return Err(invalid_staged_artifact());
    }
    file.sync_all().map_err(ClientStorageError::database)?;

    let prepared_path = artifact_root.join(format!(".{}.import", Uuid::new_v4()));
    std::fs::rename(&staged_path, &prepared_path).map_err(ClientStorageError::database)?;
    let moved_metadata =
        std::fs::symlink_metadata(&prepared_path).map_err(ClientStorageError::database)?;
    if !same_inode(&final_metadata, &moved_metadata) || !has_single_link(&moved_metadata) {
        let _ = std::fs::remove_file(&prepared_path);
        return Err(invalid_staged_artifact());
    }
    Ok(prepared_path)
}

fn validate_utf8_chunk(
    pending: &mut Vec<u8>,
    scratch: &mut Vec<u8>,
    chunk: &[u8],
) -> Result<(), ClientStorageError> {
    scratch.clear();
    scratch.extend_from_slice(pending);
    scratch.extend_from_slice(chunk);
    pending.clear();
    match std::str::from_utf8(scratch) {
        Ok(_) => Ok(()),
        Err(error) if error.error_len().is_none() => {
            pending.extend_from_slice(&scratch[error.valid_up_to()..]);
            if pending.len() <= 3 {
                Ok(())
            } else {
                Err(invalid_staged_artifact())
            }
        }
        Err(_) => Err(invalid_staged_artifact()),
    }
}

fn invalid_staged_artifact() -> ClientStorageError {
    ClientStorageError::InvalidState(
        "artifact staging file failed integrity verification".to_string(),
    )
}

#[cfg(unix)]
fn same_file_identity(lhs: &Metadata, rhs: &Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;

    lhs.dev() == rhs.dev()
        && lhs.ino() == rhs.ino()
        && lhs.size() == rhs.size()
        && lhs.mtime() == rhs.mtime()
        && lhs.mtime_nsec() == rhs.mtime_nsec()
        && lhs.ctime() == rhs.ctime()
        && lhs.ctime_nsec() == rhs.ctime_nsec()
}

#[cfg(unix)]
fn same_inode(lhs: &Metadata, rhs: &Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;

    lhs.dev() == rhs.dev() && lhs.ino() == rhs.ino() && lhs.size() == rhs.size()
}

#[cfg(not(unix))]
fn same_file_identity(lhs: &Metadata, rhs: &Metadata) -> bool {
    lhs.len() == rhs.len()
        && lhs.modified().ok() == rhs.modified().ok()
        && lhs.created().ok() == rhs.created().ok()
}

#[cfg(not(unix))]
fn same_inode(lhs: &Metadata, rhs: &Metadata) -> bool {
    lhs.len() == rhs.len()
}

#[cfg(unix)]
fn has_single_link(metadata: &Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;

    metadata.nlink() == 1
}

#[cfg(not(unix))]
fn has_single_link(_metadata: &Metadata) -> bool {
    true
}

#[cfg(unix)]
fn has_private_permissions(metadata: &Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;

    metadata.permissions().mode() & 0o077 == 0
}

#[cfg(not(unix))]
fn has_private_permissions(_metadata: &Metadata) -> bool {
    true
}

fn to_i64(value: u64, field: &str) -> Result<i64, ClientStorageError> {
    i64::try_from(value)
        .map_err(|_| ClientStorageError::InvalidState(format!("{field} exceeds SQLite range")))
}
