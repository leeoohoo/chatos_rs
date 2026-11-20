#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chatos_ai_runtime::{AiRuntimeResult, AiSingleStepOutcome};
    use chatos_cloud_agent_protocol::CloudAgentRunRecord;
    use serde_json::Value;
    use std::sync::{Arc, Mutex};

    fn ordering() -> CloudAgentOrdering {
        CloudAgentOrdering {
            ordering_lane_key: "task:task-1".to_string(),
            lane_seq: 1,
            agent_run_id: "run-1".to_string(),
            generation: 1,
            step_seq: 2,
        }
    }

    fn run() -> CloudAgentRunRecord {
        let now = Utc::now();
        CloudAgentRunRecord {
            ordering: ordering(),
            owner_service: "task-runner".to_string(),
            owner_entity_type: "task_run".to_string(),
            owner_entity_id: "run-1".to_string(),
            owner_user_id: "user-1".to_string(),
            agent_key: "task_runner_run_phase".to_string(),
            input: Value::Null,
            status: CloudAgentRunStatus::ModelRequesting,
            phase: CloudAgentRunPhase::ModelRequest,
            iteration: 2,
            model_config_ref: "model-1".to_string(),
            model_runtime_snapshot_ref: "snapshot-1".to_string(),
            agent_prompt_revision: "1".to_string(),
            agent_prompt_checksum: "prompt-sha".to_string(),
            capability_policy_revision: "1".to_string(),
            mcp_runtime_session_ref: Some("session-1".to_string()),
            previous_response_id: None,
            continuation_mode: None,
            pending_batch_id: None,
            pending_tool_calls: Vec::new(),
            pending_tool_results: Vec::new(),
            response_input_items: Vec::new(),
            current_input_items_ref: "input-1".to_string(),
            usage_accumulator: Value::Null,
            max_iterations: 100,
            retry_count: 0,
            deadline_at: None,
            cancel_requested: false,
            terminal_outcome: None,
            version: 4,
            created_at: now,
            updated_at: now,
        }
    }

    fn claim() -> CloudAgentClaim {
        CloudAgentClaim {
            ordering: ordering(),
            expected_status: CloudAgentRunStatus::ModelRequesting,
            expected_phase: CloudAgentRunPhase::ModelRequest,
            expected_version: 4,
            claim_token: "claim-1".to_string(),
            claim_until: Utc::now() + chrono::Duration::seconds(30),
        }
    }

    include!("lib_inline_tests/reducer.rs");
    include!("lib_inline_tests/execution_transactions.rs");
    include!("lib_inline_tests/execution_lifecycle.rs");
}
