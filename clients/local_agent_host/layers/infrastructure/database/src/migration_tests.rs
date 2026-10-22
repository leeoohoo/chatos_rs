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
            sqlx::query(sqlx::AssertSqlSafe(*statement))
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
    assert!(migration_backups(&database_path).is_empty());
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
    }
    for table in [
        "local_capability_policy_snapshots",
        "local_model_config_snapshots",
    ] {
        let count: i64 =
            sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT COUNT(*) FROM {table}")))
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
    assert_eq!(receipt_count, 0);
    let schema_version: i64 =
        sqlx::query_scalar("SELECT MAX(version) FROM client_schema_migrations")
            .fetch_one(&storage.pool)
            .await
            .expect("schema version");
    assert_eq!(schema_version, super::migration::SCHEMA_VERSION);

    storage.pool.close().await;
    drop(storage);
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
    let stale_backup = database_path.with_file_name(format!(
        "{}.pre-migration-v31-to-v32-stale.backup.sqlite",
        database_path
            .file_name()
            .expect("database file name")
            .to_string_lossy()
    ));
    std::fs::write(&stale_backup, b"stale migration backup").expect("write stale backup");

    let reopened = SqliteClientStorage::connect_file(&database_path)
        .await
        .expect("reopen current database");
    assert!(migration_backups(&database_path).is_empty());
    assert!(!stale_backup.exists());
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

#[tokio::test]
async fn version_thirty_three_discards_transient_history_and_bounds_new_rows() {
    let database_path = std::env::temp_dir().join(format!(
        "chatos-transient-history-migration-{}.sqlite",
        Uuid::new_v4()
    ));
    let storage = SqliteClientStorage::connect_file(&database_path)
        .await
        .expect("create current database");
    storage.pool.close().await;
    drop(storage);

    let options = SqliteConnectOptions::new()
        .filename(&database_path)
        .foreign_keys(true);
    let mut connection = SqliteConnection::connect_with(&options)
        .await
        .expect("open version 32 database");
    for statement in [
        "DROP TRIGGER local_agent_step_memory_discard",
        "DROP TABLE local_agent_step_memory",
        "DROP TRIGGER local_agent_command_receipts_prune",
        "DROP INDEX local_agent_command_receipts_retention",
        "DROP TRIGGER local_memory_outbox_prune",
        "DROP INDEX local_memory_outbox_synced_retention",
        "DELETE FROM client_schema_migrations WHERE version >= 33",
    ] {
        sqlx::query(statement)
            .execute(&mut connection)
            .await
            .expect("rewind version 33 schema");
    }
    sqlx::query(
        "INSERT INTO local_agent_command_receipts(\
         command_id, request_fingerprint, response_json, created_at_unix_ms\
         ) VALUES('legacy-receipt', 'legacy', '{\"large\":true}', 1)",
    )
    .execute(&mut connection)
    .await
    .expect("insert legacy receipt");
    for (record_id, status) in [("synced-record", "synced"), ("pending-record", "pending")] {
        sqlx::query(
            "INSERT INTO local_memory_outbox(\
             record_id, tenant_id, source_id, thread_id, payload_json, status,\
             created_at_unix_ms, updated_at_unix_ms\
             ) VALUES(?, 'tenant', 'source', 'thread', '{}', ?, 1, 1)",
        )
        .bind(record_id)
        .bind(status)
        .execute(&mut connection)
        .await
        .expect("insert legacy outbox row");
    }
    connection.close().await.expect("close version 32 database");

    let migrated = SqliteClientStorage::connect_file(&database_path)
        .await
        .expect("migrate transient history");
    let receipt_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM local_agent_command_receipts")
            .fetch_one(&migrated.pool)
            .await
            .expect("count migrated receipts");
    assert_eq!(receipt_count, 0);
    let outbox_statuses: Vec<String> =
        sqlx::query_scalar("SELECT status FROM local_memory_outbox ORDER BY status")
            .fetch_all(&migrated.pool)
            .await
            .expect("read migrated outbox");
    assert_eq!(outbox_statuses, vec!["pending"]);
    assert!(migration_backups(&database_path).is_empty());

    sqlx::query(
        "INSERT INTO local_agent_command_receipts(\
         command_id, request_fingerprint, response_json, created_at_unix_ms\
         ) VALUES('expired', 'old', '{}', 1)",
    )
    .execute(&migrated.pool)
    .await
    .expect("insert expired receipt");
    sqlx::query(
        "INSERT INTO local_agent_command_receipts(\
         command_id, request_fingerprint, response_json, created_at_unix_ms\
         ) VALUES('current', 'new', '{}', 86400002)",
    )
    .execute(&migrated.pool)
    .await
    .expect("insert current receipt");
    let expired_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM local_agent_command_receipts WHERE command_id = 'expired'",
    )
    .fetch_one(&migrated.pool)
    .await
    .expect("count expired receipts");
    assert_eq!(expired_count, 0);

    sqlx::query(
        "WITH RECURSIVE sequence(value) AS (\
           SELECT 1 UNION ALL SELECT value + 1 FROM sequence WHERE value < 5001\
         ) INSERT INTO local_agent_command_receipts(\
           command_id, request_fingerprint, response_json, created_at_unix_ms\
         ) SELECT printf('bulk-%05d', value), 'bulk', '{}', 86400002 + value FROM sequence",
    )
    .execute(&migrated.pool)
    .await
    .expect("insert bounded receipts");
    let bounded_receipt_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM local_agent_command_receipts")
            .fetch_one(&migrated.pool)
            .await
            .expect("count bounded receipts");
    assert_eq!(bounded_receipt_count, 5_000);

    sqlx::query(
        "WITH RECURSIVE sequence(value) AS (\
           SELECT 1 UNION ALL SELECT value + 1 FROM sequence WHERE value < 1001\
         ) INSERT INTO local_memory_outbox(\
           record_id, tenant_id, source_id, thread_id, payload_json, status,\
           created_at_unix_ms, updated_at_unix_ms\
         ) SELECT printf('bulk-%05d', value), 'tenant', 'bulk-source', 'thread', '{}',\
                  'pending', value, value FROM sequence",
    )
    .execute(&migrated.pool)
    .await
    .expect("insert outbox rows");
    sqlx::query("UPDATE local_memory_outbox SET status = 'synced' WHERE source_id = 'bulk-source'")
        .execute(&migrated.pool)
        .await
        .expect("sync outbox rows");
    let synced_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM local_memory_outbox WHERE status = 'synced'")
            .fetch_one(&migrated.pool)
            .await
            .expect("count bounded outbox rows");
    assert_eq!(synced_count, 1_000);

    migrated.pool.close().await;
    drop(migrated);
    remove_database_files(&database_path);
}

