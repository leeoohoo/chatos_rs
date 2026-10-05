// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

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
