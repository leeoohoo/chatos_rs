// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{ClientStorageError, SqliteResultExt};
use sqlx::{sqlite::SqliteConnectOptions, Connection, SqliteConnection, SqlitePool};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use uuid::Uuid;

pub(super) fn create_database_parent(path: &Path) -> Result<(), ClientStorageError> {
    if let Some(parent) = path.parent().filter(|value| !value.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|error| {
            ClientStorageError::InvalidState(format!(
                "create client storage directory failed: {error}"
            ))
        })?;
    }
    Ok(())
}

pub(super) fn temporary_artifact_root(
) -> Result<(PathBuf, Arc<tempfile::TempDir>), ClientStorageError> {
    let temporary_root = Arc::new(
        tempfile::Builder::new()
            .prefix("chatos-local-agent-")
            .tempdir()
            .map_err(ClientStorageError::database)?,
    );
    Ok((temporary_root.path().join("artifacts"), temporary_root))
}

pub(super) fn create_private_directory(path: &Path) -> Result<(), ClientStorageError> {
    std::fs::create_dir_all(path).map_err(ClientStorageError::database)?;
    let metadata = std::fs::symlink_metadata(path).map_err(ClientStorageError::database)?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err(ClientStorageError::InvalidState(format!(
            "private storage root is not a directory: {}",
            path.display()
        )));
    }
    restrict_directory_permissions(path)
}

pub(super) fn remove_stale_artifact_staging_files(root: &Path) -> Result<(), ClientStorageError> {
    remove_stale_artifact_staging_files_at(root, SystemTime::now())
}

fn remove_stale_artifact_staging_files_at(
    root: &Path,
    now: SystemTime,
) -> Result<(), ClientStorageError> {
    const MAXIMUM_AGE: Duration = Duration::from_secs(24 * 60 * 60);
    let entries = std::fs::read_dir(root).map_err(ClientStorageError::database)?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !is_artifact_staging_name(name) {
            continue;
        }
        let Ok(metadata) = std::fs::symlink_metadata(entry.path()) else {
            continue;
        };
        if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
            continue;
        }
        let Ok(modified) = metadata.modified() else {
            continue;
        };
        if now
            .duration_since(modified)
            .is_ok_and(|age| age >= MAXIMUM_AGE)
        {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    Ok(())
}

fn is_artifact_staging_name(name: &str) -> bool {
    let token = if let Some(token) = name.strip_suffix(".staged") {
        token
    } else if let Some(token) = name
        .strip_prefix('.')
        .and_then(|value| value.strip_suffix(".tmp"))
    {
        token
    } else {
        return false;
    };
    token.len() == 36
        && token.bytes().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => byte == b'-',
            _ => byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte),
        })
}

#[cfg(unix)]
fn restrict_directory_permissions(path: &Path) -> Result<(), ClientStorageError> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = std::fs::metadata(path)
        .map_err(ClientStorageError::database)?
        .permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(path, permissions).map_err(ClientStorageError::database)
}

#[cfg(not(unix))]
fn restrict_directory_permissions(_path: &Path) -> Result<(), ClientStorageError> {
    Ok(())
}

pub(super) async fn verify_integrity(pool: &SqlitePool) -> Result<(), ClientStorageError> {
    let mut connection = pool.acquire().await.db()?;
    verify_connection(&mut connection).await
}

pub(super) async fn verify_available(pool: &SqlitePool) -> Result<(), ClientStorageError> {
    let mut connection = pool.acquire().await.db()?;
    let _: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM client_schema_migrations")
        .fetch_one(&mut *connection)
        .await
        .db()?;
    Ok(())
}

