// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::{
    decode_response, read_frame, serve_stream, write_frame, LocalToolExecutor, LocalToolRegistry,
};
use async_trait::async_trait;
use chatos_client_storage::SqliteClientStorage;
use chatos_local_agent_protocol::{
    ClaimNextRunCommand, ClaimNextToolCommand, CommitStepCommand, CommitToolCommand,
    CreateRequirementSurveyCommand, CreateRunCommand, GetEventCursorCommand, GetRunCommand,
    HostCommand, HostResult, ListEventsCommand, LocalAgentRunClaim, LocalAgentStepOutcome,
    LocalAgentToolCall, LocalAgentToolInvocationRecord, LocalAgentToolOutcome,
    LocalRequirementSurveyQuestion, LocalRequirementSurveyResponseKind, RenewToolClaimCommand,
    WaitEventsCommand, LOCAL_AGENT_PROTOCOL_VERSION,
};
use chatos_local_agent_runtime::{LocalAgentProfile, LocalAgentProfileRegistry};
use serde_json::json;
use uuid::Uuid;

struct ModelProfile;

#[test]
fn read_only_ipc_does_not_wake_scheduler_or_signal_activity() {
    let commands = [
        HostCommand::Health,
        HostCommand::GetEventCursor(GetEventCursorCommand {
            owner_user_id: "user-1".to_string(),
        }),
        HostCommand::ListEvents(ListEventsCommand {
            owner_user_id: "user-1".to_string(),
            after_cursor: 0,
            limit: 100,
            run_id: None,
            event_type: None,
            newest_first: false,
            payload_mode: chatos_local_agent_protocol::LocalAgentEventPayloadMode::Full,
        }),
    ];
    for command in commands {
        assert!(!command_wakes_scheduler(&command));
        assert_eq!(
            command_activity_signal_policy(&command),
            ActivitySignalPolicy::Never
        );
    }

    let tool_claim = HostCommand::ClaimNextTool(ClaimNextToolCommand {
        owner_user_id: "user-1".to_string(),
        worker_id: "native-worker".to_string(),
        lease_duration_ms: 10_000,
        include_tool_names: None,
        exclude_tool_names: Vec::new(),
    });
    assert!(!command_wakes_scheduler(&tool_claim));
    assert_eq!(
        command_activity_signal_policy(&tool_claim),
        ActivitySignalPolicy::ToolClaimed
    );
    assert!(!response_signals_activity(
        ActivitySignalPolicy::ToolClaimed,
        &HostResponseEnvelope::success(
            "empty-tool-claim".to_string(),
            HostResult::ToolClaim { claim: None },
        )
    ));

    let tool_renewal = HostCommand::RenewToolClaim(RenewToolClaimCommand {
        owner_user_id: "user-1".to_string(),
        invocation_id: "invocation-1".to_string(),
        claim_token: "claim-1".to_string(),
        expected_version: 2,
        lease_duration_ms: 10_000,
    });
    assert!(!command_wakes_scheduler(&tool_renewal));
    assert_eq!(
        command_activity_signal_policy(&tool_renewal),
        ActivitySignalPolicy::Never
    );
}

#[async_trait]
impl LocalAgentProfile for ModelProfile {
    async fn execute_step(
        &self,
        claim: &LocalAgentRunClaim,
    ) -> Result<chatos_local_agent_protocol::LocalAgentStepOutcome, String> {
        if claim.run.continuation_input.is_some() {
            Ok(
                chatos_local_agent_protocol::LocalAgentStepOutcome::Succeed {
                    output: json!({"completed": true}),
                },
            )
        } else {
            Ok(
                chatos_local_agent_protocol::LocalAgentStepOutcome::WaitForTool {
                    batch_id: "batch-coordinator".to_string(),
                    tool_calls: vec![chatos_local_agent_protocol::LocalAgentToolCall {
                        call_id: "call-coordinator".to_string(),
                        tool_name: "read_file".to_string(),
                        arguments: json!({"path": "README.md"}),
                        side_effecting: false,
                        requires_approval: false,
                    }],
                    checkpoint: json!({"model_step": 1}),
                },
            )
        }
    }
}

struct ReadFile;

