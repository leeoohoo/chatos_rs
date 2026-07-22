// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::{collections::HashMap, time::Duration};

use memory_engine_sdk::{
    ComposeContextBlock, ComposeContextMeta, ComposeContextPolicy, ComposeContextResponse,
    EngineRecord,
};
use serde_json::json;

use super::{
    compose_response_to_input_items, compose_response_to_input_items_with_budget,
    MemoryContextComposer, MemoryRecordScope, MemoryScope,
};
use crate::tool_runtime::ToolResultModelBudgetLimits;

#[test]
fn memory_scope_builder_keeps_runtime_source_key() {
    let policy = ComposeContextPolicy {
        include_recent_records: Some(false),
        include_thread_summary: Some(true),
        include_subject_memory: Some(true),
        recent_record_limit: Some(12),
        summary_limit: Some(3),
    };
    let scope = MemoryScope::thread("tenant_1", "task_runner", "task_thread_1")
        .with_subject_id("contact_1")
        .with_related_subject_ids(["project_1", "agent_1"])
        .with_policy(policy);

    assert_eq!(scope.tenant_id, "tenant_1");
    assert_eq!(scope.source_id, "task_runner");
    assert_eq!(scope.thread_id, "task_thread_1");
    assert_eq!(scope.subject_id.as_deref(), Some("contact_1"));
    assert_eq!(scope.related_subject_ids, vec!["project_1", "agent_1"]);
    assert_eq!(
        scope
            .policy
            .as_ref()
            .and_then(|value| value.recent_record_limit),
        Some(12)
    );
}

#[test]
fn memory_record_scope_builder_defaults_to_pending_message_records() {
    let scope = MemoryRecordScope::new("tenant_1")
        .with_thread_id("thread_1")
        .with_record_type("task_message")
        .with_default_summary_status(None);

    assert_eq!(scope.tenant_id, "tenant_1");
    assert_eq!(scope.thread_id.as_deref(), Some("thread_1"));
    assert_eq!(scope.record_type, "task_message");
    assert!(scope.default_summary_status.is_none());

    let message_scope = MemoryRecordScope::message_thread("tenant_1", "thread_2");
    assert_eq!(message_scope.record_type, "message");
    assert_eq!(
        message_scope.default_summary_status.as_deref(),
        Some("pending")
    );
}

#[test]
fn direct_composer_rejects_mismatched_scope_source_key() {
    let composer =
        MemoryContextComposer::new_direct("http://127.0.0.1:1", Duration::from_secs(1), "chatos")
            .expect("composer");
    assert_eq!(composer.source_id(), Some("chatos"));

    let matching = MemoryScope::thread("tenant_1", "chatos", "thread_1");
    composer
        .validate_scope_source(&matching)
        .expect("matching scope source");

    let mismatched = MemoryScope::thread("tenant_1", "task_runner", "thread_1");
    let err = composer
        .validate_scope_source(&mismatched)
        .expect_err("mismatched scope source");
    assert!(err.contains("source_id mismatch"));
}

#[test]
fn compose_response_to_input_items_rebuilds_tool_exchange_in_responses_shape() {
    let response = ComposeContextResponse {
        thread_id: "thread-1".to_string(),
        blocks: vec![ComposeContextBlock {
            block_type: "summary".to_string(),
            text: "recent summary".to_string(),
        }],
        recent_records: vec![
            EngineRecord {
                id: "rec-1".to_string(),
                thread_id: "thread-1".to_string(),
                tenant_id: "tenant-1".to_string(),
                source_id: "task".to_string(),
                external_record_id: None,
                role: "assistant".to_string(),
                record_type: "message".to_string(),
                content: "calling tool".to_string(),
                structured_payload: None,
                metadata: Some(json!({
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {
                            "name": "demo.search",
                            "arguments": "{\"q\":\"rust\"}"
                        }
                    }]
                })),
                summary_status: "pending".to_string(),
                summary_id: None,
                summarized_at: None,
                created_at: "2026-06-08T00:00:00Z".to_string(),
            },
            EngineRecord {
                id: "rec-2".to_string(),
                thread_id: "thread-1".to_string(),
                tenant_id: "tenant-1".to_string(),
                source_id: "task".to_string(),
                external_record_id: None,
                role: "tool".to_string(),
                record_type: "message".to_string(),
                content: "done".to_string(),
                structured_payload: None,
                metadata: Some(json!({
                    "tool_call_id": "call_1"
                })),
                summary_status: "pending".to_string(),
                summary_id: None,
                summarized_at: None,
                created_at: "2026-06-08T00:00:01Z".to_string(),
            },
        ],
        meta: ComposeContextMeta {
            summary_count: 1,
            recent_record_count: 2,
        },
    };

    let items = compose_response_to_input_items(&response);
    assert_eq!(
        items[0].get("type").and_then(|value| value.as_str()),
        Some("message")
    );
    assert!(items.iter().any(|item| {
        item.get("type").and_then(|value| value.as_str()) == Some("function_call")
            && item.get("call_id").and_then(|value| value.as_str()) == Some("call_1")
    }));
    assert!(items.iter().any(|item| {
        item.get("type").and_then(|value| value.as_str()) == Some("function_call_output")
            && item.get("call_id").and_then(|value| value.as_str()) == Some("call_1")
    }));
    assert!(!items
        .iter()
        .any(|item| { item.get("role").and_then(|value| value.as_str()) == Some("tool") }));
}

