// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[tokio::test]
async fn expired_claim_cannot_commit_a_late_step() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    storage
        .create_run(&command("create-1", "create"), &run(1_000), "event-created")
        .await
        .expect("create");
    let claim = storage
        .claim_next_run(
            &command("claim-1", "claim"),
            "user-1",
            "worker-1",
            "token-1",
            2_000,
            3_000,
            "event-claimed",
        )
        .await
        .expect("claim")
        .expect("claimed run");
    let error = storage
        .apply_transition(
            &command("commit-1", "commit"),
            &RunTransition {
                run_id: claim.run.run_id,
                claim_token: claim.claim_token,
                expected_version: claim.run.version,
                expected_status: LocalAgentRunStatus::ModelRunning,
                next_status: LocalAgentRunStatus::Succeeded,
                next_model_attempt: 1,
                next_attempt_at_unix_ms: None,
                pending_tool_batch: None,
                tool_batch: None,
                checkpoint: None,
                clear_continuation_input: true,
                terminal_outcome: Some(serde_json::json!({"answer": 42})),
                event_id: "event-completed".to_string(),
                event_type: "run_succeeded".to_string(),
                event_payload: serde_json::json!({"answer": 42}),
                occurred_at_unix_ms: 3_001,
            },
        )
        .await
        .expect_err("expired claim must fail");
    assert!(matches!(error, ClientStorageError::Conflict(_)));
}

#[tokio::test]
async fn claim_renewal_preserves_versions_and_rejects_expired_or_foreign_leases() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    storage
        .create_run(
            &command("create-renew", "create"),
            &run(1_000),
            "event-created",
        )
        .await
        .expect("create");
    let claim = storage
        .claim_next_run(
            &command("claim-renew", "claim"),
            "user-1",
            "worker-1",
            "run-token",
            2_000,
            3_000,
            "event-claimed",
        )
        .await
        .expect("claim")
        .expect("claimed run");
    assert!(storage
        .renew_run_claim(
            "user-1",
            &claim.run.run_id,
            &claim.claim_token,
            claim.run.version,
            2_500,
            5_000,
        )
        .await
        .expect("renew run"));
    assert!(!storage
        .renew_run_claim(
            "user-2",
            &claim.run.run_id,
            &claim.claim_token,
            claim.run.version,
            2_600,
            6_000,
        )
        .await
        .expect("foreign renewal"));
    let renewed = storage
        .get_run(&claim.run.run_id)
        .await
        .expect("get run")
        .expect("run");
    assert_eq!(renewed.version, claim.run.version);
    assert_eq!(renewed.claim_until_unix_ms, Some(5_000));
    assert!(!storage
        .renew_run_claim(
            "user-1",
            &claim.run.run_id,
            &claim.claim_token,
            claim.run.version,
            5_001,
            8_000,
        )
        .await
        .expect("expired renewal"));

    sqlx::query(
        "UPDATE local_agent_runs SET status = 'waiting_tool_result', \
         claim_token = NULL, claim_until_unix_ms = NULL WHERE run_id = 'run-1'",
    )
    .execute(&storage.pool)
    .await
    .expect("prepare tool run");
    sqlx::query(
        "INSERT INTO local_agent_tool_invocations(\
         invocation_id, run_id, batch_id, call_id, tool_name, arguments_json, \
         side_effecting, requires_approval, approval_status, status, result_json, error_text, \
         version, claim_token, claim_until_unix_ms, created_at_unix_ms, updated_at_unix_ms) \
         VALUES('invocation-renew', 'run-1', 'batch-1', 'call-1', 'read_file', '{}', \
         0, 0, 'not_required', 'running', NULL, NULL, 2, 'tool-token', 3_000, 1_000, 2_000)",
    )
    .execute(&storage.pool)
    .await
    .expect("insert running tool");
    assert!(storage
        .renew_tool_claim("user-1", "invocation-renew", "tool-token", 2, 2_500, 5_000,)
        .await
        .expect("renew tool"));
    assert!(!storage
        .renew_tool_claim("user-2", "invocation-renew", "tool-token", 2, 2_600, 6_000,)
        .await
        .expect("foreign tool renewal"));
    let renewed_tool = storage
        .get_tool_invocation("invocation-renew")
        .await
        .expect("get tool")
        .expect("tool");
    assert_eq!(renewed_tool.version, 2);
    assert_eq!(renewed_tool.claim_until_unix_ms, Some(5_000));
    assert!(!storage
        .renew_tool_claim("user-1", "invocation-renew", "tool-token", 2, 5_001, 8_000,)
        .await
        .expect("expired tool renewal"));

    let receipt_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM local_agent_command_receipts")
            .fetch_one(&storage.pool)
            .await
            .expect("receipt count");
    assert_eq!(receipt_count, 2);
}

