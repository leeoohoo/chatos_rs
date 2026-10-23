// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

//! Conservative startup guard, not the exclusive online-maintenance protocol.
//! Never delete a recovery point to obtain space for its replacement.

use super::{ClientStorageError, SqliteResultExt};
use sqlx::SqlitePool;
use std::path::Path;

const MAX_RECOVERY_FILES: usize = 8;
const MIN_RECOVERY_TOTAL_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const MAX_DATABASE_SIZED_RECOVERY_FILES: u64 = 2;
const SAFETY_MARGIN_BYTES: u64 = 64 * 1024 * 1024;

pub(super) async fn preflight(
    pool: &SqlitePool,
    database_path: &Path,
) -> Result<(), ClientStorageError> {
    let page_count: i64 = sqlx::query_scalar("PRAGMA page_count")
        .fetch_one(pool)
        .await
        .db()?;
    let page_size: i64 = sqlx::query_scalar("PRAGMA page_size")
        .fetch_one(pool)
        .await
        .db()?;
    let logical_bytes = u64::try_from(page_count)
        .ok()
        .zip(u64::try_from(page_size).ok())
        .and_then(|(count, size)| count.checked_mul(size))
        .ok_or_else(|| ClientStorageError::InvalidState("invalid SQLite page size/count".into()))?;
    let database_bytes = std::fs::metadata(database_path)
        .map_err(ClientStorageError::database)?
        .len()
        .max(logical_bytes);
    let mut wal_path = database_path.as_os_str().to_os_string();
    wal_path.push("-wal");
    let wal_bytes = match std::fs::metadata(Path::new(&wal_path)) {
        Ok(metadata) => metadata.len(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
        Err(error) => return Err(ClientStorageError::database(error)),
    };
    let parent = database_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    // The backup and any rewrite are placed on this volume, not the temp volume.
    let available_bytes = fs2::available_space(parent).map_err(ClientStorageError::database)?;
    let file_name = database_path
        .file_name()
        .ok_or_else(|| ClientStorageError::InvalidState("database path has no file name".into()))?;
    let mut prefix = file_name.to_os_string();
    prefix.push(".pre-migration-");
    let prefix = prefix.to_string_lossy();
    let mut recovery_files = 0;
    let mut recovery_bytes = 0_u64;
    for entry in std::fs::read_dir(parent).map_err(ClientStorageError::database)? {
        let entry = entry.map_err(ClientStorageError::database)?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(prefix.as_ref())
            && (name.ends_with(".backup.sqlite") || name.ends_with(".backup.sqlite.incomplete"))
        {
            let metadata = entry
                .path()
                .symlink_metadata()
                .map_err(ClientStorageError::database)?;
            if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
                return Err(ClientStorageError::MaintenanceRequired(format!(
                    "migration deferred: recovery path is not a regular file: {}",
                    entry.path().display()
                )));
            }
            recovery_files += 1;
            recovery_bytes = recovery_bytes.checked_add(metadata.len()).ok_or_else(|| {
                ClientStorageError::MaintenanceRequired(
                    "migration recovery byte accounting overflow".into(),
                )
            })?;
        }
    }
    check_budget(
        database_bytes,
        wal_bytes,
        available_bytes,
        recovery_files,
        recovery_bytes,
    )
}

