// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use chatos_client_storage::SqliteClientStorage;
use chatos_local_agent_protocol::{
    ClaimNextRunCommand, ClaimNextToolCommand, CommitStepCommand, CommitToolCommand,
    CreateRunCommand, CreateTaskGraphCommand, GetTaskGraphCommand, LocalAgentToolCall,
    LocalTaskSpec, LocalTaskStatus, LOCAL_AGENT_PROTOCOL_VERSION,
};
use serde_json::Value;

fn envelope(command_id: &str, command: HostCommand) -> HostRequestEnvelope {
    HostRequestEnvelope {
        protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
        command_id: command_id.to_string(),
        command,
    }
}

fn create_command() -> HostCommand {
    HostCommand::CreateRun(CreateRunCommand {
        run_id: "run-1".to_string(),
        owner_user_id: "user-1".to_string(),
        owner_entity_type: "conversation".to_string(),
        owner_entity_id: "conversation-1".to_string(),
        profile_key: "main_chat".to_string(),
        model_config_ref: "model-1".to_string(),
        model_config_revision: "revision-1".to_string(),
        capability_policy_revision: "policy-1".to_string(),
        input: json!({"message": "hello"}),
        max_iterations: 4,
    })
}

#[tokio::test]
async fn runtime_drives_one_durable_step_and_replays_command() {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let runtime = LocalAgentRuntime::with_clock(storage, Arc::new(|| Ok(10_000)));
    runtime.initialize("user-1").await.expect("initialize");

    let created = runtime.handle(envelope("create-1", create_command())).await;
    assert!(created.ok);
    let replay = runtime.handle(envelope("create-1", create_command())).await;
    assert_eq!(created, replay);

    let claimed = runtime
        .handle(envelope(
            "claim-1",
            HostCommand::ClaimNextRun(ClaimNextRunCommand {
                owner_user_id: "user-1".to_string(),
                worker_id: "worker-1".to_string(),
                lease_duration_ms: 10_000,
            }),
        ))
        .await;
    let claim = match claimed.result.expect("claim result") {
        HostResult::Claim { claim: Some(claim) } => claim,
        result => panic!("unexpected result: {result:?}"),
    };
    let completed = runtime
        .handle(envelope(
            "commit-1",
            HostCommand::CommitStep(CommitStepCommand {
                owner_user_id: "user-1".to_string(),
                run_id: claim.run.run_id.clone(),
                claim_token: claim.claim_token.clone(),
                expected_version: claim.run.version,
                outcome: LocalAgentStepOutcome::Succeed {
                    output: json!({"answer": 42}),
                },
            }),
        ))
        .await;
    let completed_replay = runtime
        .handle(envelope(
            "commit-1",
            HostCommand::CommitStep(CommitStepCommand {
                owner_user_id: "user-1".to_string(),
                run_id: claim.run.run_id.clone(),
                claim_token: claim.claim_token,
                expected_version: claim.run.version,
                outcome: LocalAgentStepOutcome::Succeed {
                    output: json!({"answer": 42}),
                },
            }),
        ))
        .await;
    assert_eq!(completed, completed_replay);
    let run = match completed.result.expect("commit result") {
        HostResult::Run { run } => run,
        result => panic!("unexpected result: {result:?}"),
    };
    assert_eq!(run.status, LocalAgentRunStatus::Succeeded);
    assert_eq!(run.terminal_outcome, Some(json!({"answer": 42})));
    assert_eq!(run.version, 3);
}