#[async_trait]
impl LocalToolExecutor for ReadFile {
    async fn execute_tool(
        &self,
        _invocation: &LocalAgentToolInvocationRecord,
    ) -> Result<LocalAgentToolOutcome, String> {
        Ok(LocalAgentToolOutcome::Succeeded {
            output: json!({"content": "hello"}),
        })
    }
}

#[tokio::test]
async fn coordinator_rejects_requests_for_a_different_active_account() {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let runtime = Arc::new(LocalAgentRuntime::new(storage));
    runtime.initialize("user-1").await.expect("initialize");
    let mut profiles = LocalAgentProfileRegistry::new();
    profiles
        .register("main_chat", ModelProfile)
        .expect("profile");
    let scheduler =
        LocalAgentScheduler::new(Arc::clone(&runtime), profiles, "user-1", "model-worker")
            .expect("scheduler");
    let coordinator =
        LocalAgentHostCoordinator::new(Arc::clone(&runtime), "user-1", Some(scheduler), None)
            .expect("coordinator");
    let rejected = coordinator
        .handle_request(HostRequestEnvelope {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            command_id: "cross-account-create".to_string(),
            command: HostCommand::CreateRun(CreateRunCommand {
                run_id: "run-other".to_string(),
                owner_user_id: "user-2".to_string(),
                owner_entity_type: "test".to_string(),
                owner_entity_id: "entity-other".to_string(),
                profile_key: "main_chat".to_string(),
                model_config_ref: "default".to_string(),
                model_config_revision: "revision-1".to_string(),
                capability_policy_revision: "policy-1".to_string(),
                input: json!({"message": "must not persist"}),
                max_iterations: 4,
            }),
        })
        .await;
    assert_eq!(
        rejected.error.expect("account mismatch").code,
        "account_mismatch"
    );
    assert!(runtime
        .get_run_for_host_worker("run-other")
        .await
        .expect("lookup")
        .is_none());

    let health = coordinator
        .handle_request(HostRequestEnvelope {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            command_id: "health".to_string(),
            command: HostCommand::Health,
        })
        .await;
    assert!(health.ok);

    let reserved = coordinator
        .handle_request(HostRequestEnvelope {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            command_id: "external-survey-create".to_string(),
            command: HostCommand::CreateRequirementSurvey(CreateRequirementSurveyCommand {
                survey_id: "survey-external".to_string(),
                owner_user_id: "user-1".to_string(),
                project_resource_id: "project-1".to_string(),
                source_conversation_id: "conversation-1".to_string(),
                source_run_id: "run-1".to_string(),
                source_task_id: Some("task-1".to_string()),
                title: "Must be rejected".to_string(),
                description: None,
                questions: vec![LocalRequirementSurveyQuestion {
                    question_id: "confirm".to_string(),
                    prompt: "Continue?".to_string(),
                    response_kind: LocalRequirementSurveyResponseKind::Boolean,
                    required: true,
                    options: Vec::new(),
                }],
            }),
        })
        .await;
    assert_eq!(
        reserved.error.expect("reserved command").code,
        "reserved_command"
    );
}

