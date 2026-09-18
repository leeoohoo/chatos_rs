// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

//! Owner-local state reducer: performs one claimed transition and returns outbox intents.

use chatos_cloud_agent_protocol::{CloudAgentOrdering, CloudAgentRunPhase, CloudAgentRunStatus};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

mod execution;
mod input_history;
mod input_projection;
mod rabbitmq_driver;
mod reducer;
mod run_contract;
mod state_repository;
mod state_store;

pub use execution::{
    consume_cloud_agent_single_step, CloudAgentConsumeDisposition, CloudAgentProfile,
    CloudAgentProfileRegistry, CloudAgentSingleStepExecution, CloudAgentSingleStepExecutor,
    CloudAgentSingleStepOutput,
};
pub use input_projection::{
    cloud_agent_mcp_result_callback_payload, cloud_agent_mcp_result_input_items,
    cloud_agent_trigger_execution_identity, cloud_agent_trigger_input_items,
};
pub use rabbitmq_driver::{
    publish_cloud_agent_intent, spawn_cloud_agent_consumer, spawn_cloud_agent_outbox_reconciler,
    CloudAgentQueueOwner, CloudAgentRabbitMqTopology, CloudAgentServiceAdapter,
    CloudAgentServiceRuntime,
};
pub use reducer::{materialize_mcp_command, reduce_single_step, CloudAgentModelTrigger};
pub use run_contract::{
    create_cloud_agent_run, CloudAgentAtomicTransition, CloudAgentClaimResult,
    CloudAgentConsumeInput, CloudAgentOutboxIntent, CloudAgentRunStore, NewCloudAgentRun,
};
pub use state_repository::{
    apply_cloud_agent_transition, bounded_cloud_agent_outbox_error, classify_cloud_agent_claim,
    cloud_agent_outbox_failure, validate_initial_cloud_agent_state, CloudAgentOutboxPublishFailure,
    CloudAgentPendingOutboxIntent, CloudAgentStateRepository,
};
pub use state_store::{CloudAgentStateStore, InMemoryCloudAgentRunStore};

#[cfg(test)]
use input_history::append_response_output_items;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CloudAgentClaim {
    pub ordering: CloudAgentOrdering,
    pub expected_status: CloudAgentRunStatus,
    pub expected_phase: CloudAgentRunPhase,
    pub expected_version: u64,
    pub claim_token: String,
    pub claim_until: DateTime<Utc>,
}

