// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[tokio::test]
async fn large_create_replays_original_run_after_business_state_changes_and_restart() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("client.sqlite");
    let storage = SqliteClientStorage::connect_file(&path).await.unwrap();
    let create = command("large-create", "input");
    let mut original = run(1000);
    original.input = serde_json::json!({"message": "你".repeat(200_000)});
    storage
        .create_run(&create, &original, "created-event")
        .await
        .unwrap();
    let reference: Option<String> = sqlx::query_scalar(
        "SELECT result_digest FROM local_agent_command_receipts WHERE command_id = 'large-create'",
    )
    .fetch_one(&storage.pool)
    .await
    .unwrap();
    assert!(reference.is_some());
    sqlx::query("UPDATE local_agent_runs SET input_json = '{}', status = 'cancelled', version = 2")
        .execute(&storage.pool)
        .await
        .unwrap();
    storage.pool.close().await;
    let reopened = SqliteClientStorage::connect_file(&path).await.unwrap();
    assert_eq!(
        reopened
            .create_run(&create, &original, "repeated-event")
            .await
            .unwrap(),
        original
    );
    assert_eq!(
        reopened
            .get_run(&original.run_id)
            .await
            .unwrap()
            .unwrap()
            .version,
        2
    );
    let events: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM local_agent_events")
        .fetch_one(&reopened.pool)
        .await
        .unwrap();
    assert_eq!(events, 1);
    // A damaged retained body must not be treated as cache expiry or a new write.
    sqlx::query("DROP TRIGGER local_agent_receipt_results_immutable")
        .execute(&reopened.pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE local_agent_receipt_results SET response_json = replace(response_json, '你', '他')",
    )
    .execute(&reopened.pool)
    .await
    .unwrap();
    let error = reopened
        .create_run(&create, &original, "corrupt-retry-event")
        .await
        .unwrap_err();
    assert_eq!(error.code(), "storage_corrupt");
    assert!(!error.retryable());
    let events: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM local_agent_events")
        .fetch_one(&reopened.pool)
        .await
        .unwrap();
    assert_eq!(events, 1);
    reopened.pool.close().await;
}

#[tokio::test]
async fn large_result_accounting_failure_rolls_back_business_and_blob_without_orphans() {
    let storage = SqliteClientStorage::connect_memory().await.unwrap();
    sqlx::query("CREATE TRIGGER injected_result_accounting_failure AFTER INSERT ON local_agent_receipt_sizes BEGIN SELECT RAISE(ABORT, 'injected result accounting failure'); END")
        .execute(&storage.pool).await.unwrap();
    let create = command("large-create", "input");
    let mut original = run(1000);
    original.input = serde_json::json!({"message": "x".repeat(600_000)});
    assert!(storage
        .create_run(&create, &original, "created-event")
        .await
        .is_err());
    for table in [
        "local_agent_runs",
        "local_agent_events",
        "local_agent_command_receipts",
        "local_agent_command_tombstones",
        "local_agent_receipt_sizes",
        "local_agent_receipt_results",
    ] {
        let count: i64 =
            sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT COUNT(*) FROM {table}")))
                .fetch_one(&storage.pool)
                .await
                .unwrap();
        assert_eq!(count, 0, "{table} must roll back with the business write");
    }
    assert_eq!(
        storage.receipt_cache_usage().await.unwrap().response_bytes,
        0
    );
    sqlx::query("DROP TRIGGER injected_result_accounting_failure")
        .execute(&storage.pool)
        .await
        .unwrap();
    assert_eq!(
        storage
            .create_run(&create, &original, "created-event")
            .await
            .unwrap(),
        original
    );
}

#[tokio::test]
async fn concurrent_large_create_with_same_key_commits_one_run_event_and_result() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("client.sqlite");
    let first = SqliteClientStorage::connect_file(&path).await.unwrap();
    let second = SqliteClientStorage::connect_file(&path).await.unwrap();
    let create = command("large-create", "input");
    let mut original = run(1000);
    original.input = serde_json::json!({"message": "x".repeat(600_000)});
    let (a, b) = tokio::join!(
        first.create_run(&create, &original, "first-event"),
        second.create_run(&create, &original, "second-event")
    );
    assert_eq!(a.unwrap(), original);
    assert_eq!(b.unwrap(), original);
    for table in [
        "local_agent_runs",
        "local_agent_events",
        "local_agent_command_tombstones",
        "local_agent_receipt_results",
    ] {
        let count: i64 =
            sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT COUNT(*) FROM {table}")))
                .fetch_one(&first.pool)
                .await
                .unwrap();
        assert_eq!(count, 1, "{table} must be written once");
    }
    first.pool.close().await;
    second.pool.close().await;
}