#[tokio::test]
async fn iteration_limit_becomes_review_instead_of_success() {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let runtime = LocalAgentRuntime::with_clock(storage, Arc::new(|| Ok(10_000)));
    runtime.initialize("user-1").await.expect("initialize");
    let mut command = match create_command() {
        HostCommand::CreateRun(command) => command,
        _ => unreachable!(),
    };
    command.max_iterations = 1;
    runtime
        .handle(envelope("create-1", HostCommand::CreateRun(command)))
        .await;
    let claimed = runtime
        .handle(envelope(
            "claim-1",
            HostCommand::ClaimNextRun(ClaimNextRunCommand {
                owner_user_id: "user-1".to_string(),
                worker_id: "worker-1".to_string(),
                lease_duration_ms: 10_000,
            }),
        ))
        .await;
    let claim = match claimed.result.expect("claim result") {
        HostResult::Claim { claim: Some(claim) } => claim,
        result => panic!("unexpected result: {result:?}"),
    };
    let continued = runtime
        .handle(envelope(
            "commit-1",
            HostCommand::CommitStep(CommitStepCommand {
                owner_user_id: "user-1".to_string(),
                run_id: claim.run.run_id.clone(),
                claim_token: claim.claim_token,
                expected_version: claim.run.version,
                outcome: LocalAgentStepOutcome::Continue {
                    checkpoint: Value::Null,
                },
            }),
        ))
        .await;
    let run = match continued.result.expect("commit result") {
        HostResult::Run { run } => run,
        result => panic!("unexpected result: {result:?}"),
    };
    assert_eq!(run.status, LocalAgentRunStatus::NeedsReview);
}

#[tokio::test]
async fn task_execution_cannot_finish_before_reporting_its_outcome() {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let runtime = LocalAgentRuntime::with_clock(storage, Arc::new(|| Ok(10_000)));
    runtime.initialize("user-1").await.expect("initialize");
    let mut command = match create_command() {
        HostCommand::CreateRun(command) => command,
        _ => unreachable!(),
    };
    command.owner_entity_type = "task".to_string();
    command.owner_entity_id = "task-1".to_string();
    command.profile_key = "task_execution".to_string();
    command.input = json!({"prompt": "do the work"});
    runtime
        .handle(envelope("create-task-run", HostCommand::CreateRun(command)))
        .await;
    let claimed = runtime
        .handle(envelope(
            "claim-task-run",
            HostCommand::ClaimNextRun(ClaimNextRunCommand {
                owner_user_id: "user-1".to_string(),
                worker_id: "worker-1".to_string(),
                lease_duration_ms: 10_000,
            }),
        ))
        .await;
    let claim = match claimed.result.expect("claim result") {
        HostResult::Claim { claim: Some(claim) } => claim,
        result => panic!("unexpected result: {result:?}"),
    };

    let rejected = runtime
        .handle(envelope(
            "commit-task-run",
            HostCommand::CommitStep(CommitStepCommand {
                owner_user_id: "user-1".to_string(),
                run_id: claim.run.run_id,
                claim_token: claim.claim_token,
                expected_version: claim.run.version,
                outcome: LocalAgentStepOutcome::Succeed {
                    output: json!({"content": "done"}),
                },
            }),
        ))
        .await;

    assert!(!rejected.ok);
    assert!(rejected
        .error
        .as_ref()
        .is_some_and(|error| error.message.contains("must report its outcome")));
}