pub(super) async fn create_migration_backup(
    pool: &SqlitePool,
    database_path: &Path,
    from_version: i64,
    to_version: i64,
) -> Result<PathBuf, ClientStorageError> {
    super::migration_backup_policy::preflight(pool, database_path).await?;
    let backup_path = backup_path(database_path, from_version, to_version)?;
    // Only verified, synced snapshots receive the recovery-file suffix. A crash
    // during VACUUM leaves an explicitly incomplete file for later maintenance.
    let mut staging_name = backup_path.as_os_str().to_os_string();
    staging_name.push(".incomplete");
    let staging_path = PathBuf::from(staging_name);
    let backup_text = staging_path.to_str().ok_or_else(|| {
        ClientStorageError::InvalidState(
            "client storage backup path must be valid UTF-8".to_string(),
        )
    })?;
    let mut file_options = std::fs::OpenOptions::new();
    file_options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        file_options.mode(0o600);
    }
    // VACUUM INTO accepts an existing empty file. Reserve it privately first,
    // avoiding the interval where SQLite's default umask could expose data.
    let reserved_file = file_options
        .open(&staging_path)
        .map_err(ClientStorageError::database)?;
    drop(reserved_file);
    #[cfg(all(test, unix))]
    super::migration_crash_tests::checkpoint("backup_reserved").await;
    let mut connection = pool.acquire().await.db()?;
    #[cfg(all(test, unix))]
    super::migration_crash_tests::install_progress_checkpoint(
        &mut connection,
        "backup_vacuum_inside",
    )
    .await;
    let backup_result = sqlx::query("VACUUM INTO ?")
        .bind(backup_text)
        .execute(&mut *connection)
        .await
        .db();
    drop(connection);
    if let Err(error) = backup_result {
        remove_incomplete_backup(&staging_path);
        return Err(error);
    }
    #[cfg(all(test, unix))]
    super::migration_crash_tests::checkpoint("backup_written").await;
    if let Err(error) = restrict_file_permissions(&staging_path) {
        remove_incomplete_backup(&staging_path);
        return Err(error);
    }

    let options = SqliteConnectOptions::new()
        .filename(&staging_path)
        .read_only(true)
        .foreign_keys(true);
    let mut backup = match SqliteConnection::connect_with(&options).await.db() {
        Ok(connection) => connection,
        Err(error) => {
            remove_incomplete_backup(&staging_path);
            return Err(error);
        }
    };
    let verification = async {
        // Recovery snapshots are infrequent; validate indexes as well as pages
        // before publishing, instead of relying on startup's quicker check.
        let integrity: String = sqlx::query_scalar("PRAGMA integrity_check(1)")
            .fetch_one(&mut backup)
            .await
            .db()?;
        if integrity != "ok" {
            return Err(ClientStorageError::InvalidState(format!(
                "SQLite recovery snapshot integrity_check failed: {integrity}"
            )));
        }
        let has_migrations: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master \
             WHERE type = 'table' AND name = 'client_schema_migrations'",
        )
        .fetch_one(&mut backup)
        .await
        .db()?;
        let snapshot_version: i64 = if has_migrations == 0 {
            0
        } else {
            sqlx::query_scalar("SELECT COALESCE(MAX(version), 0) FROM client_schema_migrations")
                .fetch_one(&mut backup)
                .await
                .db()?
        };
        if snapshot_version != from_version {
            return Err(ClientStorageError::MaintenanceRequired(format!(
                "database schema changed during backup: expected {from_version}, \
                 found {snapshot_version}; migration deferred"
            )));
        }
        Ok(())
    }
    .await;
    if let Err(error) = verification {
        let _ = backup.close().await;
        remove_incomplete_backup(&staging_path);
        return Err(error);
    }
    backup.close().await.db()?;
    std::fs::OpenOptions::new()
        .write(true)
        .open(&staging_path)
        .and_then(|file| file.sync_all())
        .map_err(ClientStorageError::database)?;
    std::fs::rename(&staging_path, &backup_path).map_err(ClientStorageError::database)?;
    #[cfg(unix)]
    {
        let parent = backup_path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        std::fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(ClientStorageError::database)?;
    }
    // Do not remove this backup on migration success, failure, or future opens.
    // Verified recovery points need an explicit retention/rollback policy.
    #[cfg(all(test, unix))]
    super::migration_crash_tests::checkpoint("backup_published").await;
    Ok(backup_path)
}

