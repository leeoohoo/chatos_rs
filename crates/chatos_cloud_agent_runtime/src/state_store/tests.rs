// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use chatos_cloud_agent_protocol::{CloudAgentOrdering, CloudAgentRunPhase, CloudAgentRunStatus};
use chrono::Utc;
use serde_json::Value;

fn run_record(run_id: &str, lane_seq: u64) -> CloudAgentRunRecord {
    let now = Utc::now();
    CloudAgentRunRecord {
        ordering: CloudAgentOrdering {
            ordering_lane_key: "task:task-1".to_string(),
            lane_seq,
            agent_run_id: run_id.to_string(),
            generation: 1,
            step_seq: 1,
        },
        owner_service: "task-runner".to_string(),
        owner_entity_type: "task_run".to_string(),
        owner_entity_id: run_id.to_string(),
        owner_user_id: "user-1".to_string(),
        agent_key: "task_runner_run_phase".to_string(),
        input: Value::Null,
        status: CloudAgentRunStatus::ModelReady,
        phase: CloudAgentRunPhase::Ready,
        iteration: 0,
        model_config_ref: "model-1".to_string(),
        model_runtime_snapshot_ref: "snapshot-1".to_string(),
        agent_prompt_revision: "1".to_string(),
        agent_prompt_checksum: "checksum-1".to_string(),
        capability_policy_revision: "policy-1".to_string(),
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
        event_id: format!("{}:event", run.ordering.agent_run_id),
        topic: "run_started".to_string(),
        routing_key: "cloud_agent.test.runtime".to_string(),
        ordering: run.ordering.clone(),
        causation_id: "cause-1".to_string(),
        correlation_id: "correlation-1".to_string(),
        available_at: Utc::now() - chrono::Duration::seconds(1),
        payload: serde_json::json!({}),
    }
}

#[tokio::test]
async fn terminal_commit_advances_an_empty_lane_for_the_next_future_run() {
    let store = InMemoryCloudAgentRunStore::new();
    let first_seq = store.allocate_lane_seq("task:task-1").await.unwrap();
    let first = run_record("run-1", first_seq);
    store.insert_run(first.clone()).await.unwrap();
    let claim = CloudAgentClaim {
        ordering: first.ordering.clone(),
        expected_status: first.status,
        expected_phase: first.phase,
        expected_version: first.version,
        claim_token: "claim-1".to_string(),
        claim_until: Utc::now() + chrono::Duration::seconds(30),
    };
    assert_eq!(
        store.acquire_short_claim(&claim).await.unwrap(),
        CloudAgentClaimResult::Acquired
    );
    assert!(store
        .commit_transition(CloudAgentAtomicTransition {
            claim,
            next_input: Value::Null,
            next_status: CloudAgentRunStatus::Succeeded,
            next_phase: CloudAgentRunPhase::Terminal,
            next_step_seq: 2,
            next_iteration: 1,
            next_retry_count: 0,
            previous_response_id: None,
            continuation_mode: None,
            current_input_items_ref: "task_run:run-1:terminal".to_string(),
            mcp_runtime_session_ref: None,
            pending_batch_id: None,
            pending_tool_calls: Vec::new(),
            pending_tool_results: Vec::new(),
            response_input_items: Vec::new(),
            usage_accumulator: Value::Null,
            terminal_outcome: Some(serde_json::json!({"ok": true})),
            outbox: Vec::new(),
        })
        .await
        .unwrap());

    let second_seq = store.allocate_lane_seq("task:task-1").await.unwrap();
    assert_eq!(second_seq, 2);
    let second = run_record("run-2", second_seq);
    store.insert_run(second.clone()).await.unwrap();
    let second_claim = CloudAgentClaim {
        ordering: second.ordering.clone(),
        expected_status: second.status,
        expected_phase: second.phase,
        expected_version: second.version,
        claim_token: "claim-2".to_string(),
        claim_until: Utc::now() + chrono::Duration::seconds(30),
    };
    assert_eq!(
        store.acquire_short_claim(&second_claim).await.unwrap(),
        CloudAgentClaimResult::Acquired
    );
}

#[tokio::test]
async fn outbox_publish_failures_back_off_and_eventually_dead_letter() {
    let store = InMemoryCloudAgentRunStore::new();
    let lane_seq = store.allocate_lane_seq("task:task-1").await.unwrap();
    let run = run_record("run-outbox", lane_seq);
    let intent = outbox_intent(&run);
    store
        .insert_run_with_outbox(run, vec![intent.clone()])
        .await
        .unwrap();

    assert_eq!(store.list_ready_outbox(10).await.unwrap().len(), 1);
    let retry_at = Utc::now() + chrono::Duration::minutes(1);
    let first = store
        .mark_outbox_publish_failed(intent.event_id.as_str(), "publish failed", retry_at, 8)
        .await
        .unwrap()
        .expect("pending outbox failure");
    assert_eq!(first.publish_attempts, 1);
    assert!(!first.dead_lettered);
    assert!(store.list_ready_outbox(10).await.unwrap().is_empty());

    let mut latest = first;
    for _ in 2..=8 {
        latest = store
            .mark_outbox_publish_failed(
                intent.event_id.as_str(),
                "publish failed again",
                Utc::now() - chrono::Duration::seconds(1),
                8,
            )
            .await
            .unwrap()
            .expect("pending outbox failure");
    }
    assert_eq!(latest.publish_attempts, 8);
    assert!(latest.dead_lettered);
    assert!(store.list_ready_outbox(10).await.unwrap().is_empty());
}

#[test]
fn outbox_publish_errors_are_bounded() {
    assert_eq!(
        bounded_outbox_publish_error(&"x".repeat(3_000)).len(),
        2_000
    );
}
