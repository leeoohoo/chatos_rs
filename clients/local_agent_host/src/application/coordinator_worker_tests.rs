// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{LocalAgentHostCoordinator, LocalAgentScheduler};
use crate::HostRequestHandler;
use async_trait::async_trait;
use chatos_client_storage::SqliteClientStorage;
use chatos_local_agent_protocol::{
    ClaimNextRunCommand, CommitStepCommand, CreateRunCommand, CreateTaskGraphCommand, HostCommand,
    HostRequestEnvelope, HostResult, LocalAgentRunClaim, LocalAgentStepOutcome, LocalTaskSpec,
    LOCAL_AGENT_PROTOCOL_VERSION,
};
use chatos_local_agent_runtime::{LocalAgentProfile, LocalAgentProfileRegistry, LocalAgentRuntime};
use serde_json::json;
use std::sync::Arc;

struct UnusedProfile;

#[async_trait]
impl LocalAgentProfile for UnusedProfile {
    async fn execute_step(
        &self,
        _claim: &LocalAgentRunClaim,
    ) -> Result<LocalAgentStepOutcome, String> {
        unreachable!("the coordinator loop is not started in this test")
    }
}

fn envelope(command_id: &str, command: HostCommand) -> HostRequestEnvelope {
    HostRequestEnvelope {
        protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
        command_id: command_id.to_string(),
        command,
    }
}

#[tokio::test]
async fn coordinator_keeps_model_claims_and_commits_inside_the_host() {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let runtime = Arc::new(LocalAgentRuntime::new(storage));
    runtime.initialize("user-1").await.expect("initialize");
    runtime
        .try_handle(envelope(
            "create-model-run",
            HostCommand::CreateRun(CreateRunCommand {
                run_id: "model-run".to_string(),
                owner_user_id: "user-1".to_string(),
                owner_entity_type: "conversation".to_string(),
                owner_entity_id: "conversation-1".to_string(),
                profile_key: "main_chat".to_string(),
                model_config_ref: "model-1".to_string(),
                model_config_revision: "revision-1".to_string(),
                capability_policy_revision: "policy-1".to_string(),
                input: json!({"message": "hello"}),
                max_iterations: 4,
            }),
        ))
        .await
        .expect("create Run");
    let mut profiles = LocalAgentProfileRegistry::new();
    profiles
        .register("main_chat", UnusedProfile)
        .expect("profile");
    let scheduler = LocalAgentScheduler::new(
        Arc::clone(&runtime),
        profiles,
        "user-1",
        "local-model-worker",
    )
    .expect("scheduler");
    let coordinator =
        LocalAgentHostCoordinator::new(Arc::clone(&runtime), "user-1", Some(scheduler), None)
            .expect("coordinator");
    let rejected_run_creation = coordinator
        .handle_request(envelope(
            "external-run-create",
            HostCommand::CreateRun(CreateRunCommand {
                run_id: "forged-run".to_string(),
                owner_user_id: "user-1".to_string(),
                owner_entity_type: "conversation".to_string(),
                owner_entity_id: "conversation-1".to_string(),
                profile_key: "main_chat".to_string(),
                model_config_ref: "model-1".to_string(),
                model_config_revision: "revision-1".to_string(),
                capability_policy_revision: "policy-1".to_string(),
                input: json!({"message": "forged"}),
                max_iterations: 4,
            }),
        ))
        .await;
    let rejected_graph_creation = coordinator
        .handle_request(envelope(
            "external-graph-create",
            HostCommand::CreateTaskGraph(CreateTaskGraphCommand {
                graph_id: "forged-graph".to_string(),
                owner_user_id: "user-1".to_string(),
                source_entity_type: "conversation".to_string(),
                source_entity_id: "conversation-1".to_string(),
                tasks: vec![LocalTaskSpec {
                    task_id: "forged-task".to_string(),
                    title: "Forged".to_string(),
                    profile_key: "task_execution".to_string(),
                    model_config_ref: "model-1".to_string(),
                    model_config_revision: "revision-1".to_string(),
                    capability_policy_revision: "policy-1".to_string(),
                    input: json!({"prompt": "forged"}),
                    max_iterations: 4,
                }],
                dependencies: Vec::new(),
            }),
        ))
        .await;
    let claim_command = ClaimNextRunCommand {
        owner_user_id: "user-1".to_string(),
        worker_id: "external-model-worker".to_string(),
        lease_duration_ms: 30_000,
    };

    let rejected_claim = coordinator
        .handle_request(envelope(
            "external-model-claim",
            HostCommand::ClaimNextRun(claim_command.clone()),
        ))
        .await;

    assert_eq!(
        rejected_run_creation
            .error
            .expect("reserved Run creation")
            .code,
        "reserved_command"
    );
    assert_eq!(
        rejected_graph_creation
            .error
            .expect("reserved Task Graph creation")
            .code,
        "reserved_command"
    );
    assert_eq!(
        rejected_claim.error.expect("reserved claim").code,
        "reserved_command"
    );
    let internal_claim = runtime
        .try_handle(envelope(
            "internal-model-claim",
            HostCommand::ClaimNextRun(ClaimNextRunCommand {
                worker_id: "local-model-worker".to_string(),
                ..claim_command
            }),
        ))
        .await
        .expect("internal claim");
    let HostResult::Claim {
        claim: Some(internal_claim),
    } = internal_claim
    else {
        panic!("expected internal Run claim")
    };
    let commit = CommitStepCommand {
        owner_user_id: "user-1".to_string(),
        run_id: internal_claim.run.run_id,
        claim_token: internal_claim.claim_token,
        expected_version: internal_claim.run.version,
        outcome: LocalAgentStepOutcome::Succeed {
            output: json!({"answer": "done"}),
        },
    };

    let rejected_commit = coordinator
        .handle_request(envelope(
            "external-model-commit",
            HostCommand::CommitStep(commit.clone()),
        ))
        .await;

    assert_eq!(
        rejected_commit.error.expect("reserved commit").code,
        "reserved_command"
    );
    assert!(matches!(
        runtime
            .try_handle(envelope(
                "internal-model-commit",
                HostCommand::CommitStep(commit),
            ))
            .await
            .expect("internal commit"),
        HostResult::Run { .. }
    ));
}
