// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::fixtures::*;
use super::*;

#[tokio::test]
async fn owner_step_error_is_committed_as_terminal_failure() {
    #[derive(Clone)]
    struct FailingExecutor;

    #[async_trait]
    impl CloudAgentSingleStepExecutor for FailingExecutor {
        async fn execute_single_step(
            &self,
            _run: &CloudAgentRunRecord,
            _trigger: &CloudAgentModelTrigger,
        ) -> Result<CloudAgentSingleStepExecution, String> {
            Err("required MCPs cannot be materialized".to_string())
        }
    }

    let store = inserted_ready_run().await;
    assert_eq!(
        consume_cloud_agent_single_step(&store, &FailingExecutor, consume_input())
            .await
            .unwrap(),
        CloudAgentConsumeDisposition::Committed
    );

    let persisted = store.load_run("run-1").await.unwrap().unwrap();
    assert_eq!(persisted.status, CloudAgentRunStatus::Failed);
    assert_eq!(persisted.phase, CloudAgentRunPhase::Terminal);
    assert_eq!(
        persisted
            .terminal_outcome
            .as_ref()
            .and_then(|value| value.get("error"))
            .and_then(Value::as_str),
        Some("required MCPs cannot be materialized")
    );
    let outbox = store.list_ready_outbox(10).await.unwrap();
    assert_eq!(outbox.len(), 1);
    assert_eq!(outbox[0].topic, "owner_lifecycle_terminal");
}

#[tokio::test]
async fn execution_deadline_stops_an_in_flight_model_step_and_commits_failure() {
    let store = InMemoryCloudAgentRunStore::new();
    store.allocate_lane_seq("task:task-1").await.unwrap();
    let mut record = run();
    record.status = CloudAgentRunStatus::ModelReady;
    record.phase = CloudAgentRunPhase::Ready;
    record.ordering.step_seq = 1;
    record.iteration = 0;
    record.version = 1;
    record.deadline_at = Some(Utc::now() + chrono::Duration::milliseconds(30));
    store.insert_run(record).await.unwrap();

    let slow = SlowSingleStepExecutor {
        delay: std::time::Duration::from_millis(200),
    };
    assert_eq!(
        consume_cloud_agent_single_step(&store, &slow, consume_input())
            .await
            .unwrap(),
        CloudAgentConsumeDisposition::Committed
    );

    let persisted = store.load_run("run-1").await.unwrap().unwrap();
    assert_eq!(persisted.status, CloudAgentRunStatus::Failed);
    assert_eq!(
        persisted
            .terminal_outcome
            .as_ref()
            .and_then(|value| value.get("error"))
            .and_then(Value::as_str),
        Some("Cloud Agent execution deadline exceeded")
    );
}

#[tokio::test]
async fn shared_run_factory_allocates_the_lane_and_start_event_atomically() {
    let store = CloudAgentStateStore::memory();
    let record = create_cloud_agent_run(
        &store,
        NewCloudAgentRun {
            ordering_lane_key: "conversation:session-1".to_string(),
            agent_run_id: "turn-1".to_string(),
            owner_service: "chatos".to_string(),
            owner_entity_type: "conversation_turn".to_string(),
            owner_entity_id: "turn-1".to_string(),
            owner_user_id: "user-1".to_string(),
            agent_key: "chatos_conversation_agent".to_string(),
            input: serde_json::json!({"content": "hello"}),
            model_config_ref: "model-1".to_string(),
            model_runtime_snapshot_ref: "turn-1:model".to_string(),
            agent_prompt_revision: "1".to_string(),
            agent_prompt_checksum: "checksum-1".to_string(),
            capability_policy_revision: "policy-1".to_string(),
            mcp_runtime_session_ref: Some("session-1".to_string()),
            current_input_items_ref: "turn-1:initial".to_string(),
            max_iterations: 10,
            deadline_at: None,
            runtime_routing_key: "cloud_agent.chatos.runtime".to_string(),
            start_causation_id: "message-1".to_string(),
            start_payload: serde_json::json!({"conversation_id": "session-1"}),
        },
    )
    .await
    .unwrap();

    assert_eq!(record.ordering.lane_seq, 1);
    assert_eq!(record.status, CloudAgentRunStatus::ModelReady);
    let outbox = store.list_ready_outbox(10).await.unwrap();
    assert_eq!(outbox.len(), 1);
    assert_eq!(outbox[0].ordering, record.ordering);
    assert_eq!(outbox[0].payload["event_type"], "run_started");
    assert_eq!(outbox[0].payload["conversation_id"], "session-1");
}

