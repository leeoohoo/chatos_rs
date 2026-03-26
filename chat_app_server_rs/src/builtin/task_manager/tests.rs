use serde_json::{json, Value};

use super::parsing::{parse_task_drafts, parse_update_patch};
use super::{TaskManagerOptions, TaskManagerService};

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
fn add_task_schema_is_strict_and_compatible() {
    let service = TaskManagerService::new(TaskManagerOptions {
        server_name: "task_manager".to_string(),
        review_timeout_ms: 120_000,
    })
    .expect("task manager service should initialize");

    let add_task_tool = service
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
    let service = TaskManagerService::new(TaskManagerOptions {
        server_name: "task_manager".to_string(),
        review_timeout_ms: 120_000,
    })
    .expect("task manager service should initialize");

    let tools = service.list_tools();
    let tool_names: Vec<&str> = tools
        .iter()
        .filter_map(|tool| tool.get("name").and_then(Value::as_str))
        .collect();

    assert!(tool_names.contains(&"update_task"));
    assert!(tool_names.contains(&"complete_task"));
    assert!(tool_names.contains(&"delete_task"));
}

#[test]
fn update_task_schema_changes_is_string() {
    let service = TaskManagerService::new(TaskManagerOptions {
        server_name: "task_manager".to_string(),
        review_timeout_ms: 120_000,
    })
    .expect("task manager service should initialize");

    let update_task_tool = service
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
