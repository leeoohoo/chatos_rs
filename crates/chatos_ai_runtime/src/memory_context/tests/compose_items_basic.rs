// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[test]
fn compose_response_to_input_items_rebuilds_tool_exchange_in_responses_shape() {
    let response = ComposeContextResponse {
        thread_id: "thread-1".to_string(),
        blocks: vec![ComposeContextBlock {
            block_type: "summary".to_string(),
            text: "recent summary".to_string(),
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
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
fn compose_response_to_input_items_rejects_unpaged_overflow_without_damaged_excerpt() {
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
                content: "x".repeat(160_001),
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
            ..Default::default()
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

    let diagnostic: Value = serde_json::from_str(output).expect("valid overflow diagnostic");
    assert_eq!(diagnostic["error"], "tool_result_budget_exceeded");
    assert_eq!(diagnostic["tool"], "code.read_file");
    assert_eq!(diagnostic["original_chars"], 160_001);
    assert_eq!(diagnostic["content_preserved"], false);
    assert!(!output.contains("tool_result_truncated"));
    assert!(!output.starts_with('x'));
}