#[tokio::test]
async fn owner_input_update_commits_with_the_same_model_step() {
    let store = inserted_ready_run().await;
    let executor = TestSingleStepExecutor {
        outcome: AiSingleStepOutcome::Final(AiRuntimeResult {
            content: "done".to_string(),
            reasoning: None,
            tool_calls: None,
            finish_reason: Some("stop".to_string()),
            usage: None,
            response_id: Some("response-final".to_string()),
            response_output_items: Vec::new(),
            request_input_items: Vec::new(),
        }),
        seen_triggers: Arc::new(Mutex::new(Vec::new())),
    };

    #[derive(Clone)]
    struct InputUpdatingExecutor(TestSingleStepExecutor);

    #[async_trait]
    impl CloudAgentSingleStepExecutor for InputUpdatingExecutor {
        async fn execute_single_step(
            &self,
            run: &CloudAgentRunRecord,
            trigger: &CloudAgentModelTrigger,
        ) -> Result<CloudAgentSingleStepExecution, String> {
            let CloudAgentSingleStepExecution::Apply(output) =
                self.0.execute_single_step(run, trigger).await?
            else {
                unreachable!();
            };
            Ok(CloudAgentSingleStepExecution::Apply(Box::new(
                (*output).with_next_input(serde_json::json!({"lifecycle_round": 2})),
            )))
        }
    }

    consume_cloud_agent_single_step(&store, &InputUpdatingExecutor(executor), consume_input())
        .await
        .unwrap();
    let persisted = store.load_run("run-1").await.unwrap().unwrap();
    assert_eq!(persisted.input, serde_json::json!({"lifecycle_round": 2}));
    assert_eq!(persisted.status, CloudAgentRunStatus::Succeeded);
}

#[tokio::test]
async fn slow_model_step_renews_claim_and_blocks_duplicate_consumer() {
    let store = inserted_ready_run().await;
    let slow = SlowSingleStepExecutor {
        delay: std::time::Duration::from_millis(120),
    };
    let mut input = consume_input();
    input.claim_until = Utc::now() + chrono::Duration::milliseconds(45);

    let competing_store = store.clone();
    let competing = async move {
        tokio::time::sleep(std::time::Duration::from_millis(70)).await;
        competing_store
            .acquire_short_claim(&CloudAgentClaim {
                ordering: ordering(),
                expected_status: CloudAgentRunStatus::ModelReady,
                expected_phase: CloudAgentRunPhase::Ready,
                expected_version: 1,
                claim_token: "duplicate-claim".to_string(),
                claim_until: Utc::now() + chrono::Duration::seconds(30),
            })
            .await
            .unwrap()
    };

    let (consumed, competing_result) = tokio::join!(
        consume_cloud_agent_single_step(&store, &slow, input),
        competing,
    );

    assert_eq!(consumed.unwrap(), CloudAgentConsumeDisposition::Committed);
    assert_eq!(competing_result, CloudAgentClaimResult::Conflict);
    assert_eq!(
        store.load_run("run-1").await.unwrap().unwrap().status,
        CloudAgentRunStatus::Succeeded
    );
}

#[derive(Clone)]
struct TestProfile {
    executions: Arc<Mutex<Vec<String>>>,
    finalizations: Arc<Mutex<Vec<String>>>,
}

#[derive(Clone)]
struct SlowSingleStepExecutor {
    delay: std::time::Duration,
}

