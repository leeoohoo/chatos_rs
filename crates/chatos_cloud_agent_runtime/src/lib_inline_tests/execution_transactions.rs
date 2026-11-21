// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::fixtures::*;
use super::*;

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
            Ok(CloudAgentSingleStepExecution::Apply(Box::new(
                (*output).with_retry_input_items(self.retry_input_items.clone()),
            )))
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