fn check_budget(
    database_bytes: u64,
    wal_bytes: u64,
    available_bytes: u64,
    recovery_files: usize,
    recovery_bytes: u64,
) -> Result<(), ClientStorageError> {
    if recovery_files >= MAX_RECOVERY_FILES {
        return Err(ClientStorageError::MaintenanceRequired(format!(
            "migration deferred: {recovery_files} recovery files already exist (limit \
             {MAX_RECOVERY_FILES}); verify and archive recovery files before retrying; \
             no recovery files were deleted"
        )));
    }
    if recovery_files > 0 {
        let recovery_budget = database_bytes
            .checked_mul(MAX_DATABASE_SIZED_RECOVERY_FILES)
            .map(|bytes| bytes.max(MIN_RECOVERY_TOTAL_BYTES))
            .ok_or_else(|| {
                ClientStorageError::MaintenanceRequired(
                    "migration recovery byte budget overflow".into(),
                )
            })?;
        let prospective_recovery_bytes =
            recovery_bytes.checked_add(database_bytes).ok_or_else(|| {
                ClientStorageError::MaintenanceRequired(
                    "migration recovery byte estimate overflow".into(),
                )
            })?;
        if prospective_recovery_bytes > recovery_budget {
            return Err(ClientStorageError::MaintenanceRequired(format!(
                "migration deferred: {recovery_files} recovery files already use \
                 {recovery_bytes} bytes; another database-sized snapshot would use at least \
                 {prospective_recovery_bytes} bytes and exceed the {recovery_budget}-byte \
                 recovery budget; verify and retain only the \
                 required recovery point before retrying; no recovery files were deleted"
            )));
        }
    }
    // Reserve a backup, a database-sized rewrite, database-sized WAL growth,
    // the existing WAL and a fixed margin. This is a lower-bound admission
    // guard: arbitrary schema growth/concurrent writers still require a lease
    // and a migration-specific estimate; free space is not reserved by this check.
    let required_bytes = database_bytes
        .checked_mul(3)
        .and_then(|bytes| bytes.checked_add(wal_bytes))
        .and_then(|bytes| bytes.checked_add(SAFETY_MARGIN_BYTES))
        .ok_or_else(|| {
            ClientStorageError::MaintenanceRequired("migration size estimate overflow".into())
        })?;
    if available_bytes < required_bytes {
        return Err(ClientStorageError::MaintenanceRequired(format!(
            "migration deferred: need at least {required_bytes} free bytes on the database \
             volume, available {available_bytes}; existing database and recovery files retained"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_includes_backup_rewrite_wal_and_margin() {
        let required = 3 * 1024 + 512 + SAFETY_MARGIN_BYTES;
        let error = check_budget(1024, 512, required - 1, 0, 0).unwrap_err();
        assert_eq!(error.code(), "storage_maintenance_required");
        assert!(!error.retryable());
        assert!(check_budget(1024, 512, required, 0, 0).is_ok());
    }

    #[test]
    fn exhausted_recovery_slots_fail_closed_even_with_free_space() {
        assert!(check_budget(1, 0, u64::MAX, MAX_RECOVERY_FILES - 1, 0).is_ok());
        assert!(matches!(
            check_budget(1, 0, u64::MAX, MAX_RECOVERY_FILES, 0),
            Err(ClientStorageError::MaintenanceRequired(_))
        ));
    }

    #[test]
    fn recovery_byte_budget_allows_two_large_snapshots_but_bounds_retries() {
        let large_database = MIN_RECOVERY_TOTAL_BYTES + 1;
        assert!(check_budget(large_database, 0, u64::MAX, 0, 0).is_ok());
        assert!(check_budget(large_database, 0, u64::MAX, 1, large_database).is_ok());
        assert!(check_budget(large_database, 0, u64::MAX, 2, large_database * 2).is_err());

        let existing = 6 * 1024 * 1024 * 1024;
        let next_database = 3 * 1024 * 1024 * 1024;
        let error = check_budget(next_database, 0, u64::MAX, 1, existing).unwrap_err();
        assert_eq!(error.code(), "storage_maintenance_required");
        assert!(!error.retryable());
        assert!(error.to_string().contains("recovery budget"));
    }

    #[test]
    fn overflowing_estimate_cannot_bypass_admission() {
        assert!(check_budget(u64::MAX, 0, u64::MAX, 0, 0).is_err());
        assert!(check_budget(0, u64::MAX, u64::MAX, 0, 0).is_err());
        assert!(check_budget(1, 0, u64::MAX, 1, u64::MAX).is_err());
    }
}