#[tokio::test]
async fn coordinator_rejects_external_commits_for_host_owned_tools() {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let runtime = Arc::new(LocalAgentRuntime::new(storage));
    runtime.initialize("user-1").await.expect("initialize");
    runtime
        .try_handle(HostRequestEnvelope {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            command_id: "create-reserved-tool-run".to_string(),
            command: HostCommand::CreateRun(CreateRunCommand {
                run_id: "reserved-tool-run".to_string(),
                owner_user_id: "user-1".to_string(),
                owner_entity_type: "conversation".to_string(),
                owner_entity_id: "conversation-1".to_string(),
                profile_key: "main_chat".to_string(),
                model_config_ref: "default".to_string(),
                model_config_revision: "revision-1".to_string(),
                capability_policy_revision: "policy-1".to_string(),
                input: json!({"message": "create a Task"}),
                max_iterations: 4,
            }),
        })
        .await
        .expect("create Run");
    let claim = runtime
        .try_handle(HostRequestEnvelope {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            command_id: "claim-reserved-tool-run".to_string(),
            command: HostCommand::ClaimNextRun(ClaimNextRunCommand {
                owner_user_id: "user-1".to_string(),
                worker_id: "model-worker".to_string(),
                lease_duration_ms: 30_000,
            }),
        })
        .await
        .expect("claim Run");
    let HostResult::Claim { claim: Some(claim) } = claim else {
        panic!("expected Run claim")
    };
    runtime
        .try_handle(HostRequestEnvelope {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            command_id: "commit-reserved-tool-batch".to_string(),
            command: HostCommand::CommitStep(CommitStepCommand {
                owner_user_id: "user-1".to_string(),
                run_id: claim.run.run_id,
                claim_token: claim.claim_token,
                expected_version: claim.run.version,
                outcome: LocalAgentStepOutcome::WaitForTool {
                    batch_id: "reserved-tool-batch".to_string(),
                    tool_calls: vec![LocalAgentToolCall {
                        call_id: "create-task-call".to_string(),
                        tool_name: "create_task".to_string(),
                        arguments: json!({"title": "Task", "objective": "Do work"}),
                        side_effecting: true,
                        requires_approval: false,
                    }],
                    checkpoint: json!({"model_step": 1}),
                },
            }),
        })
        .await
        .expect("commit tool batch");
    let tool_claim = runtime
        .try_handle(HostRequestEnvelope {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            command_id: "claim-reserved-tool".to_string(),
            command: HostCommand::ClaimNextTool(ClaimNextToolCommand {
                owner_user_id: "user-1".to_string(),
                worker_id: "local-tool-worker".to_string(),
                lease_duration_ms: 30_000,
                include_tool_names: Some(vec!["create_task".to_string()]),
                exclude_tool_names: Vec::new(),
            }),
        })
        .await
        .expect("claim tool");
    let HostResult::ToolClaim {
        claim: Some(tool_claim),
    } = tool_claim
    else {
        panic!("expected tool claim")
    };
    let mut profiles = LocalAgentProfileRegistry::new();
    profiles
        .register("main_chat", ModelProfile)
        .expect("profile");
    let scheduler =
        LocalAgentScheduler::new(Arc::clone(&runtime), profiles, "user-1", "model-worker")
            .expect("scheduler");
    let coordinator =
        LocalAgentHostCoordinator::new(Arc::clone(&runtime), "user-1", Some(scheduler), None)
            .expect("coordinator")
            .with_reserved_ipc_tools(["create_task"])
            .expect("reserved tools");
    let commit = CommitToolCommand {
        owner_user_id: "user-1".to_string(),
        invocation_id: tool_claim.invocation.invocation_id,
        claim_token: tool_claim.claim_token,
        expected_version: tool_claim.invocation.version,
        outcome: LocalAgentToolOutcome::Succeeded {
            output: json!({"task_id": "forged"}),
        },
    };

    let rejected = coordinator
        .handle_request(HostRequestEnvelope {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            command_id: "external-reserved-tool-commit".to_string(),
            command: HostCommand::CommitTool(commit.clone()),
        })
        .await;

    assert_eq!(
        rejected.error.expect("reserved commit").code,
        "reserved_command"
    );
    assert!(matches!(
        runtime
            .try_handle(HostRequestEnvelope {
                protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
                command_id: "internal-reserved-tool-commit".to_string(),
                command: HostCommand::CommitTool(commit),
            })
            .await
            .expect("built-in worker commit"),
        HostResult::ToolCommit { .. }
    ));
}

