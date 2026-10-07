// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use chatos_client_storage::SqliteClientStorage;
use chatos_local_agent_protocol::{
    CancelConversationTurnCommand, CreateConversationCommand, CreateTaskGraphCommand,
    GetMessageTaskGraphCommand, HostCommand, HostRequestEnvelope, HostResult, LocalTaskDependency,
    LocalTaskSpec, StartConversationTurnCommand, LOCAL_AGENT_PROTOCOL_VERSION,
};
use serde_json::{json, Value};
use std::sync::Arc;

fn request(command_id: &str, command: HostCommand) -> HostRequestEnvelope {
    HostRequestEnvelope {
        protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
        command_id: command_id.to_string(),
        command,
    }
}

fn task(task_id: &str, conversation_id: &str, input: Value) -> LocalTaskSpec {
    let mut input = input.as_object().cloned().unwrap_or_default();
    input.insert(
        "source_conversation_id".to_string(),
        Value::String(conversation_id.to_string()),
    );
    input.insert("prompt".to_string(), Value::String(task_id.to_string()));
    LocalTaskSpec {
        task_id: task_id.to_string(),
        title: task_id.to_string(),
        profile_key: "task_execution".to_string(),
        model_config_ref: "model-1".to_string(),
        model_config_revision: "revision-1".to_string(),
        capability_policy_revision: "policy-1".to_string(),
        input: Value::Object(input),
        max_iterations: 8,
    }
}

async fn create_conversation(runtime: &LocalAgentRuntime, conversation_id: &str) {
    runtime
        .try_handle(request(
            &format!("create-{conversation_id}"),
            HostCommand::CreateConversation(CreateConversationCommand {
                conversation_id: conversation_id.to_string(),
                owner_user_id: "user-1".to_string(),
                title: conversation_id.to_string(),
                resource: None,
            }),
        ))
        .await
        .expect("create conversation");
}

async fn start_turn(
    runtime: &LocalAgentRuntime,
    conversation_id: &str,
    turn_id: &str,
    expected_conversation_version: u64,
) -> chatos_local_agent_protocol::LocalConversationTurnStart {
    let result = runtime
        .try_handle(request(
            &format!("start-{turn_id}"),
            HostCommand::StartConversationTurn(StartConversationTurnCommand {
                owner_user_id: "user-1".to_string(),
                conversation_id: conversation_id.to_string(),
                expected_conversation_version,
                turn_id: turn_id.to_string(),
                message_id: format!("message-{turn_id}"),
                run_id: format!("run-{turn_id}"),
                message: "make a plan".to_string(),
                message_metadata: json!({}),
                attachments: Vec::new(),
                model_config_ref: "model-1".to_string(),
                model_config_revision: "revision-1".to_string(),
                capability_policy_revision: "policy-1".to_string(),
                max_iterations: 8,
            }),
        ))
        .await
        .expect("start turn");
    let HostResult::ConversationTurnStarted { result } = result else {
        panic!("expected turn start");
    };
    *result
}

async fn cancel_turn(
    runtime: &LocalAgentRuntime,
    start: &chatos_local_agent_protocol::LocalConversationTurnStart,
) -> u64 {
    let result = runtime
        .try_handle(request(
            &format!("cancel-{}", start.turn.turn_id),
            HostCommand::CancelConversationTurn(CancelConversationTurnCommand {
                owner_user_id: start.conversation.owner_user_id.clone(),
                conversation_id: start.conversation.conversation_id.clone(),
                expected_conversation_version: start.conversation.version,
                turn_id: start.turn.turn_id.clone(),
                expected_run_version: Some(start.run.version),
                reason: "next message".to_string(),
            }),
        ))
        .await
        .expect("cancel turn");
    let HostResult::ConversationTurnUpdated { result } = result else {
        panic!("expected turn update");
    };
    result.conversation.version
}

async fn create_graph(
    runtime: &LocalAgentRuntime,
    command_id: &str,
    graph: CreateTaskGraphCommand,
) {
    runtime
        .try_handle(request(command_id, HostCommand::CreateTaskGraph(graph)))
        .await
        .expect("create graph");
}