#[tokio::test]
async fn response_larger_than_cache_budget_keeps_business_success_and_prevents_repeat() {
    let storage = SqliteClientStorage::connect_memory().await.unwrap();
    sqlx::query("UPDATE local_agent_receipt_cache_usage SET budget_bytes = 16 WHERE singleton = 1")
        .execute(&storage.pool)
        .await
        .unwrap();
    let create = command("oversized-create", "input");
    let original = run(1000);
    let created = storage
        .create_run(&create, &original, "created-event")
        .await
        .unwrap();
    assert_eq!(
        created, original,
        "the initial successful operation still returns its result"
    );
    let usage = storage.receipt_cache_usage().await.unwrap();
    assert_eq!(usage.response_bytes, 0);
    assert_eq!(usage.response_count, 0);
    assert!(matches!(
        storage
            .create_run(&create, &original, "repeated-event")
            .await,
        Err(ClientStorageError::ReplayExpired(_))
    ));
    assert_eq!(
        storage.get_run(&original.run_id).await.unwrap(),
        Some(original)
    );
    let events: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM local_agent_events")
        .fetch_one(&storage.pool)
        .await
        .unwrap();
    assert_eq!(events, 1);
}

#[tokio::test]
async fn cache_accounting_failure_rolls_back_run_event_identity_and_usage() {
    let storage = SqliteClientStorage::connect_memory().await.unwrap();
    sqlx::query("CREATE TRIGGER injected_accounting_failure AFTER INSERT ON local_agent_receipt_sizes BEGIN SELECT RAISE(ABORT, 'injected accounting failure'); END")
        .execute(&storage.pool).await.unwrap();
    let create = command("accounted-create", "input");
    let original = run(1000);
    assert!(storage
        .create_run(&create, &original, "created-event")
        .await
        .is_err());
    assert!(storage.get_run(&original.run_id).await.unwrap().is_none());
    for table in [
        "local_agent_events",
        "local_agent_command_receipts",
        "local_agent_command_tombstones",
        "local_agent_receipt_sizes",
    ] {
        let count: i64 =
            sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT COUNT(*) FROM {table}")))
                .fetch_one(&storage.pool)
                .await
                .unwrap();
        assert_eq!(count, 0);
    }
    assert_eq!(
        storage.receipt_cache_usage().await.unwrap().response_bytes,
        0
    );
    assert_eq!(
        storage.receipt_cache_usage().await.unwrap().response_count,
        0
    );
    sqlx::query("DROP TRIGGER injected_accounting_failure")
        .execute(&storage.pool)
        .await
        .unwrap();
    storage
        .create_run(&create, &original, "created-event")
        .await
        .unwrap();
    assert_eq!(
        storage.receipt_cache_usage().await.unwrap().response_count,
        1
    );
}

#[tokio::test]
async fn expired_create_cannot_recreate_a_deleted_run_or_repeat_its_event() {
    let storage = SqliteClientStorage::connect_memory().await.unwrap();
    let original = run(1000);
    let original_command = command("original-create", "original-input");
    storage
        .create_run(&original_command, &original, "original-event")
        .await
        .unwrap();
    sqlx::query("DELETE FROM local_agent_runs WHERE run_id = ?")
        .bind(&original.run_id)
        .execute(&storage.pool)
        .await
        .unwrap();
    let newer = owned_run("newer-run", "user-1", 86_401_001);
    storage
        .create_run(
            &command("newer-create", "newer-input"),
            &newer,
            "newer-event",
        )
        .await
        .unwrap();
    assert!(matches!(
        storage
            .create_run(&original_command, &original, "repeated-event")
            .await,
        Err(ClientStorageError::ReplayExpired(_))
    ));
    assert!(storage.get_run(&original.run_id).await.unwrap().is_none());
    let events: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM local_agent_events")
        .fetch_one(&storage.pool)
        .await
        .unwrap();
    assert_eq!(events, 1, "only the newer Run's event remains");
}

