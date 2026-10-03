// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use sqlx::Connection;
use std::time::Duration;
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
        persist_receipt: true,
    }
}

#[tokio::test]
async fn file_database_reads_remain_available_during_a_write_transaction() {
    let root = tempfile::tempdir().expect("temporary database root");
    let database_path = root.path().join("local-agent.sqlite");
    let storage = SqliteClientStorage::connect_file(&database_path)
        .await
        .expect("storage");
    assert_eq!(storage.pool.options().get_max_connections(), 4);

    let mut writer = storage.pool.acquire().await.expect("writer connection");
    SqliteClientStorage::begin_immediate(&mut writer)
        .await
        .expect("write transaction");
    let value = tokio::time::timeout(
        Duration::from_secs(1),
        sqlx::query_scalar::<_, i64>("SELECT 1").fetch_one(&storage.pool),
    )
    .await
    .expect("WAL reader must not wait for the writer")
    .expect("reader query");
    assert_eq!(value, 1);
    sqlx::query("ROLLBACK")
        .execute(&mut *writer)
        .await
        .expect("rollback");
}

#[tokio::test]
async fn memory_database_uses_one_shared_connection() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    assert_eq!(storage.pool.options().get_max_connections(), 1);
}

#[tokio::test]
async fn idle_claim_polls_do_not_persist_command_receipts() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    let run_command = command("idle-run-claim", "idle-run-claim");
    let tool_command = command("idle-tool-claim", "idle-tool-claim");

    let run_claim = storage
        .claim_next_run(
            &run_command,
            "user-1",
            "model-worker",
            "run-token",
            1_000,
            31_000,
            "idle-run-event",
        )
        .await
        .expect("empty Run claim");
    let tool_claim = storage
        .claim_next_tool(
            &tool_command,
            "user-1",
            "tool-worker",
            "tool-token",
            1_000,
            31_000,
            "idle-tool-event",
            None,
            &[],
        )
        .await
        .expect("empty Tool claim");
    assert!(run_claim.is_none());
    assert!(tool_claim.is_none());

    let receipt_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM local_agent_command_receipts")
            .fetch_one(&storage.pool)
            .await
            .expect("receipt count");
    assert_eq!(receipt_count, 0);

    storage
        .create_run(
            &command("create-after-idle", "create"),
            &run(2_000),
            "event-created",
        )
        .await
        .expect("create after idle poll");
    let claimed = storage
        .claim_next_run(
            &run_command,
            "user-1",
            "model-worker",
            "run-token",
            2_001,
            32_001,
            "claimed-run-event",
        )
        .await
        .expect("claim after idle poll");
    assert!(claimed.is_some());
}

#[tokio::test]
async fn ephemeral_internal_command_mutates_state_without_persisting_a_receipt() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    let ephemeral = IdempotentCommand {
        command_id: "internal-scheduler-create".to_string(),
        request_fingerprint: "create".to_string(),
        persist_receipt: false,
    };

    storage
        .create_run(&ephemeral, &run(2_000), "event-created")
        .await
        .expect("ephemeral create");

    assert!(storage.get_run("run-1").await.expect("get Run").is_some());
    let receipt_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM local_agent_command_receipts")
            .fetch_one(&storage.pool)
            .await
            .expect("receipt count");
    assert_eq!(receipt_count, 0);
}

#[tokio::test]
async fn owner_event_poll_plan_starts_from_the_cursor_without_a_temp_sort() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    let rows = sqlx::query(
        "EXPLAIN QUERY PLAN \
         SELECT e.cursor, e.event_id, e.run_id, e.event_type, e.payload_json, \
         e.created_at_unix_ms FROM local_agent_events e WHERE e.cursor > ? \
         AND EXISTS (SELECT 1 FROM local_agent_runs r \
           WHERE r.run_id = e.run_id AND r.owner_user_id = ?) \
         ORDER BY e.cursor LIMIT ?",
    )
    .bind(100_i64)
    .bind("user-1")
    .bind(100_i64)
    .fetch_all(&storage.pool)
    .await
    .expect("event poll query plan");
    let details = rows
        .iter()
        .map(|row| row.get::<String, _>("detail"))
        .collect::<Vec<_>>();
    assert!(details
        .iter()
        .any(|detail| detail.contains("SEARCH e USING INTEGER PRIMARY KEY")));
    assert!(!details
        .iter()
        .any(|detail| detail.contains("USE TEMP B-TREE")));
}