impl CloudAgentClaim {
    pub fn validate(&self) -> Result<(), String> {
        self.ordering.validate()?;
        if self.expected_version == 0 {
            return Err("expected_version must be greater than zero".to_string());
        }
        if self.claim_token.trim().is_empty() {
            return Err("claim_token must not be empty".to_string());
        }
        if self.expected_status.is_terminal() {
            return Err("terminal runs cannot be claimed".to_string());
        }
        Ok(())
    }
}

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

    #[test]
    fn tool_batch_uses_a_stable_step_identity_and_one_outbox_command() {
        let transition = reduce_single_step(
            &run(),
            claim(),
            "start-event-1",
            "cloud_agent.task_runner.mcp_results",
            AiSingleStepOutcome::ToolCommand {
                response: AiRuntimeResult {
                    content: String::new(),
                    reasoning: None,
                    tool_calls: None,
                    finish_reason: Some("tool_calls".to_string()),
                    usage: None,
                    response_id: Some("response-1".to_string()),
                    response_output_items: Vec::new(),
                    request_input_items: Vec::new(),
                },
                tool_calls: serde_json::json!([{"id": "call-1"}]),
            },
        )
        .unwrap();
        assert_eq!(
            transition.next_status,
            CloudAgentRunStatus::WaitingToolResult
        );
        assert_eq!(transition.outbox.len(), 1);
        assert_eq!(
            transition.pending_batch_id.as_deref(),
            Some("mcp_batch_run-1_1_2")
        );
    }

    #[test]
    fn mcp_results_resume_with_ordered_call_and_output_pairs() {
        let calls = serde_json::json!([
            {
                "id": "call-1",
                "function": {"name": "CodeMaintainer", "arguments": "{\"path\":\"README.md\"}"}
            },
            {
                "id": "call-2",
                "function": {"name": "Terminal", "arguments": "{\"command\":\"python -m unittest\"}"}
            }
        ]);
        let results = serde_json::json!([
            {"status": "completed", "result": {"written": true}},
            {"status": "failed", "error": "tests failed"}
        ]);

        let response_output = serde_json::json!([
            {"type":"reasoning","id":"rs-1","summary":[{"type":"summary_text","text":"inspect"}]},
            {"type":"function_call","id":"fc-1","call_id":"call-1","name":"CodeMaintainer","arguments":"{\"path\":\"README.md\"}"},
            {"type":"function_call","id":"fc-2","call_id":"call-2","name":"Terminal","arguments":"{\"command\":\"python -m unittest\"}"}
        ]);
        let items = cloud_agent_mcp_result_input_items(
            response_output.as_array().unwrap(),
            calls.as_array().unwrap(),
            results.as_array().unwrap(),
        )
        .unwrap();

        assert_eq!(items.len(), 5);
        assert_eq!(items[..3], response_output.as_array().unwrap()[..]);
        assert_eq!(items[3]["type"], "function_call_output");
        assert_eq!(items[3]["call_id"], "call-1");
        assert_eq!(items[3]["output"], "{\"written\":true}");
        assert_eq!(items[4]["type"], "function_call_output");
        assert_eq!(items[4]["call_id"], "call-2");
        assert_eq!(items[4]["output"], "tests failed");
    }

    #[test]
    fn mcp_result_callback_payload_preserves_call_identity() {
        let calls = serde_json::json!([
            {
                "id": "call-1",
                "invocation_id": "invocation-1",
                "conversation_turn_id": "turn-1",
                "function": {"name": "Terminal", "arguments": "{}"}
            }
        ]);
        let results = serde_json::json!([
            {"status": "completed", "result": {"background": true, "busy": true}}
        ]);

        let payload = cloud_agent_mcp_result_callback_payload(
            calls.as_array().unwrap(),
            results.as_array().unwrap(),
        )
        .unwrap();
        let result = &payload["tool_results"][0];

        assert_eq!(result["tool_call_id"], "call-1");
        assert_eq!(result["invocation_id"], "invocation-1");
        assert_eq!(result["conversation_turn_id"], "turn-1");
        assert_eq!(result["is_stream"], false);
        assert_eq!(result["success"], true);
    }

    #[test]
    fn a_single_mcp_result_uses_the_same_call_and_output_pair() {
        let calls = serde_json::json!([
            {"id": "call-1", "function": {"name": "TaskProcessLog", "arguments": "{}"}}
        ]);
        let results = serde_json::json!([
            {"status": "completed", "result": "recorded"}
        ]);

        let response_output = serde_json::json!([
            {"type":"function_call","id":"fc-1","call_id":"call-1","name":"TaskProcessLog","arguments":"{}"}
        ]);
        let items = cloud_agent_mcp_result_input_items(
            response_output.as_array().unwrap(),
            calls.as_array().unwrap(),
            results.as_array().unwrap(),
        )
        .unwrap();

        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["type"], "function_call");
        assert_eq!(items[0]["call_id"], "call-1");
        assert_eq!(items[1]["type"], "function_call_output");
        assert_eq!(items[1]["call_id"], "call-1");
    }

    #[test]
    fn mcp_image_result_is_forwarded_as_visual_input_instead_of_base64_text() {
        let calls = serde_json::json!([{
            "id": "call-image",
            "function": {"name": "computer_get_app_state", "arguments": "{}"}
        }]);
        let results = serde_json::json!([{
            "status": "completed",
            "result": {
                "content": [
                    {"type": "text", "text": "App=md.obsidian"},
                    {
                        "type": "image",
                        "data": "iVBORw0KGgo=",
                        "mimeType": "image/png"
                    }
                ],
                "isError": false
            }
        }]);
        let response_output = serde_json::json!([{
            "type": "function_call",
            "call_id": "call-image",
            "name": "computer_get_app_state",
            "arguments": "{}"
        }]);

        let items = cloud_agent_mcp_result_input_items(
            response_output.as_array().unwrap(),
            calls.as_array().unwrap(),
            results.as_array().unwrap(),
        )
        .unwrap();

        assert_eq!(items.len(), 3);
        assert_eq!(items[1]["output"], "App=md.obsidian");
        assert!(!items[1].to_string().contains("iVBORw0KGgo="));
        assert_eq!(
            items[2]
                .pointer("/content/0/image_url")
                .and_then(Value::as_str),
            Some("data:image/png;base64,iVBORw0KGgo=")
        );

        let callback = cloud_agent_mcp_result_callback_payload(
            calls.as_array().unwrap(),
            results.as_array().unwrap(),
        )
        .unwrap();
        assert_eq!(callback["tool_results"][0]["content"], "App=md.obsidian");
        assert!(!callback.to_string().contains("iVBORw0KGgo="));
    }

    #[test]
    fn multiple_tool_batches_append_without_rewriting_the_previous_prefix() {
        let first_history = serde_json::json!([
            {"role":"user","content":"implement"},
            {"type":"reasoning","id":"rs-1","summary":[]},
            {"type":"function_call","id":"fc-1","call_id":"call-1","name":"read","arguments":"{}"}
        ]);
        let first_calls = serde_json::json!([
            {"id":"call-1","function":{"name":"read","arguments":"{}"}}
        ]);
        let first_results = serde_json::json!([
            {"status":"completed","result":"contents"}
        ]);
        let batch_one = cloud_agent_mcp_result_input_items(
            first_history.as_array().unwrap(),
            first_calls.as_array().unwrap(),
            first_results.as_array().unwrap(),
        )
        .unwrap();
        let second_output = serde_json::json!([
            {"type":"reasoning","id":"rs-2","summary":[]},
            {"type":"function_call","id":"fc-2","call_id":"call-2","name":"write","arguments":"{}"}
        ]);
        let second_history = append_response_output_items(
            batch_one.as_slice(),
            second_output.as_array().unwrap(),
            None,
        );
        let second_calls = serde_json::json!([
            {"id":"call-2","function":{"name":"write","arguments":"{}"}}
        ]);
        let second_results = serde_json::json!([
            {"status":"completed","result":{"written":true}}
        ]);
        let batch_two = cloud_agent_mcp_result_input_items(
            second_history.as_slice(),
            second_calls.as_array().unwrap(),
            second_results.as_array().unwrap(),
        )
        .unwrap();

        assert_eq!(&batch_two[..batch_one.len()], batch_one.as_slice());
        assert_eq!(batch_two.last().unwrap()["call_id"], "call-2");
    }

    #[test]
    fn consumed_tool_images_are_not_carried_into_later_model_steps() {
        let request = serde_json::json!([
            {"type":"message","role":"user","content":[{"type":"input_text","text":"design the page"}]},
            {"type":"function_call","call_id":"call-image","name":"capture","arguments":"{}"},
            {"type":"function_call_output","call_id":"call-image","output":"captured"},
            {"type":"message","role":"user","content":[
                {"type":"input_image","image_url":"data:image/png;base64,large-candidate"},
                {"type":"image_url","image_url":"data:image/png;base64,large-diff"}
            ]}
        ]);
        let response = serde_json::json!([
            {"type":"reasoning","id":"rs-after-review","summary":[]},
            {"type":"function_call","call_id":"call-accept","name":"accept","arguments":"{}"}
        ]);

        let next = append_response_output_items(
            request.as_array().unwrap(),
            response.as_array().unwrap(),
            None,
        );
        let serialized = serde_json::to_string(&next).unwrap();

        assert!(serialized.contains("design the page"));
        assert!(serialized.contains("call-accept"));
        assert!(!serialized.contains("large-candidate"));
        assert!(!serialized.contains("large-diff"));
    }

    #[test]
    fn original_user_images_are_preserved() {
        let request = serde_json::json!([
            {"type":"message","role":"user","content":[
                {"type":"input_image","image_url":"data:image/png;base64,user-reference"}
            ]},
            {"type":"message","role":"assistant","content":[{"type":"output_text","text":"I will inspect it."}]},
            {"type":"function_call","call_id":"call-read","name":"read","arguments":"{}"},
            {"type":"function_call_output","call_id":"call-read","output":"done"},
            {"type":"message","role":"user","content":[
                {"type":"input_image","image_url":"data:image/png;base64,tool-capture"}
            ]}
        ]);

        let next = append_response_output_items(request.as_array().unwrap(), &[], None);
        let serialized = serde_json::to_string(&next).unwrap();

        assert!(serialized.contains("user-reference"));
        assert!(!serialized.contains("tool-capture"));
    }

    #[test]
    fn latest_compaction_item_replaces_the_older_stateless_prefix() {
        let previous = serde_json::json!([
            {"role":"user","content":"old task"},
            {"type":"reasoning","id":"rs-old","summary":[]}
        ]);
        let output = serde_json::json!([
            {"type":"compaction","id":"cmp-1","encrypted_content":"opaque"},
            {"type":"message","role":"assistant","content":[]}
        ]);

        let next = append_response_output_items(
            previous.as_array().unwrap(),
            output.as_array().unwrap(),
            None,
        );

        assert_eq!(next.len(), 2);
        assert_eq!(next[0]["type"], "compaction");
        assert_eq!(next[1]["type"], "message");
        assert!(!serde_json::to_string(&next)
            .expect("serialize compacted history")
            .contains("old task"));
    }

    #[test]
    fn retry_does_not_advance_the_model_step() {
        let transition = reduce_single_step(
            &run(),
            claim(),
            "start-event-1",
            "cloud_agent.task_runner.retries",
            AiSingleStepOutcome::Retry {
                error: "timeout".to_string(),
                retry_kind: "network".to_string(),
                next_model_attempt: 2,
                backoff_ms: 500,
            },
        )
        .unwrap();
        assert_eq!(transition.next_step_seq, 2);
        assert_eq!(transition.next_retry_count, 1);
        assert_eq!(transition.outbox.len(), 1);
        assert_eq!(
            transition.outbox[0].event_id,
            "ai_runtime_retry_run-1_1_2_attempt_2"
        );
    }

    #[derive(Clone)]
    struct TestSingleStepExecutor {
        outcome: AiSingleStepOutcome,
        seen_triggers: Arc<Mutex<Vec<CloudAgentModelTrigger>>>,
    }

    #[async_trait]
    impl CloudAgentSingleStepExecutor for TestSingleStepExecutor {
        async fn execute_single_step(
            &self,
            _run: &CloudAgentRunRecord,
            trigger: &CloudAgentModelTrigger,
        ) -> Result<CloudAgentSingleStepExecution, String> {
            self.seen_triggers.lock().unwrap().push(trigger.clone());
            Ok(CloudAgentSingleStepExecution::Apply(
                CloudAgentSingleStepOutput::new(self.outcome.clone())
                    .with_mcp_runtime("session-1", "mcp.commands")
                    .with_retry_input_items(vec![serde_json::json!({"type": "message"})]),
            ))
        }
    }

    async fn inserted_ready_run() -> InMemoryCloudAgentRunStore {
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

    fn consume_input() -> CloudAgentConsumeInput {
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

    fn tool_outcome(call_count: usize) -> AiSingleStepOutcome {
        AiSingleStepOutcome::ToolCommand {
            response: AiRuntimeResult {
                content: String::new(),
                reasoning: None,
                tool_calls: None,
                finish_reason: Some("tool_calls".to_string()),
                usage: None,
                response_id: Some("response-1".to_string()),
                response_output_items: Vec::new(),
                request_input_items: Vec::new(),
            },
            tool_calls: Value::Array(
                (0..call_count)
                    .map(|index| {
                        serde_json::json!({
                            "id": format!("call-{index}"),
                            "function": {
                                "name": format!("tool-{index}"),
                                "arguments": "{}",
                            },
                        })
                    })
                    .collect(),
            ),
        }
    }

    #[tokio::test]
    async fn single_and_multiple_tools_use_the_same_single_step_transaction() {
        for call_count in [1, 3] {
            let store = inserted_ready_run().await;
            let executor = TestSingleStepExecutor {
                outcome: tool_outcome(call_count),
                seen_triggers: Arc::new(Mutex::new(Vec::new())),
            };

            assert_eq!(
                consume_cloud_agent_single_step(&store, &executor, consume_input())
                    .await
                    .unwrap(),
                CloudAgentConsumeDisposition::Committed
            );
            let persisted = store.load_run("run-1").await.unwrap().unwrap();
            assert_eq!(persisted.status, CloudAgentRunStatus::WaitingToolResult);
            assert_eq!(persisted.pending_tool_calls.len(), call_count);
            let outbox = store.list_ready_outbox(10).await.unwrap();
            assert_eq!(outbox.len(), 1);
            assert_eq!(outbox[0].topic, "mcp_tool_call_command");
            assert_eq!(outbox[0].routing_key, "mcp.commands");
            assert_eq!(
                outbox[0].payload["calls"].as_array().unwrap().len(),
                call_count
            );
        }
    }

    #[tokio::test]
    async fn retry_keeps_exact_input_items_in_the_durable_event() {
        let store = inserted_ready_run().await;
        let executor = TestSingleStepExecutor {
            outcome: AiSingleStepOutcome::Retry {
                error: "timeout".to_string(),
                retry_kind: "network".to_string(),
                next_model_attempt: 2,
                backoff_ms: 0,
            },
            seen_triggers: Arc::new(Mutex::new(Vec::new())),
        };

        consume_cloud_agent_single_step(&store, &executor, consume_input())
            .await
            .unwrap();
        let outbox = store.list_ready_outbox(10).await.unwrap();
        assert_eq!(outbox.len(), 1);
        assert_eq!(outbox[0].topic, "ai_runtime_retry");
        assert_eq!(
            outbox[0].payload["input_items"],
            serde_json::json!([{"type": "message"}])
        );
        assert!(outbox[0].payload.get("disable_stream").is_none());
        assert!(outbox[0].payload.get("downgrade_thinking_to").is_none());
    }

    #[tokio::test]
    async fn tool_result_transport_retry_keeps_outputs_and_never_republishes_the_tool_batch() {
        let store = InMemoryCloudAgentRunStore::new();
        store.allocate_lane_seq("task:task-1").await.unwrap();
        let mut record = run();
        record.status = CloudAgentRunStatus::WaitingToolResult;
        record.phase = CloudAgentRunPhase::ToolBatch;
        record.ordering.step_seq = 2;
        record.iteration = 1;
        record.version = 1;
        record.pending_batch_id = Some("mcp_batch_run-1_1_1".to_string());
        record.pending_tool_calls = vec![serde_json::json!({
            "id": "call-write-1",
            "function": {
                "name": "code_maintainer_write_stage_edit_batch",
                "arguments": "{\"path\":\"src/App.tsx\"}"
            }
        })];
        record.response_input_items = vec![serde_json::json!({
            "type": "function_call",
            "call_id": "call-write-1",
            "name": "code_maintainer_write_stage_edit_batch",
            "arguments": "{\"path\":\"src/App.tsx\"}"
        })];
        store.insert_run(record).await.unwrap();

        let durable_retry_items = vec![
            serde_json::json!({
                "type": "function_call",
                "call_id": "call-write-1",
                "name": "code_maintainer_write_stage_edit_batch",
                "arguments": "{\"path\":\"src/App.tsx\"}"
            }),
            serde_json::json!({
                "type": "function_call_output",
                "call_id": "call-write-1",
                "output": "{\"written\":true}"
            }),
        ];
        let seen_triggers = Arc::new(Mutex::new(Vec::new()));
        let retry = TestSingleStepExecutor {
            outcome: AiSingleStepOutcome::Retry {
                error: "stream response body failed: unexpected eof".to_string(),
                retry_kind: "network".to_string(),
                next_model_attempt: 2,
                backoff_ms: 0,
            },
            seen_triggers: Arc::clone(&seen_triggers),
        };
        let retry = struct_with_retry_items(retry, durable_retry_items.clone());
        let tool_result_event_id = "mcp-result-batch-1";
        let input = CloudAgentConsumeInput {
            agent_run_id: "run-1".to_string(),
            event_id: tool_result_event_id.to_string(),
            trigger: CloudAgentModelTrigger::ToolResults {
                event_id: tool_result_event_id.to_string(),
                batch_id: "mcp_batch_run-1_1_1".to_string(),
                source_step_seq: 1,
                items: vec![serde_json::json!({
                    "status": "completed",
                    "result": {"written": true}
                })],
            },
            expected_status: CloudAgentRunStatus::WaitingToolResult,
            expected_phase: CloudAgentRunPhase::ToolBatch,
            claim_token: "claim-after-tools".to_string(),
            claim_until: Utc::now() + chrono::Duration::seconds(30),
            output_routing_key: "cloud_agent.task_runner.runtime".to_string(),
        };

        assert_eq!(
            consume_cloud_agent_single_step(&store, &retry, input)
                .await
                .unwrap(),
            CloudAgentConsumeDisposition::Committed
        );
        let persisted = store.load_run("run-1").await.unwrap().unwrap();
        assert_eq!(persisted.status, CloudAgentRunStatus::RetryScheduled);
        assert_eq!(persisted.response_input_items, durable_retry_items);
        assert_eq!(persisted.pending_tool_calls.len(), 1);
        let outbox = store.list_ready_outbox(10).await.unwrap();
        assert_eq!(outbox.len(), 1);
        assert_eq!(outbox[0].topic, "ai_runtime_retry");
        assert_eq!(
            outbox[0].payload["input_items"][1]["type"],
            "function_call_output"
        );
        assert!(!outbox
            .iter()
            .any(|intent| intent.topic == "mcp_tool_call_command"));

        let retry_event_id = outbox[0].event_id.clone();
        assert!(store
            .mark_outbox_published(retry_event_id.as_str())
            .await
            .unwrap());
        let final_executor = TestSingleStepExecutor {
            outcome: AiSingleStepOutcome::Final(AiRuntimeResult {
                content: "done without repeating the write".to_string(),
                reasoning: None,
                tool_calls: None,
                finish_reason: Some("stop".to_string()),
                usage: None,
                response_id: Some("response-after-retry".to_string()),
                response_output_items: Vec::new(),
                request_input_items: durable_retry_items,
            }),
            seen_triggers: Arc::clone(&seen_triggers),
        };
        let retry_input = CloudAgentConsumeInput {
            agent_run_id: "run-1".to_string(),
            event_id: retry_event_id.clone(),
            trigger: CloudAgentModelTrigger::Retry {
                event_id: retry_event_id,
                model_attempt: 2,
                payload: Value::Null,
            },
            expected_status: CloudAgentRunStatus::RetryScheduled,
            expected_phase: CloudAgentRunPhase::RetryDelay,
            claim_token: "claim-retry-final".to_string(),
            claim_until: Utc::now() + chrono::Duration::seconds(30),
            output_routing_key: "cloud_agent.task_runner.runtime".to_string(),
        };
        assert_eq!(
            consume_cloud_agent_single_step(&store, &final_executor, retry_input)
                .await
                .unwrap(),
            CloudAgentConsumeDisposition::Committed
        );
        assert_eq!(seen_triggers.lock().unwrap().len(), 2);
        assert_eq!(
            store.load_run("run-1").await.unwrap().unwrap().status,
            CloudAgentRunStatus::Succeeded
        );
        assert!(store
            .list_ready_outbox(10)
            .await
            .unwrap()
            .iter()
            .all(|intent| intent.topic != "mcp_tool_call_command"));
    }

    fn struct_with_retry_items(
        executor: TestSingleStepExecutor,
        retry_input_items: Vec<Value>,
    ) -> impl CloudAgentSingleStepExecutor {
        #[derive(Clone)]
        struct RetryInputExecutor {
            executor: TestSingleStepExecutor,
            retry_input_items: Vec<Value>,
        }

        #[async_trait]
        impl CloudAgentSingleStepExecutor for RetryInputExecutor {
            async fn execute_single_step(
                &self,
                run: &CloudAgentRunRecord,
                trigger: &CloudAgentModelTrigger,
            ) -> Result<CloudAgentSingleStepExecution, String> {
                let CloudAgentSingleStepExecution::Apply(output) =
                    self.executor.execute_single_step(run, trigger).await?
                else {
                    unreachable!();
                };
                Ok(CloudAgentSingleStepExecution::Apply(
                    output.with_retry_input_items(self.retry_input_items.clone()),
                ))
            }
        }

        RetryInputExecutor {
            executor,
            retry_input_items,
        }
    }

    #[tokio::test]
    async fn consecutive_retries_publish_distinct_events_and_can_finish_terminally() {
        let store = inserted_ready_run().await;
        let seen_triggers = Arc::new(Mutex::new(Vec::new()));
        let first_retry = TestSingleStepExecutor {
            outcome: AiSingleStepOutcome::Retry {
                error: "connect failed".to_string(),
                retry_kind: "network".to_string(),
                next_model_attempt: 2,
                backoff_ms: 0,
            },
            seen_triggers: Arc::clone(&seen_triggers),
        };
        consume_cloud_agent_single_step(&store, &first_retry, consume_input())
            .await
            .unwrap();
        let first_outbox = store.list_ready_outbox(10).await.unwrap();
        assert_eq!(first_outbox.len(), 1);
        let first_event_id = first_outbox[0].event_id.clone();
        assert!(store
            .mark_outbox_published(first_event_id.as_str())
            .await
            .unwrap());

        let second_retry = TestSingleStepExecutor {
            outcome: AiSingleStepOutcome::Retry {
                error: "dns failed".to_string(),
                retry_kind: "network".to_string(),
                next_model_attempt: 3,
                backoff_ms: 0,
            },
            seen_triggers: Arc::clone(&seen_triggers),
        };
        let second_input = CloudAgentConsumeInput {
            agent_run_id: "run-1".to_string(),
            event_id: first_event_id.clone(),
            trigger: CloudAgentModelTrigger::Retry {
                event_id: first_event_id,
                model_attempt: 2,
                payload: Value::Null,
            },
            expected_status: CloudAgentRunStatus::RetryScheduled,
            expected_phase: CloudAgentRunPhase::RetryDelay,
            claim_token: "claim-second-retry".to_string(),
            claim_until: Utc::now() + chrono::Duration::seconds(30),
            output_routing_key: "cloud_agent.task_runner.runtime".to_string(),
        };
        consume_cloud_agent_single_step(&store, &second_retry, second_input)
            .await
            .unwrap();
        let second_outbox = store.list_ready_outbox(10).await.unwrap();
        assert_eq!(second_outbox.len(), 1);
        assert_ne!(second_outbox[0].event_id, first_outbox[0].event_id);
        assert_eq!(second_outbox[0].payload["model_attempt"], 3);
        let second_event_id = second_outbox[0].event_id.clone();
        assert!(store
            .mark_outbox_published(second_event_id.as_str())
            .await
            .unwrap());

        let exhausted = TestSingleStepExecutor {
            outcome: AiSingleStepOutcome::Failed {
                error: "network retry exhausted".to_string(),
            },
            seen_triggers,
        };
        let exhausted_input = CloudAgentConsumeInput {
            agent_run_id: "run-1".to_string(),
            event_id: second_event_id.clone(),
            trigger: CloudAgentModelTrigger::Retry {
                event_id: second_event_id,
                model_attempt: 3,
                payload: Value::Null,
            },
            expected_status: CloudAgentRunStatus::RetryScheduled,
            expected_phase: CloudAgentRunPhase::RetryDelay,
            claim_token: "claim-exhausted-retry".to_string(),
            claim_until: Utc::now() + chrono::Duration::seconds(30),
            output_routing_key: "cloud_agent.task_runner.runtime".to_string(),
        };
        consume_cloud_agent_single_step(&store, &exhausted, exhausted_input)
            .await
            .unwrap();

        let persisted = store.load_run("run-1").await.unwrap().unwrap();
        assert_eq!(persisted.status, CloudAgentRunStatus::Failed);
        assert_eq!(persisted.phase, CloudAgentRunPhase::Terminal);
        assert_eq!(
            persisted.terminal_outcome.unwrap()["error"],
            "network retry exhausted"
        );
    }

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
                Ok(CloudAgentSingleStepExecution::Apply(
                    output.with_next_input(serde_json::json!({"lifecycle_round": 2})),
                ))
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
            Ok(CloudAgentSingleStepExecution::Apply(
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
            ))
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
            Ok(CloudAgentSingleStepExecution::Apply(
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
            ))
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
        let registry =
            CloudAgentProfileRegistry::new("task-runner", CloudAgentStateStore::memory())
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
        let registry =
            CloudAgentProfileRegistry::new("task-runner", CloudAgentStateStore::memory())
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
}