#[tokio::test]
async fn receipt_failure_rolls_back_business_write_event_and_identity() {
    let storage = SqliteClientStorage::connect_memory().await.unwrap();
    sqlx::query("CREATE TRIGGER injected_receipt_failure BEFORE INSERT ON local_agent_command_tombstones BEGIN SELECT RAISE(ABORT, 'injected receipt failure'); END")
        .execute(&storage.pool).await.unwrap();
    let create = command("retryable-create", "input");
    let original = run(1000);
    assert!(storage
        .create_run(&create, &original, "created-event")
        .await
        .is_err());
    assert!(storage.get_run(&original.run_id).await.unwrap().is_none());
    for table in [
        "local_agent_events",
        "local_agent_command_receipts",
        "local_agent_command_tombstones",
    ] {
        let count: i64 =
            sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT COUNT(*) FROM {table}")))
                .fetch_one(&storage.pool)
                .await
                .unwrap();
        assert_eq!(count, 0, "{table} must roll back");
    }
    sqlx::query("DROP TRIGGER injected_receipt_failure")
        .execute(&storage.pool)
        .await
        .unwrap();
    storage
        .create_run(&create, &original, "created-event")
        .await
        .unwrap();
}

#[tokio::test]
async fn concurrent_same_key_writes_share_one_response_and_one_side_effect() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("client.sqlite");
    let storage = SqliteClientStorage::connect_file(&path).await.unwrap();
    let second_connection = SqliteClientStorage::connect_file(&path).await.unwrap();
    let create = command("concurrent-create", "identical-input");
    let original = run(1000);
    let (first, second) = tokio::join!(
        storage.create_run(&create, &original, "first-event"),
        second_connection.create_run(&create, &original, "second-event"),
    );
    assert_eq!(first.unwrap(), second.unwrap());
    let events: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM local_agent_events")
        .fetch_one(&storage.pool)
        .await
        .unwrap();
    assert_eq!(events, 1);
    let identities: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM local_agent_command_tombstones")
        .fetch_one(&storage.pool)
        .await
        .unwrap();
    assert_eq!(identities, 1);
    storage.pool.close().await;
    second_connection.pool.close().await;
}

#[tokio::test]
async fn compact_fingerprint_replays_legacy_receipt_and_rejects_changed_request() {
    use sha2::{Digest, Sha256};

    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    let legacy = r#"{"type":"create_run","owner_user_id":"user-1","input":"original"}"#;
    let fingerprint = format!("sha256:v1:{:x}", Sha256::digest(legacy.as_bytes()));
    let original = run(1_000);
    let created = storage
        .create_run(
            &command("legacy-create", legacy),
            &original,
            "created-event",
        )
        .await
        .expect("legacy create");
    let replay = storage
        .create_run(
            &command("legacy-create", &fingerprint),
            &original,
            "unused-event",
        )
        .await
        .expect("replay after upgrade");
    assert_eq!(created, replay);

    let changed = format!("sha256:v1:{:x}", Sha256::digest(b"changed request"));
    let error = storage
        .create_run(
            &command("legacy-create", &changed),
            &original,
            "unused-event",
        )
        .await
        .expect_err("same command ID with changed input must fail");
    assert!(matches!(error, ClientStorageError::CommandMismatch(_)));
    let events: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM local_agent_events")
        .fetch_one(&storage.pool)
        .await
        .expect("event count");
    assert_eq!(events, 1);

    let compact_run = owned_run("run-compact", "user-1", 2_000);
    let compact_command = command("compact-create", &fingerprint);
    storage
        .create_run(&compact_command, &compact_run, "compact-event")
        .await
        .expect("compact create");
    storage
        .create_run(&compact_command, &compact_run, "unused-compact-event")
        .await
        .expect("compact replay");
    let stored: String = sqlx::query_scalar(
        "SELECT request_fingerprint FROM local_agent_command_receipts WHERE command_id = 'compact-create'"
    ).fetch_one(&storage.pool).await.expect("stored fingerprint");
    assert_eq!(stored.len(), 74);
    assert_eq!(stored, fingerprint);
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
async fn expired_model_step_is_requeued_without_user_review() {
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
    assert_eq!(recovered.status, LocalAgentRunStatus::ModelReady);
    assert_eq!(recovered.iteration, 0);
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
            .next_retry_at("user-1")
            .await
            .expect("user-1 claim expiry"),
        Some(3_000)
    );
    assert_eq!(
        storage
            .next_retry_at("user-2")
            .await
            .expect("user-2 claim expiry"),
        Some(3_000)
    );

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
        LocalAgentRunStatus::ModelReady
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
            .next_retry_at("user-1")
            .await
            .expect("user-1 claim expiry"),
        Some(3_000)
    );
    assert_eq!(
        storage
            .next_retry_at("user-2")
            .await
            .expect("user-2 claim expiry"),
        Some(3_000)
    );

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
            None,
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
