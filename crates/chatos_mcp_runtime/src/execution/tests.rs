// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use serde_json::json;

use super::{execute_tool_calls_parallel, execute_tool_calls_stream};
use crate::{ToolCallContext, ToolResultCallback};

#[tokio::test]
async fn sequential_execution_emits_stream_and_final_results() {
    let callback_count = Arc::new(AtomicUsize::new(0));
    let callback: ToolResultCallback = {
        let callback_count = Arc::clone(&callback_count);
        Arc::new(move |_| {
            callback_count.fetch_add(1, Ordering::SeqCst);
        })
    };
    let tool_calls = vec![json!({
        "id": "call-1",
        "function": {"name": "search", "arguments": "{\"query\":\"rust\"}"}
    })];

    let results = execute_tool_calls_stream(
        tool_calls.as_slice(),
        ToolCallContext::new(None, Some("turn-1".to_string()), None),
        Some(callback),
        |_name, args, stream| async move {
            assert_eq!(args, json!({"query": "rust"}));
            stream.expect("stream callback")("chunk".to_string());
            Ok(("done".to_string(), Some(json!({"count": 1}))))
        },
    )
    .await;

    assert_eq!(callback_count.load(Ordering::SeqCst), 2);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].conversation_turn_id.as_deref(), Some("turn-1"));
    assert_eq!(results[0].result, Some(json!({"count": 1})));
}

#[tokio::test]
async fn aborted_context_skips_execution_and_callbacks() {
    let results = execute_tool_calls_stream(
        &[json!({"id": "call-1", "function": {"name": "search", "arguments": "{}"}})],
        ToolCallContext::new(Some("session-1".to_string()), None, None)
            .with_abort_checker(Arc::new(|_| true)),
        None,
        |_name, _args, _stream| async move { Ok(("unexpected".to_string(), None)) },
    )
    .await;

    assert!(results.is_empty());
}

#[tokio::test]
async fn parallel_execution_maps_panics_to_the_originating_call() {
    let tool_calls = vec![
        json!({"id": "call-ok", "function": {"name": "read_file", "arguments": "{}"}}),
        json!({"id": "call-panic", "function": {"name": "search_text", "arguments": "{}"}}),
    ];

    let results = execute_tool_calls_parallel(
        tool_calls.as_slice(),
        ToolCallContext::default(),
        None,
        |name, _args, _context, _stream| async move {
            if name == "search_text" {
                panic!("simulated tool panic");
            }
            Ok((format!("ok:{name}"), None))
        },
    )
    .await;

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].tool_call_id, "call-ok");
    assert!(results[0].success);
    assert_eq!(results[1].tool_call_id, "call-panic");
    assert!(results[1].is_error);
    assert!(results[1].content.contains("internal panic"));
}
