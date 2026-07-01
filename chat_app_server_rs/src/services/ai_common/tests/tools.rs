// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[test]
fn build_tool_result_metadata_keeps_tool_flags() {
    let result = ToolResult {
        tool_call_id: "call_1".to_string(),
        name: "mcp.query".to_string(),
        success: true,
        is_error: false,
        is_stream: false,
        conversation_turn_id: Some("turn_abc".to_string()),
        content: "ok".to_string(),
        result: Some(serde_json::json!({"answer": 42})),
    };

    let metadata = build_tool_result_metadata(&result);

    assert_eq!(
        metadata.get("toolName").and_then(|value| value.as_str()),
        Some("mcp.query")
    );
    assert_eq!(
        metadata.get("success").and_then(|value| value.as_bool()),
        Some(true)
    );
    assert_eq!(
        metadata.get("isError").and_then(|value| value.as_bool()),
        Some(false)
    );
    assert_eq!(
        metadata
            .get("conversation_turn_id")
            .and_then(|value| value.as_str()),
        Some("turn_abc")
    );
    assert_eq!(
        metadata
            .get("structured_result")
            .and_then(|value| value.get("answer"))
            .and_then(|value| value.as_i64()),
        Some(42)
    );
}

#[test]
fn build_aborted_tool_results_only_adds_missing_calls() {
    let tool_calls = vec![
        serde_json::json!({"id": "call_existing", "function": {"name": "tool.a"}}),
        serde_json::json!({"id": "call_missing", "function": {"name": "tool.b"}}),
        serde_json::json!({"id": "", "function": {"name": "tool.c"}}),
    ];

    let existing = vec![ToolResult {
        tool_call_id: "call_existing".to_string(),
        name: "tool.a".to_string(),
        success: true,
        is_error: false,
        is_stream: false,
        conversation_turn_id: None,
        content: "done".to_string(),
        result: None,
    }];

    let merged = build_aborted_tool_results(&tool_calls, Some(existing.as_slice()));

    assert_eq!(merged.len(), 2);
    assert!(merged
        .iter()
        .any(|item| item.tool_call_id == "call_existing" && item.success));
    assert!(merged
        .iter()
        .any(|item| item.tool_call_id == "call_missing" && !item.success && item.is_error));
}

#[test]
fn aborted_tool_results_if_needed_returns_none_when_not_aborted() {
    let session_id = "ai_common_aborted_if_needed_none";
    abort_registry::clear(session_id);
    let tool_calls = vec![serde_json::json!({
        "id": "call_1",
        "function": {"name": "tool.a"}
    })];

    let result = aborted_tool_results_if_needed(Some(session_id), true, &tool_calls, None);
    assert!(result.is_none());
}

#[test]
fn aborted_tool_results_if_needed_builds_results_when_aborted() {
    let session_id = "ai_common_aborted_if_needed_yes";
    abort_registry::clear(session_id);
    abort_registry::abort(session_id);

    let tool_calls = vec![serde_json::json!({
        "id": "call_1",
        "function": {"name": "tool.a"}
    })];

    let result =
        aborted_tool_results_if_needed(Some(session_id), true, &tool_calls, None).expect("results");
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].tool_call_id, "call_1");
    assert!(result[0].is_error);

    abort_registry::clear(session_id);
}

#[test]
fn build_tools_end_payload_serializes_tool_results() {
    let payload = build_tools_end_payload(&[ToolResult {
        tool_call_id: "call_1".to_string(),
        name: "tool.a".to_string(),
        success: true,
        is_error: false,
        is_stream: false,
        conversation_turn_id: Some("turn_1".to_string()),
        content: "ok".to_string(),
        result: None,
    }]);

    assert_eq!(
        payload
            .get("tool_results")
            .and_then(Value::as_array)
            .map(|items| items.len()),
        Some(1)
    );
    assert_eq!(
        payload
            .get("tool_results")
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .and_then(|item| item.get("tool_call_id"))
            .and_then(Value::as_str),
        Some("call_1")
    );
}

