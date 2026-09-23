// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use chatos_cloud_agent_protocol::{CloudAgentOrdering, CloudAgentRunPhase, CloudAgentRunStatus};
use serde_json::Value;

fn run_record(lane_key: &str, run_id: &str, lane_seq: u64) -> CloudAgentRunRecord {
    let now = Utc::now();
    CloudAgentRunRecord {
        ordering: CloudAgentOrdering {
            ordering_lane_key: lane_key.to_string(),
            lane_seq,
            agent_run_id: run_id.to_string(),
            generation: 1,
            step_seq: 1,
        },
        owner_service: "task-runner".to_string(),
        owner_entity_type: "task_run".to_string(),
        owner_entity_id: run_id.to_string(),
        owner_user_id: "postgres-contract-user".to_string(),
        agent_key: "task_runner_run_phase".to_string(),
        input: Value::Null,
        status: CloudAgentRunStatus::ModelReady,
        phase: CloudAgentRunPhase::Ready,
        iteration: 0,
        model_config_ref: "model-contract".to_string(),
        model_runtime_snapshot_ref: "snapshot-contract".to_string(),
        agent_prompt_revision: "1".to_string(),
        agent_prompt_checksum: "checksum-contract".to_string(),
        capability_policy_revision: "policy-contract".to_string(),
        mcp_runtime_session_ref: None,
        previous_response_id: None,
        continuation_mode: None,
        pending_batch_id: None,
        pending_tool_calls: Vec::new(),
        pending_tool_results: Vec::new(),
        response_input_items: Vec::new(),
        current_input_items_ref: format!("task_run:{run_id}:input"),
        usage_accumulator: Value::Null,
        max_iterations: 10,
        retry_count: 0,
        deadline_at: None,
        cancel_requested: false,
        terminal_outcome: None,
        version: 1,
        created_at: now,
        updated_at: now,
    }
}

fn outbox_intent(run: &CloudAgentRunRecord) -> CloudAgentOutboxIntent {
    CloudAgentOutboxIntent {
        event_id: format!("{}:started", run.ordering.agent_run_id),
        topic: "run_started".to_string(),
        routing_key: "cloud_agent.task_runner.runtime".to_string(),
        ordering: run.ordering.clone(),
        causation_id: "postgres-contract-cause".to_string(),
        correlation_id: "postgres-contract-correlation".to_string(),
        available_at: Utc::now() - chrono::Duration::seconds(1),
        payload: serde_json::json!({"contract": true}),
    }
}

