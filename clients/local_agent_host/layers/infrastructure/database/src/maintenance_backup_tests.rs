// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{maintenance, migration, ClientStorageError, SqliteClientStorage};
use sqlx::{sqlite::SqliteConnectOptions, Connection, SqliteConnection};
use std::path::{Path, PathBuf};

fn backup_files(directory: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.to_string_lossy().contains(".pre-migration-"))
        .collect()
}

#[tokio::test]
async fn verified_backup_contains_uncheckpointed_wal_and_survives_reopen() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("client.sqlite");
    let storage = SqliteClientStorage::connect_file(&path).await.unwrap();
    let mut writer = storage.pool.acquire().await.unwrap();
    sqlx::query("PRAGMA wal_autocheckpoint = 0")
        .execute(&mut *writer)
        .await
        .unwrap();
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&mut *writer)
        .await
        .unwrap();
    sqlx::query("CREATE TABLE backup_wal_probe(value TEXT NOT NULL)")
        .execute(&mut *writer)
        .await
        .unwrap();
    sqlx::query("INSERT INTO backup_wal_probe VALUES('committed-only-in-wal')")
        .execute(&mut *writer)
        .await
        .unwrap();
    assert!(
        std::fs::metadata(directory.path().join("client.sqlite-wal"))
            .unwrap()
            .len()
            > 0
    );
    let backup = maintenance::create_migration_backup(
        &storage.pool,
        &path,
        migration::SCHEMA_VERSION,
        migration::SCHEMA_VERSION + 1,
    )
    .await
    .unwrap();
    let mut snapshot = SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(&backup)
            .read_only(true),
    )
    .await
    .unwrap();
    let value: String = sqlx::query_scalar("SELECT value FROM backup_wal_probe")
        .fetch_one(&mut snapshot)
        .await
        .unwrap();
    assert_eq!(value, "committed-only-in-wal");
    let integrity: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(&mut snapshot)
        .await
        .unwrap();
    assert_eq!(integrity, "ok");
    snapshot.close().await.unwrap();
    assert_eq!(backup_files(directory.path()), vec![backup.clone()]);
    let bytes = std::fs::read(&backup).unwrap();
    drop(writer);
    storage.pool.close().await;
    let reopened = SqliteClientStorage::connect_file(&path).await.unwrap();
    assert_eq!(backup_files(directory.path()), vec![backup.clone()]);
    assert_eq!(std::fs::read(&backup).unwrap(), bytes);
    reopened.pool.close().await;
}

#[tokio::test]
async fn recovery_capacity_defers_migration_without_deleting_or_changing_source() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("client.sqlite");
    let storage = SqliteClientStorage::connect_file(&path).await.unwrap();
    {
        let mut connection = storage.pool.acquire().await.unwrap();
        super::migration_tests::rewind_receipt_v35(&mut connection).await;
    }
    for statement in [
        "DROP TRIGGER local_agent_step_memory_discard",
        "DROP TABLE local_agent_step_memory",
        "DELETE FROM client_schema_migrations WHERE version >= 34",
    ] {
        sqlx::query(statement).execute(&storage.pool).await.unwrap();
    }
    storage.pool.close().await;
    for index in 0..8 {
        let suffix = if index % 2 == 0 { "" } else { ".incomplete" };
        std::fs::write(
            directory.path().join(format!(
                "client.sqlite.pre-migration-sentinel-{index}.backup.sqlite{suffix}"
            )),
            b"preserve for explicit recovery assessment",
        )
        .unwrap();
    }
    let error = SqliteClientStorage::connect_file(&path).await.unwrap_err();
    assert!(matches!(error, ClientStorageError::MaintenanceRequired(_)));
    assert_eq!(error.code(), "storage_maintenance_required");
    assert!(!error.retryable());
    assert_eq!(backup_files(directory.path()).len(), 8);
    for backup in backup_files(directory.path()) {
        assert_eq!(
            std::fs::read(backup).unwrap(),
            b"preserve for explicit recovery assessment"
        );
    }
    let mut source = SqliteConnection::connect_with(
        &SqliteConnectOptions::new().filename(&path).read_only(true),
    )
    .await
    .unwrap();
    let version: i64 = sqlx::query_scalar("SELECT MAX(version) FROM client_schema_migrations")
        .fetch_one(&mut source)
        .await
        .unwrap();
    assert_eq!(version, 33);
    let migrated_table: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE name = 'local_agent_step_memory'",
    )
    .fetch_one(&mut source)
    .await
    .unwrap();
    assert_eq!(migrated_table, 0);
    source.close().await.unwrap();

    // Explicitly clear one test-owned incomplete slot; the next attempt can migrate, remove known
    // incomplete artifacts and retain its newly verified snapshot. Unknown complete names remain
    // for manual assessment. A current-schema reopen must not create another snapshot.
    std::fs::remove_file(
        directory
            .path()
            .join("client.sqlite.pre-migration-sentinel-1.backup.sqlite.incomplete"),
    )
    .unwrap();
    let migrated = SqliteClientStorage::connect_file(&path).await.unwrap();
    let retained = backup_files(directory.path());
    assert_eq!(retained.len(), 5);
    assert_eq!(
        retained
            .iter()
            .filter(|path| path
                .file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|name| name.contains("sentinel")))
            .count(),
        4,
        "unknown complete recovery names must remain for manual assessment"
    );
    assert!(retained.iter().any(|path| path
        .file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|name| name.contains("-to-v") && name.ends_with(".backup.sqlite"))));
    migrated.pool.close().await;
    let reopened = SqliteClientStorage::connect_file(&path).await.unwrap();
    assert_eq!(backup_files(directory.path()), retained);
    reopened.pool.close().await;
}

