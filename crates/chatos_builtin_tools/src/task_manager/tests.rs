// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};

use super::*;

#[derive(Debug, Clone)]
struct NoopTaskStore;

#[async_trait]
impl TaskManagerStore for NoopTaskStore {
    async fn create_tasks_for_turn(
        &self,
        _conversation_id: &str,
        _conversation_turn_id: &str,
        _draft_tasks: Vec<TaskDraft>,
    ) -> Result<Vec<Value>, String> {
        Ok(Vec::new())
    }

    async fn review_and_create_tasks(
        &self,
        _conversation_id: &str,
        _conversation_turn_id: &str,
        _draft_tasks: Vec<TaskDraft>,
        _timeout_ms: u64,
        _on_stream_chunk: Option<TaskStreamChunkCallback>,
    ) -> Result<Value, String> {
        Ok(json!({
            "confirmed": false,
            "cancelled": true,
            "reason": "noop",
        }))
    }

    async fn list_tasks_for_context(
        &self,
        _conversation_id: &str,
        _conversation_turn_id: Option<&str>,
        _include_done: bool,
        _limit: usize,
    ) -> Result<Vec<Value>, String> {
        Ok(Vec::new())
    }

    async fn update_task_by_id(
        &self,
        _conversation_id: &str,
        _task_id: &str,
        _patch: TaskUpdatePatch,
    ) -> Result<Value, String> {
        Ok(json!({ "id": "task_1" }))
    }

    async fn complete_task_by_id(
        &self,
        _conversation_id: &str,
        _task_id: &str,
        _patch: Option<TaskUpdatePatch>,
    ) -> Result<Value, String> {
        Ok(json!({ "id": "task_1", "status": "done" }))
    }

    async fn delete_task_by_id(
        &self,
        _conversation_id: &str,
        _task_id: &str,
    ) -> Result<bool, String> {
        Ok(true)
    }

    async fn task_board_updated_event(
        &self,
        _conversation_id: &str,
        _conversation_turn_id: &str,
    ) -> Option<Value> {
        None
    }
}

fn test_service(auto_create_task: bool) -> TaskManagerService {
    TaskManagerService::new(TaskManagerOptions {
        server_name: "task_manager".to_string(),
        review_timeout_ms: 120_000,
        auto_create_task,
        expose_context_ids: true,
        store: TaskManagerStoreRef::new(Arc::new(NoopTaskStore)),
    })
    .expect("task manager service should initialize")
}

fn contains_schema_key(node: &Value, key: &str) -> bool {
    match node {
        Value::Object(map) => map
            .iter()
            .any(|(name, value)| name == key || contains_schema_key(value, key)),
        Value::Array(items) => items.iter().any(|item| contains_schema_key(item, key)),
        _ => false,
    }
}

#[test]
fn parse_task_drafts_supports_single_task_shape() {
    let args = json!({ "title": "Ship task manager", "priority": "high" });
    let drafts = parse_task_drafts(&args).expect("single task payload should parse");
    assert_eq!(drafts.len(), 1);
    assert_eq!(drafts[0].title, "Ship task manager");
    assert_eq!(drafts[0].priority, "high");
}

#[test]
fn parse_task_drafts_supports_optional_prerequisite_ids() {
    let args = json!({
        "tasks": [
            {
                "title": "Implement after review",
                "prerequisite_task_id": "task-review"
            }
        ]
    });
    let drafts = parse_task_drafts(&args).expect("task payload should parse");
    assert_eq!(drafts.len(), 1);
    assert_eq!(
        drafts[0].prerequisite_task_id.as_deref(),
        Some("task-review")
    );
    assert!(drafts[0].prerequisite_task_ids.is_empty());
}

#[test]
fn add_task_schema_is_strict_and_compatible() {
    let add_task_tool = test_service(false)
        .list_tools()
        .into_iter()
        .find(|tool| tool.get("name").and_then(Value::as_str) == Some("add_task"))
        .expect("add_task tool should exist");

    let schema = add_task_tool
        .get("inputSchema")
        .expect("add_task should expose inputSchema");

    assert_eq!(
        schema.get("additionalProperties"),
        Some(&Value::Bool(false))
    );

    let root_properties = schema
        .get("properties")
        .and_then(Value::as_object)
        .expect("add_task schema should expose root properties");
    assert!(
        !root_properties.contains_key("timeout_ms"),
        "add_task schema should not allow timeout override"
    );
    assert!(
        root_properties.contains_key("prerequisite_task_id"),
        "add_task schema should expose optional prerequisite_task_id"
    );
    assert!(
        !root_properties.contains_key("prerequisite_task_ids"),
        "add_task schema should not expose plural prerequisite ids"
    );

    let task_item_schema = schema
        .get("properties")
        .and_then(Value::as_object)
        .and_then(|props| props.get("tasks"))
        .and_then(|tasks| tasks.get("items"))
        .expect("tasks.items schema should exist");

    assert_eq!(
        task_item_schema.get("additionalProperties"),
        Some(&Value::Bool(false))
    );
    assert!(
        task_item_schema
            .get("properties")
            .and_then(Value::as_object)
            .is_some_and(|props| props.contains_key("prerequisite_task_id")),
        "tasks.items schema should expose optional prerequisite_task_id"
    );
    assert!(
        task_item_schema
            .get("properties")
            .and_then(Value::as_object)
            .is_some_and(|props| !props.contains_key("prerequisite_task_ids")),
        "tasks.items schema should not expose plural prerequisite ids"
    );

    assert!(
        !contains_schema_key(schema, "oneOf"),
        "add_task schema should not contain oneOf"
    );
}

