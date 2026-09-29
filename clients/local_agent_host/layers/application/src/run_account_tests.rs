// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use chatos_client_storage::SqliteClientStorage;
use chatos_local_agent_protocol::{
    CancelRunCommand, ClaimNextRunCommand, ClaimNextToolCommand, CommitStepCommand,
    CreateRunCommand, CreateTaskGraphCommand, GetRunCommand, HostCommand, HostRequestEnvelope,
    HostResult, ListEventsCommand, LocalAgentStepOutcome, LocalAgentToolCall, LocalTaskSpec,
    ResumeRunCommand, WaitEventsCommand, LOCAL_AGENT_PROTOCOL_VERSION,
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

fn create(run_id: &str, owner_user_id: &str) -> HostCommand {
    HostCommand::CreateRun(CreateRunCommand {
        run_id: run_id.to_string(),
        owner_user_id: owner_user_id.to_string(),
        owner_entity_type: "test".to_string(),
        owner_entity_id: format!("entity-{run_id}"),
        profile_key: "main_chat".to_string(),
        model_config_ref: "model-1".to_string(),
        model_config_revision: "revision-1".to_string(),
        capability_policy_revision: "policy-1".to_string(),
        input: json!({"message": run_id}),
        max_iterations: 4,
    })
}

fn task_graph(graph_id: &str, task_id: &str, owner_user_id: &str) -> HostCommand {
    HostCommand::CreateTaskGraph(CreateTaskGraphCommand {
        graph_id: graph_id.to_string(),
        owner_user_id: owner_user_id.to_string(),
        source_entity_type: "test".to_string(),
        source_entity_id: format!("source-{graph_id}"),
        tasks: vec![LocalTaskSpec {
            task_id: task_id.to_string(),
            title: format!("Task {task_id}"),
            profile_key: "task_runner".to_string(),
            model_config_ref: "model-1".to_string(),
            model_config_revision: "revision-1".to_string(),
            capability_policy_revision: "policy-1".to_string(),
            input: json!({"task": task_id}),
            max_iterations: 4,
        }],
        dependencies: Vec::new(),
    })
}

#[tokio::test]
async fn run_detail_mutations_and_events_are_owner_scoped() {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let runtime = LocalAgentRuntime::with_clock(storage, Arc::new(|| Ok(20_000)));
    runtime
        .try_handle(request("create-a", create("run-a", "user-1")))
        .await
        .expect("create user-1 Run");
    runtime
        .try_handle(request("create-b", create("run-b", "user-2")))
        .await
        .expect("create user-2 Run");

    assert!(runtime
        .try_handle(request(
            "cross-get",
            HostCommand::GetRun(GetRunCommand {
                owner_user_id: "user-2".to_string(),
                run_id: "run-a".to_string(),
            }),
        ))
        .await
        .is_err());
    assert!(runtime
        .try_handle(request(
            "cross-resume",
            HostCommand::ResumeRun(ResumeRunCommand {
                owner_user_id: "user-2".to_string(),
                run_id: "run-a".to_string(),
                expected_version: 1,
                expected_status: LocalAgentRunStatus::Paused,
                reason: "resume from inspector".to_string(),
                input: json!({}),
            }),
        ))
        .await
        .is_err());
    assert!(runtime
        .try_handle(request(
            "cross-cancel",
            HostCommand::CancelRun(CancelRunCommand {
                owner_user_id: "user-2".to_string(),
                run_id: "run-a".to_string(),
                expected_version: Some(1),
                reason: "cancel from inspector".to_string(),
            }),
        ))
        .await
        .is_err());

    runtime
        .try_handle(request(
            "cancel-a",
            HostCommand::CancelRun(CancelRunCommand {
                owner_user_id: "user-1".to_string(),
                run_id: "run-a".to_string(),
                expected_version: Some(1),
                reason: "cancel from inspector".to_string(),
            }),
        ))
        .await
        .expect("owner cancels Run");

    let user_one_events = runtime
        .try_handle(request(
            "events-a",
            HostCommand::ListEvents(ListEventsCommand {
                owner_user_id: "user-1".to_string(),
                after_cursor: 0,
                limit: 100,
                run_id: None,
            }),
        ))
        .await
        .expect("list user-1 events");
    assert!(matches!(
        user_one_events,
        HostResult::Events { events, .. }
            if events.len() == 2 && events.iter().all(|event| event.run_id == "run-a")
    ));

    let cross_filtered = runtime
        .try_handle(request(
            "wait-cross-filter",
            HostCommand::WaitEvents(WaitEventsCommand {
                owner_user_id: "user-2".to_string(),
                after_cursor: 0,
                limit: 100,
                run_id: Some("run-a".to_string()),
                timeout_ms: 1,
            }),
        ))
        .await
        .expect("wait cross-owner filter");
    assert!(matches!(
        cross_filtered,
        HostResult::Events { events, next_cursor: 0 } if events.is_empty()
    ));
}

#[tokio::test]
async fn model_and_tool_workers_claim_only_their_active_account() {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let runtime = LocalAgentRuntime::with_clock(storage, Arc::new(|| Ok(30_000)));
    runtime
        .try_handle(request("create-a", create("run-a", "user-1")))
        .await
        .expect("create user-1 Run");
    runtime
        .try_handle(request("create-b", create("run-b", "user-2")))
        .await
        .expect("create user-2 Run");

    for (owner_user_id, expected_run_id, call_id) in
        [("user-2", "run-b", "call-b"), ("user-1", "run-a", "call-a")]
    {
        let claimed = runtime
            .try_handle(request(
                &format!("claim-{expected_run_id}"),
                HostCommand::ClaimNextRun(ClaimNextRunCommand {
                    owner_user_id: owner_user_id.to_string(),
                    worker_id: format!("model-{owner_user_id}"),
                    lease_duration_ms: 10_000,
                }),
            ))
            .await
            .expect("claim account Run");
        let HostResult::Claim { claim: Some(claim) } = claimed else {
            panic!("expected account Run claim")
        };
        assert_eq!(claim.run.run_id, expected_run_id);
        runtime
            .try_handle(request(
                &format!("commit-{expected_run_id}"),
                HostCommand::CommitStep(CommitStepCommand {
                    run_id: claim.run.run_id,
                    claim_token: claim.claim_token,
                    expected_version: claim.run.version,
                    outcome: LocalAgentStepOutcome::WaitForTool {
                        batch_id: format!("batch-{expected_run_id}"),
                        tool_calls: vec![LocalAgentToolCall {
                            call_id: call_id.to_string(),
                            tool_name: "read_file".to_string(),
                            arguments: json!({"path": expected_run_id}),
                            side_effecting: false,
                            requires_approval: false,
                        }],
                        checkpoint: json!({}),
                    },
                }),
            ))
            .await
            .expect("commit tool batch");
    }

    for (owner_user_id, expected_run_id) in [("user-2", "run-b"), ("user-1", "run-a")] {
        let claimed = runtime
            .try_handle(request(
                &format!("claim-tool-{expected_run_id}"),
                HostCommand::ClaimNextTool(ClaimNextToolCommand {
                    owner_user_id: owner_user_id.to_string(),
                    worker_id: format!("tool-{owner_user_id}"),
                    lease_duration_ms: 10_000,
                    include_tool_names: None,
                    exclude_tool_names: Vec::new(),
                }),
            ))
            .await
            .expect("claim account tool");
        assert!(matches!(
            claimed,
            HostResult::ToolClaim { claim: Some(claim) }
                if claim.invocation.run_id == expected_run_id
        ));
    }
}

#[tokio::test]
async fn ready_tasks_materialize_only_for_the_active_account() {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let runtime = LocalAgentRuntime::with_clock(storage, Arc::new(|| Ok(40_000)));
    runtime
        .try_handle(request(
            "graph-b",
            task_graph("graph-b", "task-b", "user-2"),
        ))
        .await
        .expect("create user-2 Task");
    runtime
        .try_handle(request(
            "graph-a",
            task_graph("graph-a", "task-a", "user-1"),
        ))
        .await
        .expect("create user-1 Task");

    let user_one = runtime
        .start_next_task_run("user-1")
        .await
        .expect("materialize user-1 Task")
        .expect("user-1 Run");
    assert_eq!(user_one.owner_user_id, "user-1");
    assert_eq!(user_one.owner_entity_id, "task-a");

    let user_two = runtime
        .start_next_task_run("user-2")
        .await
        .expect("materialize user-2 Task")
        .expect("user-2 Run");
    assert_eq!(user_two.owner_user_id, "user-2");
    assert_eq!(user_two.owner_entity_id, "task-b");
}
