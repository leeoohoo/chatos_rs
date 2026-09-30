// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use sqlx::Connection;
use uuid::Uuid;

fn run(now: i64) -> LocalAgentRunRecord {
    LocalAgentRunRecord {
        run_id: "run-1".to_string(),
        owner_user_id: "user-1".to_string(),
        owner_entity_type: "conversation".to_string(),
        owner_entity_id: "conversation-1".to_string(),
        profile_key: "main_chat".to_string(),
        model_config_ref: "model-1".to_string(),
        model_config_revision: "revision-1".to_string(),
        capability_policy_revision: "policy-1".to_string(),
        input: serde_json::json!({"message": "hello"}),
        status: LocalAgentRunStatus::Queued,
        iteration: 0,
        model_attempt: 1,
        max_iterations: 8,
        version: 1,
        claim_token: None,
        claim_until_unix_ms: None,
        next_attempt_at_unix_ms: None,
        pending_tool_batch: None,
        checkpoint: serde_json::Value::Null,
        continuation_input: None,
        terminal_outcome: None,
        created_at_unix_ms: now,
        updated_at_unix_ms: now,
    }
}

fn owned_run(run_id: &str, owner_user_id: &str, now: i64) -> LocalAgentRunRecord {
    let mut record = run(now);
    record.run_id = run_id.to_string();
    record.owner_user_id = owner_user_id.to_string();
    record.owner_entity_id = format!("conversation-{owner_user_id}");
    record
}

fn command(id: &str, fingerprint: &str) -> IdempotentCommand {
    IdempotentCommand {
        command_id: id.to_string(),
        request_fingerprint: fingerprint.to_string(),
    }
}

#[tokio::test]
async fn create_and_claim_are_atomic_and_idempotent() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    let created = storage
        .create_run(&command("create-1", "create"), &run(1_000), "event-created")
        .await
        .expect("create");
    assert_eq!(
        storage
            .next_retry_at("user-1")
            .await
            .expect("retry deadline"),
        None
    );
    let replay = storage
        .create_run(&command("create-1", "create"), &run(1_000), "ignored")
        .await
        .expect("replay");
    assert_eq!(created, replay);

    let claim = storage
        .claim_next_run(
            &command("claim-1", "claim"),
            "user-1",
            "worker-1",
            "token-1",
            2_000,
            12_000,
            "event-claimed",
        )
        .await
        .expect("claim")
        .expect("claimed run");
    assert_eq!(claim.run.status, LocalAgentRunStatus::ModelRunning);
    assert_eq!(claim.run.iteration, 1);
    assert_eq!(claim.run.version, 2);
    assert!(storage
        .claim_next_run(
            &command("claim-2", "claim-next"),
            "user-1",
            "worker-2",
            "token-2",
            2_001,
            12_001,
            "event-unused",
        )
        .await
        .expect("second claim")
        .is_none());

    storage
        .apply_transition(
            &command("retry-1", "retry"),
            &RunTransition {
                run_id: claim.run.run_id,
                claim_token: claim.claim_token,
                expected_version: claim.run.version,
                expected_status: LocalAgentRunStatus::ModelRunning,
                next_status: LocalAgentRunStatus::RetryScheduled,
                next_model_attempt: 2,
                next_attempt_at_unix_ms: Some(20_000),
                pending_tool_batch: None,
                tool_batch: None,
                checkpoint: None,
                clear_continuation_input: false,
                terminal_outcome: None,
                event_id: "event-retry".to_string(),
                event_type: "run_retry_scheduled".to_string(),
                event_payload: serde_json::json!({"next_attempt_at_unix_ms": 20_000}),
                occurred_at_unix_ms: 2_002,
            },
        )
        .await
        .expect("schedule retry");
    assert_eq!(
        storage
            .next_retry_at("user-1")
            .await
            .expect("retry deadline"),
        Some(20_000)
    );
}