#[tokio::test]
async fn source_task_graph_plan_uses_the_compound_source_index() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    let rows = sqlx::query(
        "EXPLAIN QUERY PLAN \
         SELECT g.graph_id, MAX(t.updated_at_unix_ms) AS updated_at_unix_ms \
         FROM local_task_graphs g JOIN local_tasks t ON t.graph_id = g.graph_id \
         WHERE g.owner_user_id = ? AND g.source_entity_type = ? \
           AND g.source_entity_id = ? GROUP BY g.graph_id",
    )
    .bind("user-1")
    .bind("conversation_turn")
    .bind("turn-1")
    .fetch_all(&storage.pool)
    .await
    .expect("source Task Graph query plan");
    let details = rows
        .iter()
        .map(|row| row.get::<String, _>("detail"))
        .collect::<Vec<_>>();
    assert!(details.iter().any(|detail| {
        detail.contains(
            "SEARCH g USING INDEX local_task_graphs_source \
             (owner_user_id=? AND source_entity_type=? AND source_entity_id=?)",
        )
    }));
}

#[tokio::test]
async fn recent_run_pages_filter_before_decode_and_use_the_owner_time_index() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    storage
        .create_run(&command("create-old-run", "old"), &run(1_000), "event-old")
        .await
        .expect("old Run");
    let mut recent = run(2_000);
    recent.run_id = "run-recent".to_string();
    recent.owner_entity_id = "conversation-recent".to_string();
    storage
        .create_run(
            &command("create-recent-run", "recent"),
            &recent,
            "event-recent",
        )
        .await
        .expect("recent Run");
    sqlx::query("UPDATE local_agent_runs SET input_json = 'invalid' WHERE run_id = 'run-1'")
        .execute(&storage.pool)
        .await
        .expect("corrupt filtered old input");
    sqlx::query(
        "UPDATE local_agent_runs SET checkpoint_json = 'invalid' WHERE run_id = 'run-recent'",
    )
    .execute(&storage.pool)
    .await
    .expect("corrupt unneeded recent checkpoint");

    let page = storage
        .list_runs(
            "user-1",
            LocalAgentRunListScope::All,
            None,
            Some(1_500),
            None,
            None,
            10,
        )
        .await
        .expect("recent Run page");
    assert_eq!(page.runs.len(), 1);
    assert_eq!(page.runs[0].run_id, "run-recent");

    let rows = sqlx::query(
        "EXPLAIN QUERY PLAN SELECT run_id FROM local_agent_runs \
         WHERE owner_user_id = ? AND updated_at_unix_ms >= ? \
         ORDER BY updated_at_unix_ms DESC, run_id DESC LIMIT ?",
    )
    .bind("user-1")
    .bind(1_500_i64)
    .bind(10_i64)
    .fetch_all(&storage.pool)
    .await
    .expect("recent Run query plan");
    let details = rows
        .iter()
        .map(|row| row.get::<String, _>("detail"))
        .collect::<Vec<_>>();
    assert!(details.iter().any(|detail| {
        detail.contains(
            "SEARCH local_agent_runs USING COVERING INDEX local_agent_runs_owner_updated \
             (owner_user_id=? AND updated_at_unix_ms>?)",
        )
    }));
    assert!(!details
        .iter()
        .any(|detail| detail.contains("USE TEMP B-TREE")));
}

#[tokio::test]
async fn run_status_filter_is_applied_before_the_page_limit() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    let mut waiting = run(1_000);
    waiting.run_id = "run-waiting".to_string();
    waiting.status = LocalAgentRunStatus::WaitingUser;
    storage
        .create_run(
            &command("create-waiting-run", "waiting"),
            &waiting,
            "event-waiting",
        )
        .await
        .expect("waiting Run");
    let mut newer = run(2_000);
    newer.run_id = "run-newer".to_string();
    storage
        .create_run(&command("create-newer-run", "newer"), &newer, "event-newer")
        .await
        .expect("newer Run");

    let page = storage
        .list_runs(
            "user-1",
            LocalAgentRunListScope::Active,
            Some(LocalAgentRunStatus::WaitingUser),
            None,
            None,
            None,
            1,
        )
        .await
        .expect("waiting Run page");
    assert_eq!(page.runs.len(), 1);
    assert_eq!(page.runs[0].run_id, "run-waiting");
}