#[tokio::test]
async fn message_graph_restores_server_task_runner_root_and_cross_turn_semantics() {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let runtime = LocalAgentRuntime::new(storage);
    create_conversation(&runtime, "conversation-1").await;
    let old_turn = start_turn(&runtime, "conversation-1", "turn-old", 1).await;
    create_graph(
        &runtime,
        "graph-old",
        CreateTaskGraphCommand {
            graph_id: "graph-old".to_string(),
            owner_user_id: "user-1".to_string(),
            source_entity_type: "conversation_turn".to_string(),
            source_entity_id: "turn-old".to_string(),
            tasks: vec![task(
                "task-old",
                "conversation-1",
                json!({
                    "client_ref": "old",
                    "prerequisite_task_ids": []
                }),
            )],
            dependencies: Vec::new(),
        },
    )
    .await;
    let version = cancel_turn(&runtime, &old_turn).await;
    let _current_turn = start_turn(&runtime, "conversation-1", "turn-current", version).await;

    create_conversation(&runtime, "conversation-2").await;
    let _other_turn = start_turn(&runtime, "conversation-2", "turn-other", 1).await;
    create_graph(
        &runtime,
        "graph-other",
        CreateTaskGraphCommand {
            graph_id: "graph-other".to_string(),
            owner_user_id: "user-1".to_string(),
            source_entity_type: "conversation_turn".to_string(),
            source_entity_id: "turn-other".to_string(),
            tasks: vec![task("task-other", "conversation-2", json!({}))],
            dependencies: Vec::new(),
        },
    )
    .await;

    create_graph(
        &runtime,
        "graph-current",
        CreateTaskGraphCommand {
            graph_id: "graph-current".to_string(),
            owner_user_id: "user-1".to_string(),
            source_entity_type: "conversation_turn".to_string(),
            source_entity_id: "turn-current".to_string(),
            tasks: vec![
                task(
                    "task-a",
                    "conversation-1",
                    json!({
                        "client_ref": "a",
                        "input_payload": {
                            "execution_client_ref": "a",
                            "dependency_context_refs": ["b"]
                        },
                        "prerequisite_task_ids": ["task-old", "task-other"]
                    }),
                ),
                task(
                    "task-b",
                    "conversation-1",
                    json!({
                        "client_ref": "b",
                        "input_payload": {
                            "execution_client_ref": "b",
                            "dependency_context_refs": []
                        },
                        "prerequisite_task_ids": []
                    }),
                ),
            ],
            dependencies: vec![LocalTaskDependency {
                task_id: "task-b".to_string(),
                prerequisite_task_id: "task-a".to_string(),
            }],
        },
    )
    .await;

    let result = runtime
        .try_handle(request(
            "message-graph",
            HostCommand::GetMessageTaskGraph(GetMessageTaskGraphCommand {
                owner_user_id: "user-1".to_string(),
                source_conversation_id: "conversation-1".to_string(),
                source_turn_id: "turn-current".to_string(),
                source_user_message_id: Some("message-turn-current".to_string()),
            }),
        ))
        .await
        .expect("message graph");
    let HostResult::MessageTaskGraph { graph } = result else {
        panic!("expected message Task Graph");
    };
    assert_eq!(graph.root_task_ids, vec!["task-a", "task-b"]);
    assert_eq!(graph.nodes.len(), 3);
    assert!(!graph
        .nodes
        .iter()
        .any(|node| node.task.task_id == "task-other"));
    for task_id in ["task-a", "task-b"] {
        let node = graph
            .nodes
            .iter()
            .find(|node| node.task.task_id == task_id)
            .expect("current node");
        assert_eq!(node.depth, 0);
        assert!(node.is_root && node.is_current_message);
    }
    let old = graph
        .nodes
        .iter()
        .find(|node| node.task.task_id == "task-old")
        .expect("old prerequisite");
    assert_eq!(old.depth, 1);
    assert!(!old.is_root && !old.is_current_message);
    assert!(graph.edges.iter().any(|edge| {
        edge.source_task_id == "task-old"
            && edge.target_task_id == "task-a"
            && edge.kind == "prerequisite"
    }));
    assert!(graph.edges.iter().any(|edge| {
        edge.source_task_id == "task-a"
            && edge.target_task_id == "task-b"
            && edge.kind == "prerequisite"
    }));
    assert!(graph.edges.iter().any(|edge| {
        edge.source_task_id == "task-b" && edge.target_task_id == "task-a" && edge.kind == "context"
    }));
}