#[tokio::test]
async fn a_normally_completed_run_owns_success_independently_of_internal_result() {
    for result_status in ["succeeded", "failed", "blocked"] {
        let storage = Arc::new(SqliteClientStorage::connect_memory().await.unwrap());
        let runtime = LocalAgentRuntime::with_clock(storage, Arc::new(|| Ok(10_000)));
        runtime.initialize("user-1").await.unwrap();
        runtime.try_handle(envelope("create", HostCommand::CreateTaskGraph(CreateTaskGraphCommand {
            graph_id:"graph-1".into(),owner_user_id:"user-1".into(),source_entity_type:"test".into(),source_entity_id:"test-1".into(),
            tasks:vec![LocalTaskSpec { task_id:"task-1".into(),title:"Review".into(),profile_key:"task_execution".into(),model_config_ref:"model-1".into(),model_config_revision:"revision-1".into(),capability_policy_revision:"policy-1".into(),input:json!({"prompt":"Review","acceptance_report_version":1,"acceptance_criteria":["Review completed"],"final_response_max_chars":80}),max_iterations:8 }],dependencies:vec![]
        }))).await.unwrap();
        runtime
            .start_next_task_run("user-1")
            .await
            .unwrap()
            .unwrap();
        let HostResult::Claim { claim: Some(claim) } = runtime
            .try_handle(envelope(
                "claim",
                HostCommand::ClaimNextRun(ClaimNextRunCommand {
                    owner_user_id: "user-1".into(),
                    worker_id: "worker".into(),
                    lease_duration_ms: 10000,
                }),
            ))
            .await
            .unwrap()
        else {
            panic!("claim")
        };
        runtime
            .try_handle(envelope(
                "wait",
                HostCommand::CommitStep(CommitStepCommand {
                    owner_user_id: "user-1".into(),
                    run_id: claim.run.run_id.clone(),
                    claim_token: claim.claim_token,
                    expected_version: claim.run.version,
                    outcome: LocalAgentStepOutcome::WaitForTool {
                        batch_id: "batch".into(),
                        tool_calls: vec![LocalAgentToolCall {
                            call_id: "call".into(),
                            tool_name: "task_run_process_report_outcome".into(),
                            arguments: json!({"result_status":result_status,"reason":"Review result"}),
                            side_effecting: false,
                            requires_approval: false,
                        }],
                        checkpoint: json!({}),
                    },
                }),
            ))
            .await
            .unwrap();
        let HostResult::ToolClaim { claim: Some(tool) } = runtime
            .try_handle(envelope(
                "tool",
                HostCommand::ClaimNextTool(ClaimNextToolCommand {
                    owner_user_id: "user-1".into(),
                    worker_id: "tools".into(),
                    lease_duration_ms: 10000,
                    include_tool_names: None,
                    exclude_tool_names: vec![],
                    execution_route: None,
                }),
            ))
            .await
            .unwrap()
        else {
            panic!("tool claim")
        };
        let report = json!({"reported":true,"result_status":result_status,"status":result_status,"reason":"Review result","final_response":"Review complete: the reviewed artifact has defects.","acceptance_checks":[{"criterion_index":0,"status":if result_status=="succeeded" {"failed"} else {"not_verified"},"evidence":"Compared the requested review criteria"}]});
        runtime
            .try_handle(envelope(
                "report",
                HostCommand::CommitTool(CommitToolCommand {
                    owner_user_id: "user-1".into(),
                    invocation_id: tool.invocation.invocation_id,
                    claim_token: tool.claim_token,
                    expected_version: tool.invocation.version,
                    outcome: LocalAgentToolOutcome::Succeeded {
                        output: report.clone(),
                    },
                }),
            ))
            .await
            .unwrap();
        let HostResult::Claim { claim: Some(claim) } = runtime
            .try_handle(envelope(
                "final-claim",
                HostCommand::ClaimNextRun(ClaimNextRunCommand {
                    owner_user_id: "user-1".into(),
                    worker_id: "worker".into(),
                    lease_duration_ms: 10000,
                }),
            ))
            .await
            .unwrap()
        else {
            panic!("final claim")
        };
        let finish = |content: Value| {
            HostCommand::CommitStep(CommitStepCommand {
                owner_user_id: "user-1".into(),
                run_id: claim.run.run_id.clone(),
                claim_token: claim.claim_token.clone(),
                expected_version: claim.run.version,
                outcome: LocalAgentStepOutcome::Succeed {
                    output: json!({"content":content}),
                },
            })
        };
        assert!(runtime
            .try_handle(envelope("rewrite", finish(json!("Unchecked rewrite"))))
            .await
            .is_err());
        runtime
            .try_handle(envelope("finish", finish(report["final_response"].clone())))
            .await
            .unwrap();
        let HostResult::TaskGraph { graph } = runtime
            .try_handle(envelope(
                "graph",
                HostCommand::GetTaskGraph(GetTaskGraphCommand {
                    owner_user_id: "user-1".into(),
                    graph_id: "graph-1".into(),
                }),
            ))
            .await
            .unwrap()
        else {
            panic!("graph")
        };
        assert_eq!(graph.tasks[0].status, LocalTaskStatus::Succeeded);
    }
}