#[tokio::test]
async fn expired_unknown_step_requires_review_instead_of_replay() {
    let database_path = std::env::temp_dir().join(format!(
        "chatos-local-agent-storage-{}.sqlite",
        Uuid::new_v4()
    ));
    let storage = SqliteClientStorage::connect_file(&database_path)
        .await
        .expect("storage");
    storage
        .create_run(&command("create-1", "create"), &run(1_000), "event-created")
        .await
        .expect("create");
    storage
        .claim_next_run(
            &command("claim-1", "claim"),
            "user-1",
            "worker-1",
            &Uuid::new_v4().to_string(),
            2_000,
            3_000,
            "event-claimed",
        )
        .await
        .expect("claim");
    storage.pool.close().await;
    drop(storage);

    let storage = SqliteClientStorage::connect_file(&database_path)
        .await
        .expect("reopened storage");
    assert_eq!(
        storage
            .recover_expired_claims("user-1", 3_001)
            .await
            .expect("recover"),
        1
    );
    let recovered = storage.get_run("run-1").await.expect("get").expect("run");
    assert_eq!(recovered.status, LocalAgentRunStatus::NeedsReview);
    assert!(recovered.claim_token.is_none());
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

#[tokio::test]
async fn recovery_claiming_and_retry_deadlines_are_owner_scoped() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    for (run_id, owner) in [("run-user-1", "user-1"), ("run-user-2", "user-2")] {
        storage
            .create_run(
                &command(&format!("create-{run_id}"), &format!("create-{run_id}")),
                &owned_run(run_id, owner, 1_000),
                &format!("event-create-{run_id}"),
            )
            .await
            .expect("create run");
        storage
            .claim_next_run(
                &command(&format!("claim-{run_id}"), &format!("claim-{run_id}")),
                owner,
                &format!("worker-{owner}"),
                &format!("token-{owner}"),
                2_000,
                3_000,
                &format!("event-claim-{run_id}"),
            )
            .await
            .expect("claim run")
            .expect("claimed run");
    }

    assert_eq!(
        storage
            .recover_expired_claims("user-1", 3_001)
            .await
            .expect("recover user-1"),
        1
    );
    assert_eq!(
        storage
            .get_run("run-user-1")
            .await
            .expect("get user-1")
            .expect("user-1 run")
            .status,
        LocalAgentRunStatus::NeedsReview
    );
    assert_eq!(
        storage
            .get_run("run-user-2")
            .await
            .expect("get user-2")
            .expect("user-2 run")
            .status,
        LocalAgentRunStatus::ModelRunning
    );

    storage
        .claim_next_run(
            &command("claim-user-1-again", "claim-user-1-again"),
            "user-1",
            "worker-user-1",
            "token-user-1-again",
            4_000,
            5_000,
            "event-claim-user-1-again",
        )
        .await
        .expect("claim without cross-account recovery");
    assert_eq!(
        storage
            .get_run("run-user-2")
            .await
            .expect("get user-2")
            .expect("user-2 run")
            .status,
        LocalAgentRunStatus::ModelRunning
    );

    sqlx::query(
        "UPDATE local_agent_runs SET status = 'retry_scheduled', \
         next_attempt_at_unix_ms = CASE owner_user_id \
           WHEN 'user-1' THEN 20_000 ELSE 10_000 END, \
         claim_token = NULL, claim_until_unix_ms = NULL",
    )
    .execute(&storage.pool)
    .await
    .expect("schedule retries");
    assert_eq!(
        storage.next_retry_at("user-1").await.expect("user-1 retry"),
        Some(20_000)
    );
    assert_eq!(
        storage.next_retry_at("user-2").await.expect("user-2 retry"),
        Some(10_000)
    );
}

#[tokio::test]
async fn tool_recovery_is_owner_scoped() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    for (run_id, owner) in [("run-user-1", "user-1"), ("run-user-2", "user-2")] {
        storage
            .create_run(
                &command(&format!("create-{run_id}"), &format!("create-{run_id}")),
                &owned_run(run_id, owner, 1_000),
                &format!("event-create-{run_id}"),
            )
            .await
            .expect("create run");
        sqlx::query("UPDATE local_agent_runs SET status = 'waiting_tool_result' WHERE run_id = ?")
            .bind(run_id)
            .execute(&storage.pool)
            .await
            .expect("set waiting tool");
        sqlx::query(
            "INSERT INTO local_agent_tool_invocations(\
               invocation_id, run_id, batch_id, call_id, tool_name, arguments_json, \
               side_effecting, status, version, claim_token, claim_until_unix_ms, \
               created_at_unix_ms, updated_at_unix_ms\
             ) VALUES(?, ?, 'batch-1', 'call-1', 'read_file', '{}', 0, 'running', \
               1, ?, 3_000, 1_000, 2_000)",
        )
        .bind(format!("invocation-{owner}"))
        .bind(run_id)
        .bind(format!("tool-token-{owner}"))
        .execute(&storage.pool)
        .await
        .expect("insert running tool");
    }

    assert_eq!(
        storage
            .recover_expired_tool_claims("user-1", 3_001)
            .await
            .expect("recover user-1 tools"),
        1
    );
    let statuses: Vec<(String, String)> = sqlx::query_as(
        "SELECT invocation_id, status FROM local_agent_tool_invocations ORDER BY invocation_id",
    )
    .fetch_all(&storage.pool)
    .await
    .expect("tool statuses");
    assert_eq!(
        statuses,
        vec![
            ("invocation-user-1".to_string(), "pending".to_string()),
            ("invocation-user-2".to_string(), "running".to_string()),
        ]
    );

    storage
        .claim_next_tool(
            &command("claim-user-1-tool", "claim-user-1-tool"),
            "user-1",
            "tool-worker-user-1",
            "tool-token-user-1-next",
            4_000,
            5_000,
            "event-claim-user-1-tool",
            None,
            &[],
        )
        .await
        .expect("claim user-1 tool")
        .expect("claimed user-1 tool");
    let user_2_status: String = sqlx::query_scalar(
        "SELECT status FROM local_agent_tool_invocations WHERE invocation_id = ?",
    )
    .bind("invocation-user-2")
    .fetch_one(&storage.pool)
    .await
    .expect("user-2 tool status");
    assert_eq!(user_2_status, "running");
}

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
        sqlx::query(statement)
            .execute(&mut connection)
            .await
            .expect("schema v1");
    }
    for statement in crate::schema::SCHEMA_V2 {
        sqlx::query(statement)
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
    assert_eq!(schema_version, 25);
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
