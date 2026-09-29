// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{ClientStorageError, SqliteResultExt};
use sqlx::{sqlite::SqliteConnectOptions, Connection, SqliteConnection, SqlitePool};
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub(super) async fn verify_integrity(pool: &SqlitePool) -> Result<(), ClientStorageError> {
    let mut connection = pool.acquire().await.db()?;
    verify_connection(&mut connection).await
}

pub(super) async fn create_migration_backup(
    pool: &SqlitePool,
    database_path: &Path,
    from_version: i64,
    to_version: i64,
) -> Result<PathBuf, ClientStorageError> {
    let backup_path = backup_path(database_path, from_version, to_version)?;
    let backup_text = backup_path.to_str().ok_or_else(|| {
        ClientStorageError::InvalidState(
            "client storage backup path must be valid UTF-8".to_string(),
        )
    })?;
    let mut connection = pool.acquire().await.db()?;
    let backup_result = sqlx::query("VACUUM INTO ?")
        .bind(backup_text)
        .execute(&mut *connection)
        .await
        .db();
    drop(connection);
    if let Err(error) = backup_result {
        remove_incomplete_backup(&backup_path);
        return Err(error);
    }
    if let Err(error) = restrict_file_permissions(&backup_path) {
        remove_incomplete_backup(&backup_path);
        return Err(error);
    }

    let options = SqliteConnectOptions::new()
        .filename(&backup_path)
        .read_only(true)
        .foreign_keys(true);
    let mut backup = match SqliteConnection::connect_with(&options).await.db() {
        Ok(connection) => connection,
        Err(error) => {
            remove_incomplete_backup(&backup_path);
            return Err(error);
        }
    };
    if let Err(error) = verify_connection(&mut backup).await {
        let _ = backup.close().await;
        remove_incomplete_backup(&backup_path);
        return Err(error);
    }
    backup.close().await.db()?;
    Ok(backup_path)
}

#[cfg(unix)]
pub(super) fn restrict_file_permissions(path: &Path) -> Result<(), ClientStorageError> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = std::fs::metadata(path)
        .map_err(ClientStorageError::database)?
        .permissions();
    permissions.set_mode(0o600);
    std::fs::set_permissions(path, permissions).map_err(ClientStorageError::database)
}

#[cfg(not(unix))]
pub(super) fn restrict_file_permissions(_path: &Path) -> Result<(), ClientStorageError> {
    Ok(())
}

async fn verify_connection(connection: &mut SqliteConnection) -> Result<(), ClientStorageError> {
    let result: String = sqlx::query_scalar("PRAGMA quick_check(1)")
        .fetch_one(&mut *connection)
        .await
        .db()?;
    if result != "ok" {
        return Err(ClientStorageError::InvalidState(format!(
            "SQLite quick_check failed: {result}"
        )));
    }
    Ok(())
}

fn backup_path(
    database_path: &Path,
    from_version: i64,
    to_version: i64,
) -> Result<PathBuf, ClientStorageError> {
    let file_name = database_path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| {
            ClientStorageError::InvalidState(
                "client storage path must have a valid UTF-8 file name".to_string(),
            )
        })?;
    Ok(database_path.with_file_name(format!(
        "{file_name}.pre-migration-v{from_version}-to-v{to_version}-{}.backup.sqlite",
        Uuid::new_v4()
    )))
}

fn remove_incomplete_backup(path: &Path) {
    // Preserve the primary database error; this path is a generated,
    // incomplete artifact and is never used for recovery.
    let _ = std::fs::remove_file(path);
}
