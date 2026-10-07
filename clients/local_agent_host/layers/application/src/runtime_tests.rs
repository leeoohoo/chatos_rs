// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use chatos_client_storage::SqliteClientStorage;
use chatos_local_agent_protocol::{
    ClaimNextRunCommand, CommitStepCommand, CreateRunCommand, LOCAL_AGENT_PROTOCOL_VERSION,
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
