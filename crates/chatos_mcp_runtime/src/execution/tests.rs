// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use serde_json::json;

use super::{execute_tool_calls_parallel, execute_tool_calls_stream};
use crate::{ToolCallContext, ToolResultCallback};

#[test]
fn failed_external_result_strips_binary_and_trace_but_keeps_diagnosis() {
    let result = super::external_tool_failure_from_value(
        "call-1".into(),
        "external".into(),
        None,
        Some(
            &json!({"code": "missing_input", "message": "supply query", "trace": "private trace",
            "nested": {"stackTrace": "internal frames", "data": "secret binary"},
            "_model_input": [{"image_url": "data:image/png;base64,AAA"}]}),
        ),
        "native_tool_reported_error",
        Some(200),
    );
    assert!(result.content.contains("supply query"));
    assert!(result.content.contains("missing_input"));
    for excluded in [
        "private trace",
        "internal frames",
        "secret binary",
        "base64",
    ] {
        assert!(!result.content.contains(excluded));
    }
    assert!(result.transient_model_input.is_none());
    assert!(!result.fatal_error);
    assert!(result.content.chars().count() <= 200);
}

#[test]
fn provider_error_flag_is_not_overridden_by_success_status_or_false_alias() {
    let result = super::external_tool_result_from_value(
        "call".into(),
        "test".into(),
        None,
        &json!({"isError": false, "is_error": true, "content": "rejected"}),
        None,
    );
    assert!(result.is_error && !result.success);
    assert_eq!(result.content, "rejected");
    let success = super::external_tool_result_from_value(
        "call".into(),
        "test".into(),
        None,
        &json!({"is_error": false, "text": "activated"}),
        None,
    );
    assert!(success.success && !success.is_error);
    assert_eq!(success.content, "activated");
}

#[test]
fn durable_reference_content_preserves_text_and_structured_result() {
    let result = super::external_tool_result_from_value(
        "call".into(),
        "browser".into(),
        None,
        &json!({
            "content": {
                "content": [{"type": "text", "text": "result summary"}],
                "structuredContent": {"steps": [{"status": "failed"}]}
            }
        }),
        None,
    );

    let reference: serde_json::Value =
        serde_json::from_str(&super::external_tool_result_reference_content(&result))
            .expect("reference json");

    assert_eq!(reference["content"], "result summary");
    assert_eq!(
        reference["structured_result"]["steps"][0]["status"],
        "failed"
    );
}

#[test]
fn deduplicated_mcp_text_restores_structured_content_for_the_model() {
    let structured = json!({
        "workspace": {"id": "solution-1", "revision": 3},
        "requirements": [{"id": "requirement-1", "title": "完整需求"}]
    });
    let result = super::external_tool_result_from_value(
        "call".into(),
        "solution_get_workspace".into(),
        None,
        &json!({
            "content": {
                "_meta": {"chatos/deduplicatedStructuredTextCopies": 1},
                "content": [{
                    "type": "text",
                    "text": "[Duplicate JSON text omitted; use structuredContent as the canonical result.]"
                }],
                "structuredContent": structured
            }
        }),
        None,
    );

    let model_payload: serde_json::Value = serde_json::from_str(&result.content).unwrap();
    assert_eq!(model_payload["structuredContent"], structured);
    assert_eq!(
        model_payload["_meta"]["chatos/deduplicatedStructuredTextCopies"],
        1
    );
    assert_eq!(result.result, Some(structured));
    assert!(!result.content.contains("Duplicate JSON text omitted"));
}

#[test]
fn deduplicated_mcp_text_preserves_distinct_supplemental_text() {
    let result = super::external_tool_result_from_value(
        "call".into(),
        "solution_get_workspace".into(),
        None,
        &json!({
            "content": {
                "_meta": {"chatos/deduplicatedStructuredTextCopies": 1},
                "content": [
                    {"type": "text", "text": "important plugin warning"},
                    {"type": "text", "text": "[Duplicate JSON text omitted; use structuredContent as the canonical result.]"}
                ],
                "structuredContent": {"workspace": {"id": "solution-1"}}
            }
        }),
        None,
    );

    let model_payload: serde_json::Value = serde_json::from_str(&result.content).unwrap();
    assert_eq!(
        model_payload["content"][0]["text"],
        "important plugin warning"
    );
    assert_eq!(
        model_payload["structuredContent"]["workspace"]["id"],
        "solution-1"
    );
    assert!(!result.content.contains("Duplicate JSON text omitted"));
}

