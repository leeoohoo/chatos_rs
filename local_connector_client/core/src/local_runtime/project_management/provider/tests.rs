// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;

use chatos_mcp_runtime::{BuiltinToolProvider, ToolCallContext};
use chatos_project_mcp_contract::tools;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::local_runtime::storage::{LocalDatabase, UpsertLocalProjectInput};

use super::LocalProjectManagementProvider;

const USER_ID: &str = "provider-user";
const PROJECT_ID: &str = "provider-project";

#[tokio::test]
async fn project_management_tools_write_and_read_local_sqlite() {
    let root = std::env::temp_dir().join(format!("chatos-local-pm-provider-{}", Uuid::new_v4()));
    let database = LocalDatabase::open(root.join("runtime.sqlite3"))
        .await
        .expect("open provider database");
    database
        .upsert_project(UpsertLocalProjectInput {
            project_id: PROJECT_ID.to_string(),
            owner_user_id: USER_ID.to_string(),
            device_id: "provider-device".to_string(),
            workspace_id: "provider-workspace".to_string(),
            project_name: "Provider project".to_string(),
            root_relative_path: None,
        })
        .await
        .expect("upsert provider project");
    let provider = LocalProjectManagementProvider::new(database.clone(), USER_ID, PROJECT_ID);
    assert_eq!(
        provider.list_tools().len(),
        tools::PROJECT_MANAGEMENT_SERVER_TOOL_NAMES.len()
    );

    let prerequisite = call(
        &provider,
        tools::CREATE_REQUIREMENT,
        json!({ "title": "Prepare SQLite schema", "status": "approved" }),
    )
    .await;
    let prerequisite_id = string_at(&prerequisite, "/id");
    let requirement = call(
        &provider,
        tools::CREATE_REQUIREMENT,
        json!({ "title": "Implement local project tools", "status": "approved" }),
    )
    .await;
    let requirement_id = string_at(&requirement, "/id");

    let updated = call(
        &provider,
        tools::UPDATE_REQUIREMENT,
        json!({
            "requirement_id": requirement_id,
            "patch": { "title": "Implement local project MCP", "status": "in_progress" },
            "prerequisite_requirement_ids": [prerequisite_id]
        }),
    )
    .await;
    assert_eq!(
        updated
            .pointer("/requirement/title")
            .and_then(Value::as_str),
        Some("Implement local project MCP")
    );

    let document = call(
        &provider,
        tools::UPSERT_REQUIREMENT_TECHNICAL_DOCUMENT,
        json!({
            "requirement_id": requirement_id,
            "doc_type": "implementation_plan",
            "title": "Local implementation plan",
            "content": "Use the in-process SQLite provider."
        }),
    )
    .await;
    assert_eq!(document.get("version").and_then(Value::as_i64), Some(1));

    let task = call(
        &provider,
        tools::CREATE_PROJECT_TASK,
        json!({
            "requirement_id": requirement_id,
            "title": "Wire Plan Mode",
            "status": "ready",
            "tags": ["local", "sqlite"]
        }),
    )
    .await;
    let task_id = string_at(&task, "/project_task/id");
    let graph = call(&provider, tools::GET_PROJECT_DEPENDENCY_GRAPH, json!({})).await;
    assert!(has_edge(
        &graph,
        format!("requirement:{prerequisite_id}").as_str(),
        format!("requirement:{requirement_id}").as_str(),
    ));
    assert!(has_edge(
        &graph,
        format!("requirement:{requirement_id}").as_str(),
        format!("work_item:{task_id}").as_str(),
    ));

    assert_eq!(
        database
            .list_local_requirements(USER_ID, PROJECT_ID, false)
            .await
            .expect("list provider requirements")
            .len(),
        2
    );
    assert_eq!(
        database
            .list_local_project_work_items(USER_ID, PROJECT_ID, false)
            .await
            .expect("list provider work items")
            .len(),
        1
    );
    database.close().await;
    fs::remove_dir_all(root).expect("cleanup provider database");
}

async fn call(provider: &LocalProjectManagementProvider, name: &str, args: Value) -> Value {
    let result = provider
        .call_tool(name, args, ToolCallContext::default(), None)
        .await
        .unwrap_or_else(|error| panic!("call {name}: {error}"));
    let text = result
        .pointer("/content/0/text")
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("tool {name} did not return text content"));
    serde_json::from_str(text).unwrap_or_else(|error| panic!("decode {name} result: {error}"))
}

fn string_at(value: &Value, pointer: &str) -> String {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("missing string at {pointer}"))
        .to_string()
}

fn has_edge(graph: &Value, from: &str, to: &str) -> bool {
    graph
        .get("edges")
        .and_then(Value::as_array)
        .is_some_and(|edges| {
            edges.iter().any(|edge| {
                edge.get("from").and_then(Value::as_str) == Some(from)
                    && edge.get("to").and_then(Value::as_str) == Some(to)
            })
        })
}
