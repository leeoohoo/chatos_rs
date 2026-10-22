// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

//! Conservative startup guard, not the exclusive online-maintenance protocol.
//! Never delete a recovery point to obtain space for its replacement.

use super::{ClientStorageError, SqliteResultExt};
use sqlx::SqlitePool;
use std::path::Path;

const MAX_RECOVERY_FILES: usize = 8;
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
    for entry in std::fs::read_dir(parent).map_err(ClientStorageError::database)? {
        let entry = entry.map_err(ClientStorageError::database)?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(prefix.as_ref())
            && (name.ends_with(".backup.sqlite") || name.ends_with(".backup.sqlite.incomplete"))
        {
            recovery_files += 1;
        }
    }
    check_budget(database_bytes, wal_bytes, available_bytes, recovery_files)
}

fn check_budget(
    database_bytes: u64,
    wal_bytes: u64,
    available_bytes: u64,
    recovery_files: usize,
) -> Result<(), ClientStorageError> {
    if recovery_files >= MAX_RECOVERY_FILES {
        return Err(ClientStorageError::MaintenanceRequired(format!(
            "migration deferred: {recovery_files} recovery files already exist (limit \
             {MAX_RECOVERY_FILES}); verify and archive recovery files before retrying; \
             no recovery files were deleted"
        )));
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
        let error = check_budget(1024, 512, required - 1, 0).unwrap_err();
        assert_eq!(error.code(), "storage_maintenance_required");
        assert!(!error.retryable());
        assert!(check_budget(1024, 512, required, 0).is_ok());
    }

    #[test]
    fn exhausted_recovery_slots_fail_closed_even_with_free_space() {
        assert!(check_budget(1, 0, u64::MAX, MAX_RECOVERY_FILES - 1).is_ok());
        assert!(matches!(
            check_budget(1, 0, u64::MAX, MAX_RECOVERY_FILES),
            Err(ClientStorageError::MaintenanceRequired(_))
        ));
    }

    #[test]
    fn overflowing_estimate_cannot_bypass_admission() {
        assert!(check_budget(u64::MAX, 0, u64::MAX, 0).is_err());
        assert!(check_budget(0, u64::MAX, u64::MAX, 0).is_err());
    }
}