/// After a migration is fully committed and verified, keep its newly published snapshot and
/// best-effort remove older snapshots for the same database. Failures deliberately leave extra
/// recovery files in place; the next migration's count and byte admission guard will fail closed.
pub(super) fn prune_superseded_migration_backups(database_path: &Path, retained_backup: &Path) {
    let Some(parent) = database_path.parent() else {
        return;
    };
    let Some(file_name) = database_path.file_name().and_then(|value| value.to_str()) else {
        return;
    };
    let prefix = format!("{file_name}.pre-migration-");
    let Ok(entries) = std::fs::read_dir(parent) else {
        return;
    };
    let retained_from_version = retained_backup
        .file_name()
        .and_then(|value| value.to_str())
        .and_then(|name| migration_backup_from_version(name, &prefix));
    let mut earliest_compatible: Option<(i64, SystemTime, PathBuf)> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        if path == retained_backup {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Some(from_version) = migration_backup_from_version(&name, &prefix) else {
            continue;
        };
        if retained_from_version.is_none_or(|retained| from_version >= retained) {
            continue;
        }
        let Ok(metadata) = path.symlink_metadata() else {
            continue;
        };
        if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
            continue;
        }
        let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let replace = earliest_compatible
            .as_ref()
            .is_none_or(|(version, timestamp, _)| {
                from_version < *version || (from_version == *version && modified > *timestamp)
            });
        if replace {
            earliest_compatible = Some((from_version, modified, path));
        }
    }
    let earliest_compatible = earliest_compatible.map(|(_, _, path)| path);
    let Ok(entries) = std::fs::read_dir(parent) else {
        return;
    };
    let mut removed_any = false;
    for entry in entries.flatten() {
        let path = entry.path();
        if path == retained_backup || earliest_compatible.as_ref() == Some(&path) {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let is_complete = name.starts_with(&prefix) && name.ends_with(".backup.sqlite");
        let is_recovery_artifact = is_complete
            || (name.starts_with(&prefix)
                && (name.ends_with(".backup.sqlite.incomplete")
                    || name.ends_with(".backup.sqlite.incomplete-journal")));
        if !is_recovery_artifact {
            continue;
        }
        // An unrecognized complete file may come from a future/older naming scheme or a manual
        // recovery workflow. Never delete it automatically. Incomplete files are never recovery
        // points and remain safe to discard after a successful migration.
        if is_complete && migration_backup_from_version(&name, &prefix).is_none() {
            continue;
        }
        let Ok(metadata) = path.symlink_metadata() else {
            continue;
        };
        if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
            continue;
        }
        if std::fs::remove_file(path).is_ok() {
            removed_any = true;
        }
    }
    if removed_any {
        let _ = std::fs::File::open(parent).and_then(|directory| directory.sync_all());
    }
}

fn migration_backup_from_version(name: &str, prefix: &str) -> Option<i64> {
    let versioned = name
        .strip_prefix(prefix)?
        .strip_suffix(".backup.sqlite")?
        .strip_prefix('v')?;
    let (from_version, _) = versioned.split_once("-to-v")?;
    from_version.parse().ok()
}

pub(super) async fn compact_database(pool: &SqlitePool) -> Result<(), ClientStorageError> {
    let mut connection = pool.acquire().await.db()?;
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&mut *connection)
        .await
        .db()?;
    #[cfg(all(test, unix))]
    super::migration_crash_tests::install_progress_checkpoint(
        &mut connection,
        "main_vacuum_inside",
    )
    .await;
    sqlx::query("VACUUM").execute(&mut *connection).await.db()?;
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&mut *connection)
        .await
        .db()?;
    Ok(())
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

#[cfg(test)]
mod artifact_staging_tests {
    use super::*;

    #[test]
    fn cleanup_removes_only_old_regular_staging_files() {
        let root = tempfile::tempdir().expect("temporary directory");
        let token = "01234567-89ab-cdef-0123-456789abcdef";
        let staged = root.path().join(format!("{token}.staged"));
        let temporary = root.path().join(format!(".{token}.tmp"));
        let unknown = root.path().join("keep.txt");
        std::fs::write(&staged, b"staged").expect("write staged");
        std::fs::write(&temporary, b"temporary").expect("write temporary");
        std::fs::write(&unknown, b"unknown").expect("write unknown");
        let future = SystemTime::now() + Duration::from_secs(25 * 60 * 60);

        remove_stale_artifact_staging_files_at(root.path(), future).expect("cleanup staging");

        assert!(!staged.exists());
        assert!(!temporary.exists());
        assert!(unknown.exists());
    }
}
