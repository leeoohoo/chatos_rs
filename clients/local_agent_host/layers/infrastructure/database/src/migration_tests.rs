// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{schema, SqliteClientStorage};
use sqlx::{sqlite::SqliteConnectOptions, Connection, Row, SqliteConnection};
use uuid::Uuid;

#[tokio::test]
async fn version_nineteen_discards_ownerless_control_plane_snapshots() {
    let database_path = std::env::temp_dir().join(format!(
        "chatos-local-control-plane-migration-{}.sqlite",
        Uuid::new_v4()
    ));
    let options = SqliteConnectOptions::new()
        .filename(&database_path)
        .create_if_missing(true)
        .foreign_keys(true);
    let mut connection = SqliteConnection::connect_with(&options)
        .await
        .expect("legacy connection");
    sqlx::query(
        "CREATE TABLE client_schema_migrations (\
         version INTEGER PRIMARY KEY NOT NULL, applied_at_unix_ms INTEGER NOT NULL)",
    )
    .execute(&mut connection)
    .await
    .expect("migration table");
    let legacy_schemas: &[(i64, &[&str])] = &[
        (1, schema::SCHEMA_V1),
        (2, schema::SCHEMA_V2),
        (3, schema::SCHEMA_V3),
        (4, schema::SCHEMA_V4),
        (5, schema::SCHEMA_V5),
        (6, schema::SCHEMA_V6),
        (7, schema::SCHEMA_V7),
        (8, schema::SCHEMA_V8),
        (9, schema::SCHEMA_V9),
        (10, schema::SCHEMA_V10),
        (11, schema::SCHEMA_V11),
        (12, schema::SCHEMA_V12),
        (13, schema::SCHEMA_V13),
        (14, schema::SCHEMA_V14),
        (15, schema::SCHEMA_V15),
        (16, schema::SCHEMA_V16),
        (17, schema::SCHEMA_V17),
        (18, schema::SCHEMA_V18),
    ];
    for (version, statements) in legacy_schemas {
        for statement in *statements {
            sqlx::query(statement)
                .execute(&mut connection)
                .await
                .expect("legacy schema statement");
        }
        sqlx::query(
            "INSERT INTO client_schema_migrations(version, applied_at_unix_ms) VALUES(?, ?)",
        )
        .bind(version)
        .bind(version * 1_000)
        .execute(&mut connection)
        .await
        .expect("legacy migration record");
    }
    sqlx::query(
        "INSERT INTO local_capability_policy_snapshots(\
         profile_key, capability_policy_revision, instructions, prefixed_input_items_json, \
         tools_json, created_at_unix_ms) VALUES('main_chat', 'policy-1', 'old', '[]', '[]', 1)",
    )
    .execute(&mut connection)
    .await
    .expect("legacy capability");
    sqlx::query(
        "INSERT INTO local_model_config_snapshots(\
         model_config_ref, model_config_revision, credential_ref, base_url, model, provider, \
         supports_responses, supports_images, include_prompt_cache_retention, \
         created_at_unix_ms) VALUES(\
         'default', 'revision-1', 'env:MODEL_KEY', 'https://example.test', 'old-model', \
         'openai', 1, 1, 0, 1)",
    )
    .execute(&mut connection)
    .await
    .expect("legacy model");
    sqlx::query(
        "INSERT INTO local_agent_command_receipts(\
         command_id, request_fingerprint, response_json, created_at_unix_ms) VALUES\
         ('internal-control-plane-model:default:revision-1', 'old', '{}', 1),\
         ('unrelated-command', 'other', '{}', 1)",
    )
    .execute(&mut connection)
    .await
    .expect("legacy receipts");
    connection.close().await.expect("close legacy database");

    let storage = SqliteClientStorage::connect_file(&database_path)
        .await
        .expect("migrate storage");
    let backup_paths = migration_backups(&database_path);
    assert_eq!(backup_paths.len(), 1);
    let backup_path = backup_paths.into_iter().next().expect("migration backup");
    assert!(backup_path
        .file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|name| name.contains(".pre-migration-v18-to-v24-")));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        assert_eq!(
            std::fs::metadata(&database_path)
                .expect("database metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(&backup_path)
                .expect("backup metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    let backup_options = SqliteConnectOptions::new()
        .filename(&backup_path)
        .read_only(true)
        .foreign_keys(true);
    let mut backup = SqliteConnection::connect_with(&backup_options)
        .await
        .expect("open migration backup");
    let backup_check: String = sqlx::query_scalar("PRAGMA quick_check(1)")
        .fetch_one(&mut backup)
        .await
        .expect("check migration backup");
    assert_eq!(backup_check, "ok");
    let backup_version: i64 =
        sqlx::query_scalar("SELECT MAX(version) FROM client_schema_migrations")
            .fetch_one(&mut backup)
            .await
            .expect("backup schema version");
    assert_eq!(backup_version, 18);
    let legacy_snapshot_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM local_model_config_snapshots")
            .fetch_one(&mut backup)
            .await
            .expect("backup legacy snapshots");
    assert_eq!(legacy_snapshot_count, 1);
    backup.close().await.expect("close migration backup");
    for table in [
        "local_capability_policy_snapshots",
        "local_model_config_snapshots",
    ] {
        let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
            .fetch_one(&storage.pool)
            .await
            .expect("snapshot count");
        assert_eq!(
            count, 0,
            "legacy {table} rows must not be assigned an owner"
        );
    }
    assert_eq!(
        primary_key_columns(&storage, "local_capability_policy_snapshots").await,
        vec!["owner_user_id", "profile_key", "capability_policy_revision"]
    );
    assert_eq!(
        primary_key_columns(&storage, "local_model_config_snapshots").await,
        vec!["owner_user_id", "model_config_ref", "model_config_revision"]
    );
    let receipt_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM local_agent_command_receipts WHERE command_id = 'unrelated-command'",
    )
    .fetch_one(&storage.pool)
    .await
    .expect("unrelated receipt");
    assert_eq!(receipt_count, 1);
    let schema_version: i64 =
        sqlx::query_scalar("SELECT MAX(version) FROM client_schema_migrations")
            .fetch_one(&storage.pool)
            .await
            .expect("schema version");
    assert_eq!(schema_version, 24);

    storage.pool.close().await;
    drop(storage);
    for path in [
        database_path.clone(),
        database_path.with_extension("sqlite-wal"),
        database_path.with_extension("sqlite-shm"),
        backup_path,
    ] {
        if let Err(error) = std::fs::remove_file(path) {
            assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
        }
    }
}

#[tokio::test]
async fn current_database_reopen_does_not_create_redundant_backup() {
    let database_path = std::env::temp_dir().join(format!(
        "chatos-local-current-schema-{}.sqlite",
        Uuid::new_v4()
    ));
    let storage = SqliteClientStorage::connect_file(&database_path)
        .await
        .expect("create current database");
    storage.pool.close().await;
    drop(storage);
    assert!(migration_backups(&database_path).is_empty());

    let reopened = SqliteClientStorage::connect_file(&database_path)
        .await
        .expect("reopen current database");
    assert!(migration_backups(&database_path).is_empty());
    reopened.pool.close().await;
    drop(reopened);
    for path in [
        database_path.clone(),
        database_path.with_extension("sqlite-wal"),
        database_path.with_extension("sqlite-shm"),
    ] {
        if let Err(error) = std::fs::remove_file(path) {
            assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
        }
    }
}

async fn primary_key_columns(storage: &SqliteClientStorage, table: &str) -> Vec<String> {
    let mut columns = sqlx::query(&format!("PRAGMA table_info({table})"))
        .fetch_all(&storage.pool)
        .await
        .expect("table info")
        .into_iter()
        .filter_map(|row| {
            let position: i64 = row.get("pk");
            (position > 0).then(|| (position, row.get::<String, _>("name")))
        })
        .collect::<Vec<_>>();
    columns.sort_by_key(|(position, _)| *position);
    columns.into_iter().map(|(_, name)| name).collect()
}

fn migration_backups(database_path: &std::path::Path) -> Vec<std::path::PathBuf> {
    let backup_prefix = format!(
        "{}.pre-migration-",
        database_path
            .file_name()
            .expect("database file name")
            .to_string_lossy()
    );
    std::fs::read_dir(database_path.parent().expect("database parent"))
        .expect("list migration backups")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|name| {
                    name.starts_with(&backup_prefix) && name.ends_with(".backup.sqlite")
                })
        })
        .collect()
}