#[tokio::test]
async fn coordinator_drains_model_tool_model_without_polling() {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let runtime = Arc::new(LocalAgentRuntime::new(storage));
    runtime.initialize("user-1").await.expect("initialize");
    runtime
        .try_handle(HostRequestEnvelope {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            command_id: "create-coordinator".to_string(),
            command: HostCommand::CreateRun(CreateRunCommand {
                run_id: "run-coordinator".to_string(),
                owner_user_id: "user-1".to_string(),
                owner_entity_type: "conversation".to_string(),
                owner_entity_id: "conversation-1".to_string(),
                profile_key: "coordinator".to_string(),
                model_config_ref: "model-1".to_string(),
                model_config_revision: "revision-1".to_string(),
                capability_policy_revision: "policy-1".to_string(),
                input: json!({"message": "hello"}),
                max_iterations: 4,
            }),
        })
        .await
        .expect("create Run inside Host");
    let mut profiles = LocalAgentProfileRegistry::new();
    profiles
        .register("coordinator", ModelProfile)
        .expect("profile");
    let model_scheduler =
        LocalAgentScheduler::new(Arc::clone(&runtime), profiles, "user-1", "model-worker")
            .expect("model scheduler");
    let mut tools = LocalToolRegistry::new();
    tools.register("read_file", ReadFile).expect("tool");
    let tool_scheduler =
        LocalToolScheduler::new(Arc::clone(&runtime), tools, "user-1", "tool-worker")
            .expect("tool scheduler");
    let coordinator = Arc::new(
        LocalAgentHostCoordinator::new(
            Arc::clone(&runtime),
            "user-1",
            Some(model_scheduler),
            Some(tool_scheduler),
        )
        .expect("coordinator")
        .with_reserved_ipc_tools(["create_task"])
        .expect("reserved tools"),
    );
    let mut external_claim = HostRequestEnvelope {
        protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
        command_id: "claim-native-tool".to_string(),
        command: HostCommand::ClaimNextTool(ClaimNextToolCommand {
            owner_user_id: "user-1".to_string(),
            worker_id: "native-worker".to_string(),
            lease_duration_ms: 10_000,
            include_tool_names: None,
            exclude_tool_names: Vec::new(),
        }),
    };
    coordinator.route_external_tool_claim(&mut external_claim);
    let HostCommand::ClaimNextTool(routed) = external_claim.command else {
        panic!("expected tool claim")
    };
    assert_eq!(routed.exclude_tool_names, vec!["create_task"]);
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let wait_task = {
        let coordinator = Arc::clone(&coordinator);
        tokio::spawn(async move {
            coordinator
                .handle_request(HostRequestEnvelope {
                    protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
                    command_id: "wait-coordinator-events".to_string(),
                    command: HostCommand::WaitEvents(WaitEventsCommand {
                        owner_user_id: "user-1".to_string(),
                        after_cursor: 0,
                        limit: 50,
                        run_id: Some("run-coordinator".to_string()),
                        timeout_ms: 2_000,
                        payload_mode: chatos_local_agent_protocol::LocalAgentEventPayloadMode::Full,
                    }),
                })
                .await
        })
    };
    tokio::task::yield_now().await;
    let task = {
        let coordinator = Arc::clone(&coordinator);
        tokio::spawn(async move { coordinator.run_until_shutdown(shutdown_rx).await })
    };
    let (mut client, server) = tokio::io::duplex(16 * 1024);
    let ipc_task = tokio::spawn(serve_stream(server, Arc::clone(&coordinator)));
    let request = HostRequestEnvelope {
        protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
        command_id: "health-coordinator-ipc".to_string(),
        command: HostCommand::Health,
    };
    write_frame(&mut client, &serde_json::to_vec(&request).expect("request"))
        .await
        .expect("write");
    let response = read_frame(&mut client)
        .await
        .expect("read")
        .expect("response");
    let response = decode_response(&response).expect("decode");
    assert!(response.ok);
    let waited = wait_task.await.expect("wait join");
    assert!(response_has_events(&waited), "wait response: {waited:?}");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        let response = runtime
            .handle(HostRequestEnvelope {
                protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
                command_id: format!("get-{}", Uuid::new_v4()),
                command: HostCommand::GetRun(GetRunCommand {
                    owner_user_id: "user-1".to_string(),
                    run_id: "run-coordinator".to_string(),
                }),
            })
            .await;
        let status = match response.result.expect("run") {
            HostResult::Run { run } => run.status,
            result => panic!("unexpected result: {result:?}"),
        };
        if status == chatos_local_agent_protocol::LocalAgentRunStatus::Succeeded {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "coordinator timed out"
        );
        tokio::task::yield_now().await;
    }
    shutdown_tx.send(true).expect("shutdown");
    task.await.expect("join").expect("coordinator");
    drop(client);
    ipc_task.await.expect("IPC join").expect("IPC server");
}