#[tokio::test]
async fn execute_tool_lifecycle_runs_callbacks_and_persists_results() {
    let started = Arc::new(std::sync::Mutex::new(Vec::<Value>::new()));
    let ended = Arc::new(std::sync::Mutex::new(Vec::<Value>::new()));
    let persisted = Arc::new(std::sync::Mutex::new(Vec::<Vec<ToolResult>>::new()));

    let callbacks = AiClientCallbacks {
        on_tools_start: Some({
            let started = started.clone();
            Arc::new(move |value: Value| {
                started.lock().expect("lock poisoned").push(value);
            })
        }),
        on_tools_end: Some({
            let ended = ended.clone();
            Arc::new(move |value: Value| {
                ended.lock().expect("lock poisoned").push(value);
            })
        }),
        ..Default::default()
    };

    let tool_calls = vec![json!({
        "id": "call_1",
        "function": {"name": "tool.a"}
    })];
    let raw_result = ToolResult {
        tool_call_id: "call_1".to_string(),
        name: "tool.a".to_string(),
        success: true,
        is_error: false,
        is_stream: false,
        conversation_turn_id: Some("turn_1".to_string()),
        content: "ok".to_string(),
        result: None,
    };

    let outcome = execute_tool_lifecycle(
        tool_calls.as_slice(),
        Value::Array(tool_calls.clone()),
        Some("session_ok"),
        true,
        &callbacks,
        move |_| {
            let raw_result = raw_result.clone();
            async move { vec![raw_result] }
        },
        |results| results.to_vec(),
        {
            let persisted = persisted.clone();
            move |results| {
                let persisted = persisted.clone();
                async move {
                    persisted.lock().expect("lock poisoned").push(results);
                }
            }
        },
    )
    .await
    .expect("tool execution should succeed");

    assert_eq!(outcome.persisted_results.len(), 1);
    assert_eq!(started.lock().expect("lock poisoned").len(), 1);
    assert_eq!(ended.lock().expect("lock poisoned").len(), 1);
    assert_eq!(persisted.lock().expect("lock poisoned").len(), 1);
}

#[tokio::test]
async fn execute_tool_lifecycle_persists_aborted_results_before_execution() {
    let session_id = "ai_common_tool_lifecycle_abort";
    abort_registry::clear(session_id);
    abort_registry::abort(session_id);

    let persisted = Arc::new(std::sync::Mutex::new(Vec::<Vec<ToolResult>>::new()));
    let callbacks = AiClientCallbacks::default();
    let tool_calls = vec![json!({
        "id": "call_1",
        "function": {"name": "tool.a"}
    })];

    let result = execute_tool_lifecycle(
        tool_calls.as_slice(),
        Value::Array(tool_calls.clone()),
        Some(session_id),
        true,
        &callbacks,
        |_| async move { Vec::new() },
        |results| results.to_vec(),
        {
            let persisted = persisted.clone();
            move |results| {
                let persisted = persisted.clone();
                async move {
                    persisted.lock().expect("lock poisoned").push(results);
                }
            }
        },
    )
    .await;

    assert!(matches!(result, Err(err) if err == "aborted"));
    assert_eq!(persisted.lock().expect("lock poisoned").len(), 1);
    assert_eq!(
        persisted.lock().expect("lock poisoned")[0][0].tool_call_id,
        "call_1"
    );

    abort_registry::clear(session_id);
}

#[test]
fn build_tool_stream_callback_emits_result_when_not_aborted() {
    let session_id = "ai_common_tool_stream_emit";
    abort_registry::clear(session_id);

    let captured = Arc::new(std::sync::Mutex::new(Vec::<Value>::new()));
    let on_stream = {
        let captured = captured.clone();
        Arc::new(move |value: Value| {
            captured.lock().expect("lock poisoned").push(value);
        }) as Arc<dyn Fn(Value) + Send + Sync>
    };

    let callback = build_tool_stream_callback(Some(on_stream), Some(session_id.to_string()))
        .expect("callback should be built");

    callback(&ToolResult {
        tool_call_id: "call_1".to_string(),
        name: "mcp.search".to_string(),
        success: true,
        is_error: false,
        is_stream: false,
        conversation_turn_id: None,
        content: "ok".to_string(),
        result: None,
    });

    let events = captured.lock().expect("lock poisoned");
    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0]
            .get("tool_call_id")
            .and_then(|value| value.as_str()),
        Some("call_1")
    );

    abort_registry::clear(session_id);
}

#[test]
fn build_tool_stream_callback_skips_result_when_aborted() {
    let session_id = "ai_common_tool_stream_aborted";
    abort_registry::clear(session_id);
    abort_registry::abort(session_id);

    let captured = Arc::new(std::sync::Mutex::new(Vec::<Value>::new()));
    let on_stream = {
        let captured = captured.clone();
        Arc::new(move |value: Value| {
            captured.lock().expect("lock poisoned").push(value);
        }) as Arc<dyn Fn(Value) + Send + Sync>
    };

    let callback = build_tool_stream_callback(Some(on_stream), Some(session_id.to_string()))
        .expect("callback should be built");

    callback(&ToolResult {
        tool_call_id: "call_2".to_string(),
        name: "mcp.read".to_string(),
        success: true,
        is_error: false,
        is_stream: false,
        conversation_turn_id: None,
        content: "ok".to_string(),
        result: None,
    });

    assert!(captured.lock().expect("lock poisoned").is_empty());

    abort_registry::clear(session_id);
}