#[test]
fn compose_response_to_input_items_skips_orphan_tool_outputs() {
    let response = ComposeContextResponse {
        thread_id: "thread-1".to_string(),
        blocks: Vec::new(),
        recent_records: vec![EngineRecord {
            id: "rec-1".to_string(),
            thread_id: "thread-1".to_string(),
            tenant_id: "tenant-1".to_string(),
            source_id: "task".to_string(),
            external_record_id: None,
            role: "tool".to_string(),
            record_type: "message".to_string(),
            content: "done".to_string(),
            structured_payload: None,
            metadata: Some(json!({
                "tool_call_id": "call_missing"
            })),
            summary_status: "pending".to_string(),
            summary_id: None,
            summarized_at: None,
            created_at: "2026-06-08T00:00:01Z".to_string(),
        }],
        meta: ComposeContextMeta {
            summary_count: 0,
            recent_record_count: 1,
        },
    };

    let items = compose_response_to_input_items(&response);
    assert!(items.is_empty());
}

#[test]
fn compose_response_to_input_items_skips_orphan_tool_calls() {
    let response = ComposeContextResponse {
        thread_id: "thread-1".to_string(),
        blocks: Vec::new(),
        recent_records: vec![EngineRecord {
            id: "rec-1".to_string(),
            thread_id: "thread-1".to_string(),
            tenant_id: "tenant-1".to_string(),
            source_id: "task".to_string(),
            external_record_id: None,
            role: "assistant".to_string(),
            record_type: "message".to_string(),
            content: "calling tool".to_string(),
            structured_payload: None,
            metadata: Some(json!({
                "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": {
                        "name": "demo.search",
                        "arguments": "{\"q\":\"rust\"}"
                    }
                }]
            })),
            summary_status: "pending".to_string(),
            summary_id: None,
            summarized_at: None,
            created_at: "2026-06-08T00:00:00Z".to_string(),
        }],
        meta: ComposeContextMeta {
            summary_count: 0,
            recent_record_count: 1,
        },
    };

    let items = compose_response_to_input_items(&response);

    assert!(items.iter().any(|item| {
        item.get("type").and_then(|value| value.as_str()) == Some("message")
            && item.get("role").and_then(|value| value.as_str()) == Some("assistant")
    }));
    assert!(!items.iter().any(|item| {
        item.get("type").and_then(|value| value.as_str()) == Some("function_call")
    }));
}

#[test]
fn compose_response_to_input_items_omits_large_tool_outputs() {
    let response = ComposeContextResponse {
        thread_id: "thread-1".to_string(),
        blocks: Vec::new(),
        recent_records: vec![
            EngineRecord {
                id: "rec-1".to_string(),
                thread_id: "thread-1".to_string(),
                tenant_id: "tenant-1".to_string(),
                source_id: "task".to_string(),
                external_record_id: None,
                role: "assistant".to_string(),
                record_type: "message".to_string(),
                content: "calling tool".to_string(),
                structured_payload: None,
                metadata: Some(json!({
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {
                            "name": "code.read_file",
                            "arguments": "{\"path\":\"big.log\"}"
                        }
                    }]
                })),
                summary_status: "pending".to_string(),
                summary_id: None,
                summarized_at: None,
                created_at: "2026-06-08T00:00:00Z".to_string(),
            },
            EngineRecord {
                id: "rec-2".to_string(),
                thread_id: "thread-1".to_string(),
                tenant_id: "tenant-1".to_string(),
                source_id: "task".to_string(),
                external_record_id: None,
                role: "tool".to_string(),
                record_type: "message".to_string(),
                content: "x".repeat(9_000),
                structured_payload: None,
                metadata: Some(json!({
                    "tool_call_id": "call_1",
                    "name": "code.read_file"
                })),
                summary_status: "pending".to_string(),
                summary_id: None,
                summarized_at: None,
                created_at: "2026-06-08T00:00:01Z".to_string(),
            },
        ],
        meta: ComposeContextMeta {
            summary_count: 0,
            recent_record_count: 2,
        },
    };

    let items = compose_response_to_input_items(&response);
    let output = items
        .iter()
        .find(|item| {
            item.get("type").and_then(|value| value.as_str()) == Some("function_call_output")
        })
        .and_then(|item| item.get("output"))
        .and_then(|value| value.as_str())
        .unwrap_or_default();

    assert!(output.contains("Tool result omitted"));
    assert!(output.contains("code.read_file"));
    assert!(output.len() < 1_000);
}