#[tokio::test]
async fn successful_migration_keeps_new_snapshot_and_prunes_superseded_recovery_files() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("client.sqlite");
    let storage = SqliteClientStorage::connect_file(&path).await.unwrap();
    {
        let mut connection = storage.pool.acquire().await.unwrap();
        super::migration_tests::rewind_receipt_v35(&mut connection).await;
    }
    storage.pool.close().await;

    let old_complete = directory
        .path()
        .join("client.sqlite.pre-migration-v34-to-v37-old.backup.sqlite");
    let old_incomplete = directory
        .path()
        .join("client.sqlite.pre-migration-v34-to-v37-old.backup.sqlite.incomplete");
    let old_journal = directory
        .path()
        .join("client.sqlite.pre-migration-v34-to-v37-old.backup.sqlite.incomplete-journal");
    let unrelated = directory.path().join("client.before-manual-repair.sqlite");
    for file in [&old_complete, &old_incomplete, &old_journal, &unrelated] {
        std::fs::write(file, b"preserve until migration succeeds").unwrap();
    }

    let migrated = SqliteClientStorage::connect_file(&path).await.unwrap();
    let backups = backup_files(directory.path());
    assert_eq!(backups.len(), 1);
    assert!(backups[0]
        .file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|name| name.ends_with(".backup.sqlite")));
    assert_ne!(backups[0], old_complete);
    assert!(!old_complete.exists());
    assert!(!old_incomplete.exists());
    assert!(!old_journal.exists());
    assert_eq!(
        std::fs::read(unrelated).unwrap(),
        b"preserve until migration succeeds"
    );
    migrated.pool.close().await;
}

#[tokio::test]
async fn schema_change_during_backup_rejects_snapshot_without_removing_old_recovery() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("client.sqlite");
    let storage = SqliteClientStorage::connect_file(&path).await.unwrap();
    let original = maintenance::create_migration_backup(
        &storage.pool,
        &path,
        migration::SCHEMA_VERSION,
        migration::SCHEMA_VERSION + 1,
    )
    .await
    .unwrap();
    let bytes = std::fs::read(&original).unwrap();
    let error = maintenance::create_migration_backup(
        &storage.pool,
        &path,
        migration::SCHEMA_VERSION - 1,
        migration::SCHEMA_VERSION,
    )
    .await
    .unwrap_err();
    assert!(matches!(error, ClientStorageError::MaintenanceRequired(_)));
    assert_eq!(backup_files(directory.path()), vec![original.clone()]);
    assert_eq!(std::fs::read(&original).unwrap(), bytes);
    storage.pool.close().await;
}

#[tokio::test]
async fn existing_unversioned_database_gets_a_recoverable_snapshot() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("client.sqlite");
    let mut source = SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true),
    )
    .await
    .unwrap();
    sqlx::query("CREATE TABLE preexisting_data(value TEXT NOT NULL)")
        .execute(&mut source)
        .await
        .unwrap();
    sqlx::query("INSERT INTO preexisting_data VALUES('retain')")
        .execute(&mut source)
        .await
        .unwrap();
    source.close().await.unwrap();
    let storage = SqliteClientStorage::connect_file(&path).await.unwrap();
    let backups = backup_files(directory.path());
    assert_eq!(backups.len(), 1);
    let mut snapshot = SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(&backups[0])
            .read_only(true),
    )
    .await
    .unwrap();
    let value: String = sqlx::query_scalar("SELECT value FROM preexisting_data")
        .fetch_one(&mut snapshot)
        .await
        .unwrap();
    assert_eq!(value, "retain");
    let migrations: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE name = 'client_schema_migrations'",
    )
    .fetch_one(&mut snapshot)
    .await
    .unwrap();
    assert_eq!(migrations, 0);
    snapshot.close().await.unwrap();
    storage.pool.close().await;
}
