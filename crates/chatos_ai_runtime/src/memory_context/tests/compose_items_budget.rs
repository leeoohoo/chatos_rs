// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[test]
fn compose_response_to_input_items_keeps_oldest_output_and_rejects_later_overflow() {
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
                "function": {"name": "task_execution_list_tasks", "arguments": "{}"}
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
                "name": "task_execution_list_tasks"
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
            ..Default::default()
        },
    };

    let items = compose_response_to_input_items_with_budget(
        &response,
        Some(ToolResultModelBudgetLimits::new(100, 45)),
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
        outputs.get("call_old").copied(),
        Some("{\"count\":3,\"tasks\":[\"older-task-state\"]}")
    );
    let diagnostic: Value = serde_json::from_str(outputs["call_new"]).expect("overflow JSON");
    assert_eq!(diagnostic["error"], "tool_result_budget_exceeded");
    assert_eq!(diagnostic["reason"], "total_budget");
    assert_eq!(diagnostic["available_chars"], 5);
    assert_eq!(diagnostic["content_preserved"], false);
    assert!(!outputs["call_new"].starts_with("{\"cou"));
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
            ..Default::default()
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

#[test]
fn compose_response_to_input_items_expires_run_scoped_references_from_memory() {
    let team_ref = format!("team_ref_{}", "a".repeat(64));
    let asset_ref = format!("asset_ref_{}", "b".repeat(64));
    let todo_ref = format!("todo_ref_{}", "c".repeat(64));
    let response = ComposeContextResponse {
        thread_id: "thread-1".to_string(),
        blocks: vec![ComposeContextBlock {
            block_type: "thread_summary".to_string(),
            text: format!("Previous Todo used {todo_ref}"),
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
                content: format!("Inspecting the historical team {team_ref}"),
                structured_payload: None,
                metadata: Some(json!({
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {
                            "name": "team_asset_list",
                            "arguments": json!({"team_ref": team_ref}).to_string()
                        }
                    }]
                })),
                summary_status: "pending".to_string(),
                summary_id: None,
                summarized_at: None,
                created_at: "2026-11-18T00:00:00Z".to_string(),
            },
            EngineRecord {
                id: "rec-2".to_string(),
                thread_id: "thread-1".to_string(),
                tenant_id: "tenant-1".to_string(),
                source_id: "task".to_string(),
                external_record_id: None,
                role: "tool".to_string(),
                record_type: "message".to_string(),
                content: json!({"asset_ref": asset_ref, "title": "Decision"}).to_string(),
                structured_payload: None,
                metadata: Some(json!({
                    "tool_call_id": "call_1",
                    "name": "team_asset_list"
                })),
                summary_status: "pending".to_string(),
                summary_id: None,
                summarized_at: None,
                created_at: "2026-11-18T00:00:01Z".to_string(),
            },
        ],
        meta: ComposeContextMeta {
            summary_count: 1,
            recent_record_count: 2,
            ..Default::default()
        },
    };

    let encoded = Value::Array(compose_response_to_input_items(&response)).to_string();

    assert!(!encoded.contains(&team_ref));
    assert!(!encoded.contains(&asset_ref));
    assert!(!encoded.contains(&todo_ref));
    assert!(encoded.matches("[expired_run_scoped_reference]").count() >= 4);
    assert!(encoded.contains("\"call_id\":\"call_1\""));
    assert!(encoded.contains("Decision"));
}
