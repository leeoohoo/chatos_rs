// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

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
async fn active_run_pages_hide_review_attempts_after_the_task_moves_on() {
    let storage = SqliteClientStorage::connect_memory()
        .await
        .expect("storage");
    let mut review = run(1_000);
    review.run_id = "run-review".to_string();
    review.owner_entity_type = "task".to_string();
    review.owner_entity_id = "task-1".to_string();
    review.profile_key = "task_execution".to_string();
    review.status = LocalAgentRunStatus::NeedsReview;
    storage
        .create_run(
            &command("create-review-run", "review"),
            &review,
            "event-review",
        )
        .await
        .expect("review Run");
    sqlx::query(
        "INSERT INTO local_task_graphs (graph_id, owner_user_id, source_entity_type, \
         source_entity_id, created_at_unix_ms) VALUES ('graph-1', 'user-1', \
         'conversation_turn', 'turn-1', 1000)",
    )
    .execute(&storage.pool)
    .await
    .expect("task graph");
    sqlx::query(
        "INSERT INTO local_tasks (task_id, graph_id, title, profile_key, model_config_ref, \
         model_config_revision, capability_policy_revision, input_json, max_iterations, status, \
         active_run_id, version, created_at_unix_ms, updated_at_unix_ms) VALUES \
         ('task-1', 'graph-1', 'Review task', 'task_execution', 'model-1', 'revision-1', \
         'policy-1', '{}', 8, 'blocked', NULL, 1, 1000, 1000)",
    )
    .execute(&storage.pool)
    .await
    .expect("blocked task");

    let blocked = storage
        .list_runs(
            "user-1",
            LocalAgentRunListScope::Active,
            None,
            None,
            None,
            None,
            10,
        )
        .await
        .expect("blocked activity page");
    assert_eq!(blocked.runs.len(), 1);
    assert_eq!(blocked.runs[0].run_id, "run-review");

    sqlx::query(
        "UPDATE local_tasks SET status = 'succeeded', version = version + 1, \
         updated_at_unix_ms = 2000 WHERE task_id = 'task-1'",
    )
    .execute(&storage.pool)
    .await
    .expect("completed task");
    let completed = storage
        .list_runs(
            "user-1",
            LocalAgentRunListScope::Active,
            None,
            None,
            None,
            None,
            10,
        )
        .await
        .expect("completed activity page");
    assert!(completed.runs.is_empty());
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