#[tokio::test]
async fn failed_migration_preserves_recovery_backup() {
    let database_path = std::env::temp_dir().join(format!(
        "chatos-failed-migration-backup-{}.sqlite",
        Uuid::new_v4()
    ));
    let storage = SqliteClientStorage::connect_file(&database_path)
        .await
        .expect("create current database");
    storage.pool.close().await;
    drop(storage);

    let options = SqliteConnectOptions::new()
        .filename(&database_path)
        .foreign_keys(true);
    let mut connection = SqliteConnection::connect_with(&options)
        .await
        .expect("open database to damage migration source");
    sqlx::query("DELETE FROM client_schema_migrations WHERE version >= 33")
        .execute(&mut connection)
        .await
        .expect("rewind schema version");
    sqlx::query("DROP TABLE local_memory_outbox")
        .execute(&mut connection)
        .await
        .expect("remove required migration table");
    connection.close().await.expect("close damaged database");

    assert!(SqliteClientStorage::connect_file(&database_path)
        .await
        .is_err());
    let backups = migration_backups(&database_path);
    assert_eq!(backups.len(), 1);

    for backup in backups {
        std::fs::remove_file(backup).expect("remove test migration backup");
    }
    remove_database_files(&database_path);
}

#[tokio::test]
async fn version_twenty_nine_backfills_failed_conversation_messages() {
    let database_path = std::env::temp_dir().join(format!(
        "chatos-failed-conversation-migration-{}.sqlite",
        Uuid::new_v4()
    ));
    let storage = SqliteClientStorage::connect_file(&database_path)
        .await
        .expect("create current database");
    storage.pool.close().await;
    drop(storage);

    let options = SqliteConnectOptions::new()
        .filename(&database_path)
        .foreign_keys(true);
    let mut connection = SqliteConnection::connect_with(&options)
        .await
        .expect("open version 28 database");
    sqlx::query("DROP TRIGGER local_agent_command_receipts_prune")
        .execute(&mut connection)
        .await
        .expect("remove version 33 receipt trigger");
    sqlx::query("DROP TRIGGER local_agent_step_memory_discard")
        .execute(&mut connection)
        .await
        .expect("remove version 34 trigger");
    sqlx::query("DROP TABLE local_agent_step_memory")
        .execute(&mut connection)
        .await
        .expect("remove version 34 table");
    sqlx::query("DROP INDEX local_agent_command_receipts_retention")
        .execute(&mut connection)
        .await
        .expect("remove version 33 receipt index");
    sqlx::query("DROP TRIGGER local_memory_outbox_prune")
        .execute(&mut connection)
        .await
        .expect("remove version 33 outbox trigger");
    sqlx::query("DROP INDEX local_memory_outbox_synced_retention")
        .execute(&mut connection)
        .await
        .expect("remove version 33 outbox index");
    sqlx::query("DROP TABLE local_requirement_plans")
        .execute(&mut connection)
        .await
        .expect("remove version 32 table");
    sqlx::query("DROP TABLE local_task_followup_reviews")
        .execute(&mut connection)
        .await
        .expect("remove version 30 table");
    sqlx::query("DROP INDEX local_agent_tool_invocations_route_claimable")
        .execute(&mut connection)
        .await
        .expect("remove version 31 route index");
    sqlx::query("ALTER TABLE local_agent_tool_invocations DROP COLUMN execution_route")
        .execute(&mut connection)
        .await
        .expect("remove version 31 route column");
    sqlx::query("DELETE FROM client_schema_migrations WHERE version >= 29")
        .execute(&mut connection)
        .await
        .expect("rewind schema version");
    sqlx::query(
        "INSERT INTO local_conversations(\
         conversation_id, owner_user_id, title, version, created_at_unix_ms, updated_at_unix_ms\
         ) VALUES('conversation-failed', 'user-1', 'Failed conversation', 1, 1000, 1000)",
    )
    .execute(&mut connection)
    .await
    .expect("insert conversation");
    sqlx::query(
        "INSERT INTO local_agent_runs(\
         run_id, owner_user_id, owner_entity_type, owner_entity_id, profile_key,\
         model_config_ref, model_config_revision, capability_policy_revision, input_json, status,\
         iteration, model_attempt, max_iterations, version, terminal_outcome_json, checkpoint_json,\
         created_at_unix_ms, updated_at_unix_ms\
         ) VALUES(\
         'run-failed', 'user-1', 'conversation_turn', 'turn-failed', 'main_chat',\
         'model-1', 'revision-1', 'policy-1', '{}', 'failed',\
         1, 5, 8, 2, '{\"error\":\"provider rejected request\"}', 'null', 2000, 3000\
         )",
    )
    .execute(&mut connection)
    .await
    .expect("insert failed Run");
    sqlx::query(
        "INSERT INTO local_conversation_turns(\
         turn_id, conversation_id, user_message_id, run_id, status,\
         created_at_unix_ms, updated_at_unix_ms\
         ) VALUES(\
         'turn-failed', 'conversation-failed', 'message-user', 'run-failed', 'failed', 2000, 3000\
         )",
    )
    .execute(&mut connection)
    .await
    .expect("insert failed Turn");
    sqlx::query(
        "INSERT INTO local_conversation_messages(\
         message_id, conversation_id, turn_id, ordinal, role, content_json, metadata_json,\
         created_at_unix_ms\
         ) VALUES(\
         'message-user', 'conversation-failed', 'turn-failed', 1, 'user',\
         '{\"message\":\"hello\"}', '{}', 2000\
         )",
    )
    .execute(&mut connection)
    .await
    .expect("insert user message");
    connection.close().await.expect("close version 28 database");

    let migrated = SqliteClientStorage::connect_file(&database_path)
        .await
        .expect("migrate failed conversation");
    let rows = sqlx::query(
        "SELECT message_id, ordinal, role, content_json, metadata_json \
         FROM local_conversation_messages \
         WHERE conversation_id = 'conversation-failed' ORDER BY ordinal",
    )
    .fetch_all(&migrated.pool)
    .await
    .expect("read migrated messages");
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows[1].get::<String, _>("message_id"),
        "assistant:run-failed"
    );
    assert_eq!(rows[1].get::<i64, _>("ordinal"), 2);
    assert_eq!(rows[1].get::<String, _>("role"), "assistant");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&rows[1].get::<String, _>("content_json"))
            .expect("assistant content"),
        serde_json::json!({"error": "provider rejected request"})
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&rows[1].get::<String, _>("metadata_json"))
            .expect("assistant metadata"),
        serde_json::json!({"run_id": "run-failed", "terminal_status": "failed"})
    );
    let conversation: (i64, i64) = sqlx::query_as(
        "SELECT version, updated_at_unix_ms FROM local_conversations \
         WHERE conversation_id = 'conversation-failed'",
    )
    .fetch_one(&migrated.pool)
    .await
    .expect("read migrated conversation");
    assert_eq!(conversation, (2, 3000));
    let schema_version: i64 =
        sqlx::query_scalar("SELECT MAX(version) FROM client_schema_migrations")
            .fetch_one(&migrated.pool)
            .await
            .expect("schema version");
    assert_eq!(schema_version, super::migration::SCHEMA_VERSION);

    migrated.pool.close().await;
    drop(migrated);
    for path in std::iter::once(database_path.clone())
        .chain(std::iter::once(database_path.with_extension("sqlite-wal")))
        .chain(std::iter::once(database_path.with_extension("sqlite-shm")))
        .chain(migration_backups(&database_path))
    {
        if let Err(error) = std::fs::remove_file(path) {
            assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
        }
    }
}

async fn primary_key_columns(storage: &SqliteClientStorage, table: &str) -> Vec<String> {
    let mut columns = sqlx::query(sqlx::AssertSqlSafe(format!("PRAGMA table_info({table})")))
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

fn remove_database_files(database_path: &std::path::Path) {
    for path in [
        database_path.to_path_buf(),
        database_path.with_extension("sqlite-wal"),
        database_path.with_extension("sqlite-shm"),
    ] {
        if let Err(error) = std::fs::remove_file(path) {
            assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
        }
    }
}
