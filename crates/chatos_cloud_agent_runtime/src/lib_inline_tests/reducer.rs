// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

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
        {"id": "call-1", "function": {"name": "ExampleMcpTool", "arguments": "{}"}}
    ]);
    let results = serde_json::json!([
        {"status": "completed", "result": "recorded"}
    ]);

    let response_output = serde_json::json!([
        {"type":"function_call","id":"fc-1","call_id":"call-1","name":"ExampleMcpTool","arguments":"{}"}
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
