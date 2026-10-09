// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    LocalAgentRuntime, LocalTaskToolExecutor, LocalToolExecutor, CANCEL_TASK_TOOL,
    CREATE_TASK_TOOL, GET_TASK_DEPENDENCY_GRAPH_TOOL, GET_TASK_TOOL, LIST_TASKS_TOOL,
    WAIT_FOR_TASK_COMPLETION_TOOL,
};
use chatos_client_storage::SqliteClientStorage;
use chatos_local_agent_protocol::{
    CreateConversationCommand, HostCommand, HostRequestEnvelope, HostResult,
    LocalAgentToolApprovalStatus, LocalAgentToolInvocationRecord, LocalAgentToolOutcome,
    LocalAgentToolStatus, LocalConversationResourceBinding, LocalConversationResourceKind,
    StartConversationTurnCommand, LOCAL_AGENT_PROTOCOL_VERSION,
};
use serde_json::{json, Value};
use std::sync::Arc;

#[path = "task_revision_tests.rs"]
mod revisions;

fn envelope(command_id: &str, command: HostCommand) -> HostRequestEnvelope {
    HostRequestEnvelope {
        protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
        command_id: command_id.to_string(),
        command,
    }
}

async fn add_project_conversation(
    runtime: &LocalAgentRuntime,
    conversation_id: &str,
    project_id: &str,
    turn_id: &str,
    run_id: &str,
) {
    runtime
        .try_handle(envelope(
            &format!("create-{conversation_id}"),
            HostCommand::CreateConversation(CreateConversationCommand {
                conversation_id: conversation_id.to_string(),
                owner_user_id: "user-1".to_string(),
                title: format!("Conversation {conversation_id}"),
                resource: Some(LocalConversationResourceBinding {
                    kind: LocalConversationResourceKind::Project,
                    resource_id: project_id.to_string(),
                }),
            }),
        ))
        .await
        .expect("create conversation");
    runtime
        .try_handle(envelope(
            &format!("start-{turn_id}"),
            HostCommand::StartConversationTurn(StartConversationTurnCommand {
                owner_user_id: "user-1".to_string(),
                conversation_id: conversation_id.to_string(),
                expected_conversation_version: 1,
                turn_id: turn_id.to_string(),
                message_id: format!("message-{turn_id}"),
                run_id: run_id.to_string(),
                message: "Inspect the bound project".to_string(),
                message_metadata: Value::Null,
                attachments: Vec::new(),
                model_config_ref: "model-1".to_string(),
                model_config_revision: "revision-1".to_string(),
                capability_policy_revision: "policy-1".to_string(),
                max_iterations: 8,
            }),
        ))
        .await
        .expect("start conversation turn");
}

async fn runtime_with_project_conversations() -> Arc<LocalAgentRuntime> {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let runtime = Arc::new(LocalAgentRuntime::new(storage));
    runtime.initialize("user-1").await.expect("initialize");
    add_project_conversation(
        &runtime,
        "conversation-1",
        "project-1",
        "turn-1",
        "parent-run",
    )
    .await;
    add_project_conversation(
        &runtime,
        "conversation-2",
        "project-2",
        "turn-2",
        "other-parent-run",
    )
    .await;
    runtime
}

fn invocation(
    invocation_id: &str,
    run_id: &str,
    tool_name: &str,
    arguments: Value,
) -> LocalAgentToolInvocationRecord {
    LocalAgentToolInvocationRecord {
        invocation_id: invocation_id.to_string(),
        run_id: run_id.to_string(),
        batch_id: format!("batch-{invocation_id}"),
        call_id: format!("call-{invocation_id}"),
        tool_name: tool_name.to_string(),
        arguments,
        side_effecting: matches!(tool_name, CREATE_TASK_TOOL | CANCEL_TASK_TOOL),
        requires_approval: false,
        approval_status: LocalAgentToolApprovalStatus::NotRequired,
        approval_decided_by: None,
        approval_reason: None,
        approval_decided_at_unix_ms: None,
        status: LocalAgentToolStatus::Running,
        result: None,
        error: None,
        version: 2,
        claim_token: Some(format!("claim-{invocation_id}")),
        claim_until_unix_ms: Some(i64::MAX),
        created_at_unix_ms: 1_000,
        updated_at_unix_ms: 1_000,
    }
}

async fn succeeded_output(
    executor: &LocalTaskToolExecutor,
    invocation: LocalAgentToolInvocationRecord,
) -> Value {
    match executor
        .execute_tool(&invocation)
        .await
        .expect("execute Task tool")
    {
        LocalAgentToolOutcome::Succeeded { output } => output,
        outcome => panic!("unexpected Task tool outcome: {outcome:?}"),
    }
}

