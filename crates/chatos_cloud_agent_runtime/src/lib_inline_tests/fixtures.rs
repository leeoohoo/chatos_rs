// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[derive(Clone)]
pub(super) struct TestSingleStepExecutor {
    pub(super) outcome: AiSingleStepOutcome,
    pub(super) seen_triggers: Arc<Mutex<Vec<CloudAgentModelTrigger>>>,
}

#[async_trait]
impl CloudAgentSingleStepExecutor for TestSingleStepExecutor {
    async fn execute_single_step(
        &self,
        _run: &CloudAgentRunRecord,
        trigger: &CloudAgentModelTrigger,
    ) -> Result<CloudAgentSingleStepExecution, String> {
        self.seen_triggers.lock().unwrap().push(trigger.clone());
        Ok(CloudAgentSingleStepExecution::Apply(Box::new(
            CloudAgentSingleStepOutput::new(self.outcome.clone())
                .with_mcp_runtime("session-1", "mcp.commands")
                .with_retry_input_items(vec![serde_json::json!({"type": "message"})]),
        )))
    }
}

pub(super) async fn inserted_ready_run() -> InMemoryCloudAgentRunStore {
    let store = InMemoryCloudAgentRunStore::new();
    store.allocate_lane_seq("task:task-1").await.unwrap();
    let mut record = run();
    record.status = CloudAgentRunStatus::ModelReady;
    record.phase = CloudAgentRunPhase::Ready;
    record.ordering.step_seq = 1;
    record.iteration = 0;
    record.version = 1;
    store.insert_run(record).await.unwrap();
    store
}

pub(super) fn consume_input() -> CloudAgentConsumeInput {
    CloudAgentConsumeInput {
        agent_run_id: "run-1".to_string(),
        event_id: "run-started-1".to_string(),
        trigger: CloudAgentModelTrigger::RunStarted {
            event_id: "run-started-1".to_string(),
            payload: Value::Null,
        },
        expected_status: CloudAgentRunStatus::ModelReady,
        expected_phase: CloudAgentRunPhase::Ready,
        claim_token: "claim-single-step".to_string(),
        claim_until: Utc::now() + chrono::Duration::seconds(30),
        output_routing_key: "cloud_agent.task_runner.runtime".to_string(),
    }
}