#[tokio::test]
#[ignore = "requires TASK_RUNNER_TEST_DATABASE_URL and migrated PostgreSQL"]
async fn state_store_preserves_lane_claim_transition_and_outbox_contract() {
    let database_url = std::env::var("TASK_RUNNER_TEST_DATABASE_URL")
        .expect("TASK_RUNNER_TEST_DATABASE_URL must be set");
    let config = chatos_postgres::PostgresConfig::new(database_url).expect("test config");
    let pool = chatos_postgres::connect(&config).await.expect("test pool");
    let store = CloudAgentPostgresStore::new(pool.clone());
    let suffix = uuid::Uuid::new_v4();
    let lane_key = format!("postgres-contract:{suffix}");
    let first_id = format!("postgres-contract-first:{suffix}");
    let second_id = format!("postgres-contract-second:{suffix}");

    let first_seq = store
        .allocate_lane_seq(&lane_key)
        .await
        .expect("first lane");
    let second_seq = store
        .allocate_lane_seq(&lane_key)
        .await
        .expect("second lane");
    assert_eq!((first_seq, second_seq), (1, 2));

    let first = run_record(&lane_key, &first_id, first_seq);
    let second = run_record(&lane_key, &second_id, second_seq);
    let initial_outbox = outbox_intent(&first);
    store
        .insert_run_with_outbox(first.clone(), vec![initial_outbox.clone()])
        .await
        .expect("insert first run and outbox");
    store
        .insert_run_with_outbox(second.clone(), Vec::new())
        .await
        .expect("insert second run");

    let second_claim = CloudAgentClaim {
        ordering: second.ordering.clone(),
        expected_status: second.status,
        expected_phase: second.phase,
        expected_version: second.version,
        claim_token: "second-token".to_string(),
        claim_until: Utc::now() + chrono::Duration::seconds(30),
    };
    assert_eq!(
        store
            .acquire_short_claim(&second_claim)
            .await
            .expect("out-of-order claim"),
        CloudAgentClaimResult::OutOfOrder
    );

    let first_claim = CloudAgentClaim {
        ordering: first.ordering.clone(),
        expected_status: first.status,
        expected_phase: first.phase,
        expected_version: first.version,
        claim_token: "first-token".to_string(),
        claim_until: Utc::now() + chrono::Duration::seconds(30),
    };
    assert_eq!(
        store
            .acquire_short_claim(&first_claim)
            .await
            .expect("first claim"),
        CloudAgentClaimResult::Acquired
    );
    let mut wrong_token = first_claim.clone();
    wrong_token.claim_token = "wrong-token".to_string();
    assert!(!store
        .renew_short_claim(&wrong_token)
        .await
        .expect("wrong-token renewal"));

    assert!(store
        .commit_transition(CloudAgentAtomicTransition {
            claim: first_claim.clone(),
            next_input: Value::Null,
            next_status: CloudAgentRunStatus::Succeeded,
            next_phase: CloudAgentRunPhase::Terminal,
            next_step_seq: 2,
            next_iteration: 1,
            next_retry_count: 0,
            previous_response_id: None,
            continuation_mode: None,
            current_input_items_ref: format!("task_run:{first_id}:terminal"),
            mcp_runtime_session_ref: None,
            pending_batch_id: None,
            pending_tool_calls: Vec::new(),
            pending_tool_results: Vec::new(),
            response_input_items: Vec::new(),
            usage_accumulator: Value::Null,
            terminal_outcome: Some(serde_json::json!({"ok": true})),
            outbox: vec![initial_outbox.clone()],
        })
        .await
        .expect("terminal transition"));
    assert!(!store
        .renew_short_claim(&first_claim)
        .await
        .expect("stale renewal"));
    assert_eq!(
        store
            .acquire_short_claim(&second_claim)
            .await
            .expect("second claim after lane advance"),
        CloudAgentClaimResult::Acquired
    );

    let first_publish_token = format!("publisher-first:{suffix}");
    let pending = store
        .claim_ready_outbox_with_attempts(
            10,
            &first_publish_token,
            Utc::now() + chrono::Duration::seconds(30),
        )
        .await
        .expect("pending outbox");
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].intent.event_id, initial_outbox.event_id);
    assert_eq!(pending[0].publish_attempts, 0);
    assert!(store
        .claim_ready_outbox_with_attempts(
            10,
            "publisher-blocked",
            Utc::now() + chrono::Duration::seconds(30),
        )
        .await
        .expect("active claim blocks a second publisher")
        .is_empty());
    assert!(!store
        .mark_claimed_outbox_published(&initial_outbox.event_id, "publisher-wrong")
        .await
        .expect("wrong publisher token"));
    let retry_at = Utc::now() - chrono::Duration::seconds(1);
    let failed = store
        .mark_claimed_outbox_publish_failed(
            &initial_outbox.event_id,
            &first_publish_token,
            "transient",
            retry_at,
            2,
        )
        .await
        .expect("record publish failure")
        .expect("pending outbox failure");
    assert_eq!(failed.publish_attempts, 1);
    assert!(!failed.dead_lettered);

    let publisher_a = format!("publisher-a:{suffix}");
    let publisher_b = format!("publisher-b:{suffix}");
    let claim_until = Utc::now() + chrono::Duration::seconds(30);
    let (claimed_a, claimed_b) = tokio::join!(
        store.claim_ready_outbox_with_attempts(10, &publisher_a, claim_until),
        store.claim_ready_outbox_with_attempts(10, &publisher_b, claim_until),
    );
    let claimed_a = claimed_a.expect("publisher A claim");
    let claimed_b = claimed_b.expect("publisher B claim");
    assert_eq!(claimed_a.len() + claimed_b.len(), 1);
    let winning_token = if claimed_a.is_empty() {
        publisher_b
    } else {
        publisher_a
    };

    sqlx::query(
        "UPDATE cloud_agent_outbox SET claim_until=now()-interval '1 second' WHERE event_id=$1",
    )
    .bind(&initial_outbox.event_id)
    .execute(&pool)
    .await
    .expect("expire publisher lease");
    let recovery_token = format!("publisher-recovery:{suffix}");
    assert_eq!(
        store
            .claim_ready_outbox_with_attempts(
                10,
                &recovery_token,
                Utc::now() + chrono::Duration::seconds(30),
            )
            .await
            .expect("recover expired publisher lease")
            .len(),
        1
    );
    assert!(!store
        .mark_claimed_outbox_published(&initial_outbox.event_id, &winning_token)
        .await
        .expect("stale publisher token"));
    assert!(store
        .mark_claimed_outbox_published(&initial_outbox.event_id, &recovery_token)
        .await
        .expect("publish outbox"));
    assert!(!store
        .mark_claimed_outbox_published(&initial_outbox.event_id, &recovery_token)
        .await
        .expect("idempotent publish"));

    let active_lane: i64 = sqlx::query_scalar(
        "SELECT active_lane_seq FROM cloud_agent_lanes WHERE ordering_lane_key=$1",
    )
    .bind(&lane_key)
    .fetch_one(&pool)
    .await
    .expect("active lane");
    assert_eq!(active_lane, 2);

    store
        .release_short_claim(&second_claim)
        .await
        .expect("release second claim");
    sqlx::query("DELETE FROM cloud_agent_runs WHERE ordering_lane_key=$1")
        .bind(&lane_key)
        .execute(&pool)
        .await
        .expect("delete contract runs");
    sqlx::query("DELETE FROM cloud_agent_lanes WHERE ordering_lane_key=$1")
        .bind(&lane_key)
        .execute(&pool)
        .await
        .expect("delete contract lane");
}