#[tokio::test]
async fn main_chat_task_tools_query_and_mutate_only_the_current_project_scope() {
    let runtime = runtime_with_project_conversations().await;
    let executor = LocalTaskToolExecutor::new(Arc::clone(&runtime), "user-1").expect("executor");

    let current = succeeded_output(
        &executor,
        invocation(
            "create-current",
            "parent-run",
            CREATE_TASK_TOOL,
            json!({
                "title": "Inspect README",
                "objective": "Read the bound project's README and explain the project",
                "requires_execution": false,
                "enabled_builtin_kinds": ["CodeMaintainerRead"]
            }),
        ),
    )
    .await;
    let current_task_id = current["id"].as_str().expect("current Task id").to_string();
    let other = succeeded_output(
        &executor,
        invocation(
            "create-other",
            "other-parent-run",
            CREATE_TASK_TOOL,
            json!({
                "title": "Other project",
                "objective": "Inspect a different bound project",
                "requires_execution": false,
                "enabled_builtin_kinds": ["CodeMaintainerRead"]
            }),
        ),
    )
    .await;
    let other_task_id = other["id"].as_str().expect("other Task id").to_string();

    let listed = succeeded_output(
        &executor,
        invocation(
            "list-current",
            "parent-run",
            LIST_TASKS_TOOL,
            json!({"status": "queued", "keyword": "README"}),
        ),
    )
    .await;
    assert_eq!(listed.as_array().map(Vec::len), Some(1));
    assert_eq!(listed[0]["id"], current_task_id);

    let loaded = succeeded_output(
        &executor,
        invocation(
            "get-current",
            "parent-run",
            GET_TASK_TOOL,
            json!({"task_id": current_task_id}),
        ),
    )
    .await;
    assert_eq!(loaded["title"], "Inspect README");
    assert!(loaded.get("owner_user_id").is_none());
    assert!(loaded.get("model_config_revision").is_none());

    let cross_project_error = executor
        .execute_tool(&invocation(
            "get-other",
            "parent-run",
            GET_TASK_TOOL,
            json!({"task_id": other_task_id}),
        ))
        .await
        .expect_err("another project's Task must stay hidden");
    assert!(cross_project_error.contains("task not found"));

    let dependency_graph = succeeded_output(
        &executor,
        invocation(
            "graph-current",
            "parent-run",
            GET_TASK_DEPENDENCY_GRAPH_TOOL,
            json!({"task_id": current_task_id}),
        ),
    )
    .await;
    assert_eq!(dependency_graph["task_id"], current_task_id);
    assert_eq!(dependency_graph["prerequisites"], json!([]));
    assert_eq!(dependency_graph["transitive_prerequisites"], json!([]));
    assert_eq!(dependency_graph["ready"], true);

    let wait = succeeded_output(
        &executor,
        invocation(
            "wait-current",
            "parent-run",
            WAIT_FOR_TASK_COMPLETION_TOOL,
            json!({}),
        ),
    )
    .await;
    assert_eq!(wait["mode"], "background");

    let cancelled = succeeded_output(
        &executor,
        invocation(
            "cancel-current",
            "parent-run",
            CANCEL_TASK_TOOL,
            json!({
                "task_id": current_task_id,
                "reason": "The user changed the request"
            }),
        ),
    )
    .await;
    assert_eq!(cancelled["status"], "cancelled");
    assert_eq!(cancelled["task"]["id"], current_task_id);
}

#[tokio::test]
async fn task_query_rejects_invalid_paging_before_touching_storage() {
    let runtime = runtime_with_project_conversations().await;
    let executor = LocalTaskToolExecutor::new(runtime, "user-1").expect("executor");
    let error = executor
        .execute_tool(&invocation(
            "bad-list",
            "parent-run",
            LIST_TASKS_TOOL,
            json!({"limit": 501}),
        ))
        .await
        .expect_err("oversized Task page must be rejected");
    assert!(error.contains("invalid task query"));
}

#[tokio::test]
async fn task_tools_require_the_main_chat_conversation_context() {
    let runtime = runtime_with_project_conversations().await;
    let executor = LocalTaskToolExecutor::new(Arc::clone(&runtime), "user-1").expect("executor");
    let run = match runtime
        .try_handle(envelope(
            "get-parent-run",
            HostCommand::GetRun(chatos_local_agent_protocol::GetRunCommand {
                owner_user_id: "user-1".to_string(),
                run_id: "parent-run".to_string(),
            }),
        ))
        .await
        .expect("get parent Run")
    {
        HostResult::Run { run } => run,
        result => panic!("unexpected Run result: {result:?}"),
    };
    assert_eq!(run.profile_key, "main_chat");
    assert_eq!(run.owner_entity_type, "conversation_turn");

    let error = executor
        .execute_tool(&invocation(
            "unknown-tool",
            "parent-run",
            "read_file_raw",
            json!({"path": "README.md"}),
        ))
        .await
        .expect_err("Main Chat must not execute a project tool directly");
    assert!(error.contains("unsupported local Task tool"));
}