#[test]
fn update_patch_supports_null_due_at_and_tags_string() {
    let patch = parse_update_patch(&json!({
        "details": "refresh docs",
        "tags": "backend, task",
        "due_at": null
    }))
    .expect("update patch should parse");

    assert_eq!(patch.details.as_deref(), Some("refresh docs"));
    assert_eq!(
        patch.tags,
        Some(vec!["backend".to_string(), "task".to_string()])
    );
    assert_eq!(patch.due_at, Some(None));
}

#[test]
fn task_manager_tools_include_mutations() {
    let tools = test_service(false).list_tools();
    let tool_names: Vec<&str> = tools
        .iter()
        .filter_map(|tool| tool.get("name").and_then(Value::as_str))
        .collect();

    assert!(tool_names.contains(&"update_task"));
    assert!(tool_names.contains(&"complete_task"));
    assert!(tool_names.contains(&"delete_task"));
}

#[test]
fn task_manager_requires_explicit_conversation_context() {
    let service = test_service(false);

    let missing_conversation = service
        .call_tool("list_tasks", json!({}), None, Some("turn_1"), None)
        .expect_err("task manager must not fall back to a shared conversation");
    assert!(missing_conversation.contains("conversation_id"));

    let missing_turn = service
        .call_tool("list_tasks", json!({}), Some("session_1"), None, None)
        .expect_err("task manager must not fall back to a shared turn");
    assert!(missing_turn.contains("conversation_turn_id"));
}

#[test]
fn task_manager_can_hide_context_ids_from_tool_results() {
    let service = TaskManagerService::new(TaskManagerOptions {
        server_name: "task_manager".to_string(),
        review_timeout_ms: 120_000,
        auto_create_task: true,
        expose_context_ids: false,
        store: TaskManagerStoreRef::new(Arc::new(NoopTaskStore)),
    })
    .expect("task manager service should initialize");

    let result = service
        .call_tool(
            "list_tasks",
            json!({ "current_turn_only": true }),
            Some("task-parent"),
            Some("run-1"),
            None,
        )
        .expect("list_tasks should succeed");
    let payload = result
        .get("_structured_result")
        .expect("tool result should include structured payload");

    assert!(payload.get("conversation_id").is_none());
    assert!(payload.get("conversation_turn_id").is_none());
    assert_eq!(payload.get("count").and_then(Value::as_u64), Some(0));
}

#[test]
fn update_task_schema_changes_is_string() {
    let update_task_tool = test_service(false)
        .list_tools()
        .into_iter()
        .find(|tool| tool.get("name").and_then(Value::as_str) == Some("update_task"))
        .expect("update_task tool should exist");

    let schema = update_task_tool
        .get("inputSchema")
        .expect("update_task should expose inputSchema");

    let changes_type = schema
        .get("properties")
        .and_then(Value::as_object)
        .and_then(|props| props.get("changes"))
        .and_then(|changes| changes.get("type"))
        .and_then(Value::as_str);
    assert_eq!(changes_type, Some("string"));
}

#[test]
fn add_task_description_mentions_confirmation_behavior() {
    let manual_description = test_service(false)
        .list_tools()
        .into_iter()
        .find(|tool| tool.get("name").and_then(Value::as_str) == Some("add_task"))
        .and_then(|tool| {
            tool.get("description")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .expect("manual add_task description");
    let auto_description = test_service(true)
        .list_tools()
        .into_iter()
        .find(|tool| tool.get("name").and_then(Value::as_str) == Some("add_task"))
        .and_then(|tool| {
            tool.get("description")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .expect("auto add_task description");

    assert!(manual_description.contains("confirmed by the user"));
    assert!(auto_description.contains("persisted automatically"));
}