#[async_trait]
impl CloudAgentSingleStepExecutor for SlowSingleStepExecutor {
    async fn execute_single_step(
        &self,
        _run: &CloudAgentRunRecord,
        _trigger: &CloudAgentModelTrigger,
    ) -> Result<CloudAgentSingleStepExecution, String> {
        tokio::time::sleep(self.delay).await;
        Ok(CloudAgentSingleStepExecution::Apply(Box::new(
            CloudAgentSingleStepOutput::new(AiSingleStepOutcome::Final(AiRuntimeResult {
                content: "done".to_string(),
                reasoning: None,
                tool_calls: None,
                finish_reason: Some("stop".to_string()),
                usage: None,
                response_id: Some("response-slow".to_string()),
                response_output_items: Vec::new(),
                request_input_items: Vec::new(),
            })),
        )))
    }
}

#[async_trait]
impl CloudAgentProfile for TestProfile {
    async fn execute_single_step(
        &self,
        run: &CloudAgentRunRecord,
        _trigger: &CloudAgentModelTrigger,
    ) -> Result<CloudAgentSingleStepExecution, String> {
        self.executions.lock().unwrap().push(run.agent_key.clone());
        Ok(CloudAgentSingleStepExecution::Apply(Box::new(
            CloudAgentSingleStepOutput::new(AiSingleStepOutcome::Final(AiRuntimeResult {
                content: "done".to_string(),
                reasoning: None,
                tool_calls: None,
                finish_reason: Some("stop".to_string()),
                usage: None,
                response_id: Some("response-final".to_string()),
                response_output_items: Vec::new(),
                request_input_items: Vec::new(),
            })),
        )))
    }

    async fn finalize_terminal(&self, run: &CloudAgentRunRecord) -> Result<(), String> {
        self.finalizations
            .lock()
            .unwrap()
            .push(run.agent_key.clone());
        Ok(())
    }
}

#[tokio::test]
async fn one_profile_registration_serves_multiple_agent_keys() {
    let executions = Arc::new(Mutex::new(Vec::new()));
    let finalizations = Arc::new(Mutex::new(Vec::new()));
    let registry = CloudAgentProfileRegistry::new("task-runner", CloudAgentStateStore::memory())
        .register(
            ["task_runner_plan_phase", "task_runner_run_phase"],
            TestProfile {
                executions: Arc::clone(&executions),
                finalizations: Arc::clone(&finalizations),
            },
        )
        .unwrap();
    let trigger = CloudAgentModelTrigger::RunStarted {
        event_id: "start-1".to_string(),
        payload: Value::Null,
    };

    for key in ["task_runner_plan_phase", "task_runner_run_phase"] {
        let mut record = run();
        record.agent_key = key.to_string();
        registry
            .execute_single_step(&record, &trigger)
            .await
            .unwrap();
    }

    assert_eq!(
        executions.lock().unwrap().as_slice(),
        ["task_runner_plan_phase", "task_runner_run_phase"]
    );
    assert!(finalizations.lock().unwrap().is_empty());
}

#[tokio::test]
async fn registry_rejects_unknown_agent_keys_and_wrong_owners() {
    let registry = CloudAgentProfileRegistry::new("task-runner", CloudAgentStateStore::memory())
        .register(
            ["task_runner_run_phase"],
            TestProfile {
                executions: Arc::new(Mutex::new(Vec::new())),
                finalizations: Arc::new(Mutex::new(Vec::new())),
            },
        )
        .unwrap();
    let trigger = CloudAgentModelTrigger::RunStarted {
        event_id: "start-1".to_string(),
        payload: Value::Null,
    };

    let mut unknown = run();
    unknown.agent_key = "unknown_agent".to_string();
    assert!(registry
        .execute_single_step(&unknown, &trigger)
        .await
        .unwrap_err()
        .contains("not registered"));

    let mut wrong_owner = run();
    wrong_owner.owner_service = "chatos".to_string();
    assert!(registry
        .execute_single_step(&wrong_owner, &trigger)
        .await
        .unwrap_err()
        .contains("owner mismatch"));
}
