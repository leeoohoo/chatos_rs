// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    notepad_model_tools, LocalNotepadToolExecutor, LocalToolExecutor, NOTEPAD_CREATE_NOTE_TOOL,
    NOTEPAD_LIST_NOTES_TOOL, NOTEPAD_READ_NOTE_TOOL, NOTEPAD_TOOL_NAMES, NOTEPAD_UPDATE_NOTE_TOOL,
};
use chatos_client_storage::SqliteClientStorage;
use chatos_local_agent_protocol::{
    CreateRunCommand, HostCommand, HostRequestEnvelope, LocalAgentToolApprovalStatus,
    LocalAgentToolInvocationRecord, LocalAgentToolOutcome, LocalAgentToolStatus,
    LOCAL_AGENT_PROTOCOL_VERSION,
};
use chatos_local_agent_runtime::LocalAgentRuntime;
use serde_json::{json, Value};
use std::sync::Arc;

async fn runtime_with_parent(owner: &str) -> Arc<LocalAgentRuntime> {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let runtime = Arc::new(LocalAgentRuntime::new(storage));
    runtime.initialize(owner).await.expect("initialize");
    runtime
        .try_handle(HostRequestEnvelope {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            command_id: "create-notepad-parent".to_string(),
            command: HostCommand::CreateRun(CreateRunCommand {
                run_id: "notepad-parent".to_string(),
                owner_user_id: owner.to_string(),
                owner_entity_type: "conversation".to_string(),
                owner_entity_id: "conversation-1".to_string(),
                profile_key: "main_chat".to_string(),
                model_config_ref: "model-1".to_string(),
                model_config_revision: "revision-1".to_string(),
                capability_policy_revision: "policy-1".to_string(),
                input: json!({"message": "remember this"}),
                max_iterations: 8,
            }),
        })
        .await
        .expect("create parent");
    runtime
}

fn invocation(index: u32, tool_name: &str, arguments: Value) -> LocalAgentToolInvocationRecord {
    LocalAgentToolInvocationRecord {
        invocation_id: format!("notepad-invocation-{index}"),
        run_id: "notepad-parent".to_string(),
        batch_id: format!("notepad-batch-{index}"),
        call_id: format!("notepad-call-{index}"),
        tool_name: tool_name.to_string(),
        arguments,
        side_effecting: matches!(
            tool_name,
            NOTEPAD_CREATE_NOTE_TOOL | NOTEPAD_UPDATE_NOTE_TOOL
        ),
        requires_approval: matches!(
            tool_name,
            NOTEPAD_CREATE_NOTE_TOOL | NOTEPAD_UPDATE_NOTE_TOOL
        ),
        approval_status: LocalAgentToolApprovalStatus::Approved,
        approval_decided_by: Some("user-1".to_string()),
        approval_reason: Some("test".to_string()),
        approval_decided_at_unix_ms: Some(1_000),
        status: LocalAgentToolStatus::Running,
        result: None,
        error: None,
        version: 2,
        claim_token: Some("claim-1".to_string()),
        claim_until_unix_ms: Some(i64::MAX),
        created_at_unix_ms: 1_000,
        updated_at_unix_ms: 1_000,
    }
}

fn succeeded(outcome: LocalAgentToolOutcome) -> Value {
    match outcome {
        LocalAgentToolOutcome::Succeeded { output } => output,
        other => panic!("expected successful tool outcome, got {other:?}"),
    }
}

#[tokio::test]
async fn local_notepad_tools_create_read_update_and_search_for_active_owner() {
    let runtime = runtime_with_parent("user-1").await;
    let executor = LocalNotepadToolExecutor::new(runtime, "user-1").expect("executor");
    let created = succeeded(
        executor
            .execute_tool(&invocation(
                1,
                NOTEPAD_CREATE_NOTE_TOOL,
                json!({
                    "folder": " work / ideas ",
                    "title": " Local Host ",
                    "content": "# Local Host\n\nSQLite-backed note",
                    "tags": [" Rust ", "rust", "Offline"]
                }),
            ))
            .await
            .expect("create outcome"),
    );
    let note_id = created["note"]["note_id"]
        .as_str()
        .expect("created note id")
        .to_string();
    assert_eq!(created["note"]["owner_user_id"], "user-1");
    assert_eq!(created["note"]["folder"], "work/ideas");
    assert_eq!(created["note"]["tags"], json!(["Rust", "Offline"]));

    let listed = succeeded(
        executor
            .execute_tool(&invocation(
                2,
                NOTEPAD_LIST_NOTES_TOOL,
                json!({"query": "SQLite", "limit": 20}),
            ))
            .await
            .expect("list outcome"),
    );
    assert_eq!(listed["notes"].as_array().expect("notes").len(), 1);

    let updated = succeeded(
        executor
            .execute_tool(&invocation(
                3,
                NOTEPAD_UPDATE_NOTE_TOOL,
                json!({"id": note_id, "title": "Local Runtime"}),
            ))
            .await
            .expect("update outcome"),
    );
    assert_eq!(updated["note"]["title"], "Local Runtime");
    assert_eq!(updated["note"]["version"], 2);

    let read = succeeded(
        executor
            .execute_tool(&invocation(
                4,
                NOTEPAD_READ_NOTE_TOOL,
                json!({"id": updated["note"]["note_id"]}),
            ))
            .await
            .expect("read outcome"),
    );
    assert_eq!(read["content"], "# Local Host\n\nSQLite-backed note");
}

#[tokio::test]
async fn model_cannot_override_the_notepad_owner() {
    let runtime = runtime_with_parent("user-1").await;
    let executor = LocalNotepadToolExecutor::new(runtime, "user-1").expect("executor");
    let outcome = executor
        .execute_tool(&invocation(
            1,
            NOTEPAD_CREATE_NOTE_TOOL,
            json!({"title": "escape", "owner_user_id": "user-2"}),
        ))
        .await
        .expect("outcome");
    let LocalAgentToolOutcome::Failed { error, .. } = outcome else {
        panic!("owner injection must fail")
    };
    assert!(error.contains("unknown field `owner_user_id`"));
}

#[test]
fn local_notepad_definitions_are_complete_and_never_publish_owner_input() {
    let tools = notepad_model_tools();
    let names = tools
        .iter()
        .filter_map(|tool| tool.get("name").and_then(Value::as_str))
        .collect::<Vec<_>>();
    assert_eq!(names, NOTEPAD_TOOL_NAMES);
    assert!(tools.iter().all(|tool| tool
        .pointer("/parameters/properties/owner_user_id")
        .is_none()));
}