#[test]
fn compose_response_to_input_items_prioritizes_latest_tool_output_with_total_budget() {
    let tool_call_record = |record_id: &str, call_id: &str, created_at: &str| EngineRecord {
        id: record_id.to_string(),
        thread_id: "thread-1".to_string(),
        tenant_id: "tenant-1".to_string(),
        source_id: "task".to_string(),
        external_record_id: None,
        role: "assistant".to_string(),
        record_type: "message".to_string(),
        content: "calling tool".to_string(),
        structured_payload: None,
        metadata: Some(json!({
            "tool_calls": [{
                "id": call_id,
                "type": "function",
                "function": {"name": "task_manager_list_tasks", "arguments": "{}"}
            }]
        })),
        summary_status: "pending".to_string(),
        summary_id: None,
        summarized_at: None,
        created_at: created_at.to_string(),
    };
    let tool_output_record =
        |record_id: &str, call_id: &str, content: &str, created_at: &str| EngineRecord {
            id: record_id.to_string(),
            thread_id: "thread-1".to_string(),
            tenant_id: "tenant-1".to_string(),
            source_id: "task".to_string(),
            external_record_id: None,
            role: "tool".to_string(),
            record_type: "message".to_string(),
            content: content.to_string(),
            structured_payload: None,
            metadata: Some(json!({
                "tool_call_id": call_id,
                "name": "task_manager_list_tasks"
            })),
            summary_status: "pending".to_string(),
            summary_id: None,
            summarized_at: None,
            created_at: created_at.to_string(),
        };
    let response = ComposeContextResponse {
        thread_id: "thread-1".to_string(),
        blocks: Vec::new(),
        recent_records: vec![
            tool_call_record("rec-1", "call_old", "2026-06-08T00:00:00Z"),
            tool_output_record(
                "rec-2",
                "call_old",
                "{\"count\":3,\"tasks\":[\"older-task-state\"]}",
                "2026-06-08T00:00:01Z",
            ),
            tool_call_record("rec-3", "call_new", "2026-06-08T00:00:02Z"),
            tool_output_record(
                "rec-4",
                "call_new",
                "{\"count\":0,\"tasks\":[]}",
                "2026-06-08T00:00:03Z",
            ),
        ],
        meta: ComposeContextMeta {
            summary_count: 0,
            recent_record_count: 4,
        },
    };

    let items = compose_response_to_input_items_with_budget(
        &response,
        Some(ToolResultModelBudgetLimits::new(100, 30)),
    );
    let outputs = items
        .iter()
        .filter_map(|item| {
            let call_id = item.get("call_id").and_then(|value| value.as_str())?;
            let output = item.get("output").and_then(|value| value.as_str())?;
            Some((call_id, output))
        })
        .collect::<HashMap<_, _>>();

    assert_eq!(
        outputs.get("call_new").copied(),
        Some("{\"count\":0,\"tasks\":[]}")
    );
    assert!(outputs
        .get("call_old")
        .is_some_and(|output| output.contains("combined tool results exceed")));
}

#[test]
fn compose_response_to_input_items_reads_tool_calls_from_structured_payload() {
    let response = ComposeContextResponse {
        thread_id: "thread-1".to_string(),
        blocks: Vec::new(),
        recent_records: vec![
            EngineRecord {
                id: "rec-1".to_string(),
                thread_id: "thread-1".to_string(),
                tenant_id: "tenant-1".to_string(),
                source_id: "task".to_string(),
                external_record_id: None,
                role: "assistant".to_string(),
                record_type: "message".to_string(),
                content: "calling tool".to_string(),
                structured_payload: Some(json!([{
                    "id": "call_1",
                    "type": "function",
                    "function": {
                        "name": "demo.search",
                        "arguments": "{\"q\":\"rust\"}"
                    }
                }])),
                metadata: None,
                summary_status: "pending".to_string(),
                summary_id: None,
                summarized_at: None,
                created_at: "2026-06-08T00:00:00Z".to_string(),
            },
            EngineRecord {
                id: "rec-2".to_string(),
                thread_id: "thread-1".to_string(),
                tenant_id: "tenant-1".to_string(),
                source_id: "task".to_string(),
                external_record_id: None,
                role: "tool".to_string(),
                record_type: "message".to_string(),
                content: "done".to_string(),
                structured_payload: None,
                metadata: Some(json!({
                    "tool_call_id": "call_1"
                })),
                summary_status: "pending".to_string(),
                summary_id: None,
                summarized_at: None,
                created_at: "2026-06-08T00:00:01Z".to_string(),
            },
        ],
        meta: ComposeContextMeta {
            summary_count: 0,
            recent_record_count: 2,
        },
    };

    let items = compose_response_to_input_items(&response);
    assert!(items.iter().any(|item| {
        item.get("type").and_then(|value| value.as_str()) == Some("function_call")
            && item.get("call_id").and_then(|value| value.as_str()) == Some("call_1")
    }));
    assert!(items.iter().any(|item| {
        item.get("type").and_then(|value| value.as_str()) == Some("function_call_output")
            && item.get("call_id").and_then(|value| value.as_str()) == Some("call_1")
    }));
}