#[tokio::test]
async fn version_two_database_migrates_through_conversation_schema() {
    let database_path = std::env::temp_dir().join(format!(
        "chatos-local-agent-migration-{}.sqlite",
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
    for statement in crate::schema::SCHEMA_V1 {
        sqlx::query(sqlx::AssertSqlSafe(*statement))
            .execute(&mut connection)
            .await
            .expect("schema v1");
    }
    for statement in crate::schema::SCHEMA_V2 {
        sqlx::query(sqlx::AssertSqlSafe(*statement))
            .execute(&mut connection)
            .await
            .expect("schema v2");
    }
    sqlx::query(
        "INSERT INTO client_schema_migrations(version, applied_at_unix_ms) \
         VALUES(1, 1000), (2, 2000)",
    )
    .execute(&mut connection)
    .await
    .expect("legacy versions");
    connection.close().await.expect("close legacy database");

    let storage = SqliteClientStorage::connect_file(&database_path)
        .await
        .expect("migrate storage");
    let expected = run(3_000);
    let created = storage
        .create_run(
            &command("create-migrated", "create-migrated"),
            &expected,
            "event-created-migrated",
        )
        .await
        .expect("create after migration");
    assert_eq!(created.checkpoint, serde_json::Value::Null);
    assert!(created.continuation_input.is_none());
    assert_eq!(created.model_attempt, 1);
    let schema_version: i64 =
        sqlx::query_scalar("SELECT MAX(version) FROM client_schema_migrations")
            .fetch_one(&storage.pool)
            .await
            .expect("schema version");
    assert_eq!(schema_version, 29);
    let memory_tenant_indexes: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' \
         AND name = 'local_memory_outbox_tenant_runnable'",
    )
    .fetch_one(&storage.pool)
    .await
    .expect("Memory tenant runnable index");
    assert_eq!(memory_tenant_indexes, 1);
    let owner_recovery_indexes: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name IN (\
         'local_agent_runs_owner_runnable', 'local_agent_tool_invocations_expired')",
    )
    .fetch_one(&storage.pool)
    .await
    .expect("owner recovery indexes");
    assert_eq!(owner_recovery_indexes, 2);
    let task_tables: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN (\
         'local_task_graphs', 'local_tasks', 'local_task_dependencies')",
    )
    .fetch_one(&storage.pool)
    .await
    .expect("task tables");
    assert_eq!(task_tables, 3);
    let plugin_tables: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' \
         AND name = 'local_plugin_installations'",
    )
    .fetch_one(&storage.pool)
    .await
    .expect("plugin table");
    assert_eq!(plugin_tables, 1);
    let conversation_tables: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN (\
         'local_conversations', 'local_conversation_turns', 'local_conversation_messages')",
    )
    .fetch_one(&storage.pool)
    .await
    .expect("conversation tables");
    assert_eq!(conversation_tables, 3);
    let attachment_tables: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' \
         AND name = 'local_conversation_message_attachments'",
    )
    .fetch_one(&storage.pool)
    .await
    .expect("conversation attachment table");
    assert_eq!(attachment_tables, 1);
    let writeback_tables: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' \
         AND name = 'local_task_graph_writebacks'",
    )
    .fetch_one(&storage.pool)
    .await
    .expect("Task Graph writeback table");
    assert_eq!(writeback_tables, 1);
    let guidance_tables: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' \
         AND name = 'local_conversation_guidance'",
    )
    .fetch_one(&storage.pool)
    .await
    .expect("Conversation guidance table");
    assert_eq!(guidance_tables, 1);
    let capability_tables: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' \
         AND name = 'local_capability_policy_snapshots'",
    )
    .fetch_one(&storage.pool)
    .await
    .expect("capability snapshot table");
    assert_eq!(capability_tables, 1);
    let model_config_tables: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' \
         AND name = 'local_model_config_snapshots'",
    )
    .fetch_one(&storage.pool)
    .await
    .expect("model config snapshot table");
    assert_eq!(model_config_tables, 1);
    storage.pool.close().await;
    drop(storage);
    for path in [
        database_path.clone(),
        database_path.with_extension("sqlite-wal"),
        database_path.with_extension("sqlite-shm"),
    ] {
        if let Err(error) = std::fs::remove_file(&path) {
            assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
        }
    }
}
