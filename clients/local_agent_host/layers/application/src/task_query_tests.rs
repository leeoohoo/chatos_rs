// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use chatos_client_storage::SqliteClientStorage;
use chatos_local_agent_protocol::{
    CancelTaskCommand, CreateTaskGraphCommand, GetTaskGraphCommand, HostCommand,
    HostRequestEnvelope, HostResult, ListTaskGraphsCommand, LocalTaskGraphListScope,
    LocalTaskGraphStatus, LocalTaskSpec, LOCAL_AGENT_PROTOCOL_VERSION,
};
use serde_json::json;
use std::sync::Arc;

fn request(command_id: &str, command: HostCommand) -> HostRequestEnvelope {
    HostRequestEnvelope {
        protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
        command_id: command_id.to_string(),
        command,
    }
}

fn graph(graph_id: &str, task_id: &str, owner_user_id: &str) -> CreateTaskGraphCommand {
    CreateTaskGraphCommand {
        graph_id: graph_id.to_string(),
        owner_user_id: owner_user_id.to_string(),
        source_entity_type: "conversation".to_string(),
        source_entity_id: format!("conversation-{owner_user_id}"),
        tasks: vec![LocalTaskSpec {
            task_id: task_id.to_string(),
            title: task_id.to_string(),
            profile_key: "task_execution".to_string(),
            model_config_ref: "model-1".to_string(),
            model_config_revision: "revision-1".to_string(),
            capability_policy_revision: "policy-1".to_string(),
            input: json!({"prompt": task_id}),
            max_iterations: 4,
        }],
        dependencies: Vec::new(),
    }
}

fn list(
    owner_user_id: &str,
    scope: LocalTaskGraphListScope,
    before_updated_at_unix_ms: Option<i64>,
    before_graph_id: Option<&str>,
    limit: u32,
) -> HostCommand {
    HostCommand::ListTaskGraphs(ListTaskGraphsCommand {
        owner_user_id: owner_user_id.to_string(),
        scope,
        source_entity_type: None,
        source_entity_id: None,
        before_updated_at_unix_ms,
        before_graph_id: before_graph_id.map(str::to_string),
        limit,
    })
}

#[tokio::test]
async fn lists_owner_task_graphs_with_scope_and_stable_cursor() {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let runtime = LocalAgentRuntime::with_clock(storage, Arc::new(|| Ok(10_000)));
    for (command_id, spec) in [
        ("create-a", graph("graph-a", "task-a", "user-1")),
        ("create-b", graph("graph-b", "task-b", "user-1")),
        ("create-z", graph("graph-z", "task-z", "user-2")),
    ] {
        runtime
            .try_handle(request(command_id, HostCommand::CreateTaskGraph(spec)))
            .await
            .expect("create graph");
    }
    runtime
        .try_handle(request(
            "cancel-b",
            HostCommand::CancelTask(CancelTaskCommand {
                owner_user_id: "user-1".to_string(),
                task_id: "task-b".to_string(),
                expected_version: Some(1),
                reason: "not needed".to_string(),
                replacement_task_ids: Vec::new(),
            }),
        ))
        .await
        .expect("cancel graph-b");

    let first = runtime
        .try_handle(request(
            "list-first",
            list("user-1", LocalTaskGraphListScope::All, None, None, 1),
        ))
        .await
        .expect("first page");
    let HostResult::TaskGraphs { page } = first else {
        panic!("expected Task Graph page")
    };
    assert_eq!(page.graphs.len(), 1);
    assert_eq!(page.graphs[0].graph_id, "graph-b");
    assert_eq!(page.graphs[0].status, LocalTaskGraphStatus::Cancelled);
    assert_eq!(page.graphs[0].task_count, 1);
    let cursor_time = page.next_before_updated_at_unix_ms.expect("next timestamp");
    let cursor_id = page.next_before_graph_id.expect("next graph id");

    let second = runtime
        .try_handle(request(
            "list-second",
            list(
                "user-1",
                LocalTaskGraphListScope::All,
                Some(cursor_time),
                Some(&cursor_id),
                1,
            ),
        ))
        .await
        .expect("second page");
    assert!(matches!(
        second,
        HostResult::TaskGraphs { page }
            if page.graphs.len() == 1
                && page.graphs[0].graph_id == "graph-a"
                && page.next_before_graph_id.is_none()
    ));

    let active = runtime
        .try_handle(request(
            "list-active",
            list("user-1", LocalTaskGraphListScope::Active, None, None, 10),
        ))
        .await
        .expect("active graphs");
    assert!(matches!(
        active,
        HostResult::TaskGraphs { page }
            if page.graphs.len() == 1 && page.graphs[0].graph_id == "graph-a"
    ));
    let terminal = runtime
        .try_handle(request(
            "list-terminal",
            list("user-1", LocalTaskGraphListScope::Terminal, None, None, 10),
        ))
        .await
        .expect("terminal graphs");
    assert!(matches!(
        terminal,
        HostResult::TaskGraphs { page }
            if page.graphs.len() == 1 && page.graphs[0].graph_id == "graph-b"
    ));

    let other = runtime
        .try_handle(request(
            "list-other",
            list("user-2", LocalTaskGraphListScope::All, None, None, 10),
        ))
        .await
        .expect("other account");
    assert!(matches!(
        other,
        HostResult::TaskGraphs { page }
            if page.graphs.len() == 1 && page.graphs[0].graph_id == "graph-z"
    ));
    assert!(runtime
        .try_handle(request(
            "cross-account-read",
            HostCommand::GetTaskGraph(GetTaskGraphCommand {
                owner_user_id: "user-2".to_string(),
                graph_id: "graph-a".to_string(),
            }),
        ))
        .await
        .is_err());
    assert!(runtime
        .try_handle(request(
            "cross-account-cancel",
            HostCommand::CancelTask(CancelTaskCommand {
                owner_user_id: "user-2".to_string(),
                task_id: "task-a".to_string(),
                expected_version: Some(1),
                reason: "wrong account".to_string(),
                replacement_task_ids: Vec::new(),
            }),
        ))
        .await
        .is_err());
}

#[tokio::test]
async fn filters_task_graphs_by_source_before_pagination() {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let runtime = LocalAgentRuntime::with_clock(storage, Arc::new(|| Ok(10_000)));
    for (command_id, graph_id, turn_id) in [
        ("create-turn-a", "graph-turn-a", "turn-a"),
        ("create-turn-b", "graph-turn-b", "turn-b"),
    ] {
        let mut spec = graph(graph_id, &format!("task-{turn_id}"), "user-1");
        spec.source_entity_type = "conversation_turn".to_string();
        spec.source_entity_id = turn_id.to_string();
        runtime
            .try_handle(request(command_id, HostCommand::CreateTaskGraph(spec)))
            .await
            .expect("create source graph");
    }

    let filtered = runtime
        .try_handle(request(
            "list-turn-a",
            HostCommand::ListTaskGraphs(ListTaskGraphsCommand {
                owner_user_id: "user-1".to_string(),
                scope: LocalTaskGraphListScope::All,
                source_entity_type: Some("conversation_turn".to_string()),
                source_entity_id: Some("turn-a".to_string()),
                before_updated_at_unix_ms: None,
                before_graph_id: None,
                limit: 1,
            }),
        ))
        .await
        .expect("filtered graphs");
    assert!(matches!(
        filtered,
        HostResult::TaskGraphs { page }
            if page.graphs.len() == 1
                && page.graphs[0].graph_id == "graph-turn-a"
                && page.next_before_graph_id.is_none()
    ));
}