#[tokio::test]
async fn owner_event_pages_project_payloads_inside_sqlite() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    storage
        .create_run(
            &command("create-payload-projection", "create"),
            &run(1_000),
            "event-payload-projection",
        )
        .await
        .expect("create run");
    sqlx::query("UPDATE local_agent_events SET payload_json = ? WHERE event_id = ?")
        .bind(r#"{"conversation_id":"conversation-1","large":"do-not-copy"}"#)
        .bind("event-payload-projection")
        .execute(&storage.pool)
        .await
        .expect("replace event payload");

    let full = storage
        .list_events_for_owner(
            "user-1",
            0,
            10,
            None,
            None,
            false,
            chatos_local_agent_protocol::LocalAgentEventPayloadMode::Full,
        )
        .await
        .expect("full payload");
    assert_eq!(full[0].payload["large"], "do-not-copy");

    let routing = storage
        .list_events_for_owner(
            "user-1",
            0,
            10,
            None,
            None,
            false,
            chatos_local_agent_protocol::LocalAgentEventPayloadMode::Routing,
        )
        .await
        .expect("routing payload");
    assert_eq!(
        routing[0].payload,
        serde_json::json!({"conversation_id": "conversation-1"})
    );

    sqlx::query("UPDATE local_agent_runs SET input_json = ? WHERE run_id = ?")
        .bind(r#"{"source_conversation_id":"conversation-from-run"}"#)
        .bind("run-1")
        .execute(&storage.pool)
        .await
        .expect("replace Run routing input");
    sqlx::query("UPDATE local_agent_events SET payload_json = ? WHERE event_id = ?")
        .bind(r#"{"large":"still-do-not-copy"}"#)
        .bind("event-payload-projection")
        .execute(&storage.pool)
        .await
        .expect("remove event routing payload");
    for run_id in [None, Some("run-1")] {
        let routed_from_run = storage
            .list_events_for_owner(
                "user-1",
                0,
                10,
                run_id,
                None,
                false,
                chatos_local_agent_protocol::LocalAgentEventPayloadMode::Routing,
            )
            .await
            .expect("Run-derived routing payload");
        assert_eq!(
            routed_from_run[0].payload,
            serde_json::json!({"conversation_id": "conversation-from-run"})
        );
    }

    let none = storage
        .list_events_for_owner(
            "user-1",
            0,
            10,
            None,
            None,
            false,
            chatos_local_agent_protocol::LocalAgentEventPayloadMode::None,
        )
        .await
        .expect("no payload");
    assert!(none[0].payload.is_null());
}

#[tokio::test]
async fn newest_run_event_filters_before_limit_and_uses_run_cursor_index() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    storage
        .create_run(
            &command("create-event-filter-run", "create"),
            &run(1_000),
            "event-created",
        )
        .await
        .expect("create run");
    let mut connection = storage.pool.acquire().await.expect("connection");
    SqliteClientStorage::insert_event(
        &mut connection,
        "prompt-old",
        "run-1",
        "user_input_requested",
        &serde_json::json!({"sequence": 1}),
        2_000,
    )
    .await
    .expect("old prompt");
    SqliteClientStorage::insert_event(
        &mut connection,
        "unrelated-newer",
        "run-1",
        "run_progressed",
        &serde_json::json!({"sequence": 99}),
        3_000,
    )
    .await
    .expect("unrelated event");
    SqliteClientStorage::insert_event(
        &mut connection,
        "prompt-new",
        "run-1",
        "user_input_requested",
        &serde_json::json!({"sequence": 2}),
        4_000,
    )
    .await
    .expect("new prompt");
    drop(connection);

    let events = storage
        .list_events_for_owner(
            "user-1",
            0,
            1,
            Some("run-1"),
            Some("user_input_requested"),
            true,
            chatos_local_agent_protocol::LocalAgentEventPayloadMode::Full,
        )
        .await
        .expect("latest prompt");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_id, "prompt-new");
    assert_eq!(events[0].payload["sequence"], 2);

    let rows = sqlx::query(
        "EXPLAIN QUERY PLAN SELECT e.cursor FROM local_agent_events e \
         INNER JOIN local_agent_runs r ON r.run_id = e.run_id \
         WHERE r.owner_user_id = ? AND e.cursor > ? AND e.run_id = ? \
         AND e.event_type = ? ORDER BY e.cursor DESC LIMIT ?",
    )
    .bind("user-1")
    .bind(0_i64)
    .bind("run-1")
    .bind("user_input_requested")
    .bind(1_i64)
    .fetch_all(&storage.pool)
    .await
    .expect("latest prompt query plan");
    let details = rows
        .iter()
        .map(|row| row.get::<String, _>("detail"))
        .collect::<Vec<_>>();
    assert!(details.iter().any(|detail| {
        detail
            .contains("SEARCH e USING INDEX local_agent_events_run_cursor (run_id=? AND cursor>?)")
    }));
    assert!(!details
        .iter()
        .any(|detail| detail.contains("USE TEMP B-TREE")));
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
    assert_eq!(schema_version, 26);
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