#[test]
fn durable_reference_does_not_duplicate_a_canonical_deduplicated_payload() {
    let sentinel = "unique structured sentinel";
    let result = super::external_tool_result_from_value(
        "call".into(),
        "solution_get_workspace".into(),
        None,
        &json!({
            "content": {
                "_meta": {"chatos/deduplicatedStructuredTextCopies": 1},
                "content": [{
                    "type": "text",
                    "text": "[Duplicate JSON text omitted; use structuredContent as the canonical result.]"
                }],
                "structuredContent": {"value": sentinel}
            }
        }),
        None,
    );

    let reference = super::external_tool_result_reference_content(&result);
    assert_eq!(reference.matches(sentinel).count(), 1);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&reference).unwrap()["structuredContent"]
            ["value"],
        sentinel
    );
}

#[test]
fn failed_deduplicated_mcp_result_keeps_the_structured_diagnosis() {
    let result = super::external_tool_failure_from_value(
        "call".into(),
        "solution_get_workspace".into(),
        None,
        Some(&json!({
            "content": {
                "_meta": {"chatos/deduplicatedStructuredTextCopies": 1},
                "content": [{
                    "type": "text",
                    "text": "[Duplicate JSON text omitted; use structuredContent as the canonical result.]"
                }],
                "isError": true,
                "structuredContent": {
                    "code": "workspace_missing",
                    "error": "The active project does not have a plan yet."
                }
            }
        })),
        "",
        None,
    );

    assert!(result.content.contains("workspace_missing"));
    assert!(result.content.contains("does not have a plan"));
    assert!(!result.content.contains("Duplicate JSON text omitted"));
}

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

#[tokio::test]
async fn sequential_execution_stops_while_a_tool_is_still_waiting() {
    let aborted = Arc::new(AtomicBool::new(false));
    let context = ToolCallContext::new(Some("session-1".to_string()), None, None)
        .with_abort_checker(Arc::new({
            let aborted = Arc::clone(&aborted);
            move |_| aborted.load(Ordering::SeqCst)
        }));
    let abort_handle = tokio::spawn({
        let aborted = Arc::clone(&aborted);
        async move {
            tokio::time::sleep(std::time::Duration::from_millis(80)).await;
            aborted.store(true, Ordering::SeqCst);
        }
    });

    let results = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        execute_tool_calls_stream(
            &[json!({"id": "call-1", "function": {"name": "wait", "arguments": "{}"}})],
            context,
            None,
            |_name, _args, _stream| async move {
                std::future::pending::<
                    Result<(String, Option<serde_json::Value>), crate::ToolCallError>,
                >()
                .await
            },
        ),
    )
    .await
    .expect("abort must interrupt a waiting sequential tool");
    abort_handle.await.expect("abort task");

    assert!(results.is_empty());
}

#[tokio::test]
async fn parallel_execution_stops_while_tools_are_still_waiting() {
    let aborted = Arc::new(AtomicBool::new(false));
    let context = ToolCallContext::new(Some("session-1".to_string()), None, None)
        .with_abort_checker(Arc::new({
            let aborted = Arc::clone(&aborted);
            move |_| aborted.load(Ordering::SeqCst)
        }));
    let abort_handle = tokio::spawn({
        let aborted = Arc::clone(&aborted);
        async move {
            tokio::time::sleep(std::time::Duration::from_millis(80)).await;
            aborted.store(true, Ordering::SeqCst);
        }
    });

    let results = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        execute_tool_calls_parallel(
            &[json!({"id": "call-1", "function": {"name": "wait", "arguments": "{}"}})],
            context,
            None,
            |_name, _args, _context, _stream| async move {
                std::future::pending::<
                    Result<(String, Option<serde_json::Value>), crate::ToolCallError>,
                >()
                .await
            },
        ),
    )
    .await
    .expect("abort must interrupt waiting parallel tools");
    abort_handle.await.expect("abort task");

    assert!(results.is_empty());
}
