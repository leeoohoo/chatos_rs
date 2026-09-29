// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use chatos_client_storage::SqliteClientStorage;
use chatos_local_agent_protocol::{
    ClaimNextRunCommand, CommitStepCommand, CreateRunCommand, ResumeRunCommand,
    LOCAL_AGENT_PROTOCOL_VERSION,
};
use serde_json::json;
use std::sync::atomic::{AtomicI64, Ordering};

fn envelope(command_id: &str, command: HostCommand) -> HostRequestEnvelope {
    HostRequestEnvelope {
        protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
        command_id: command_id.to_string(),
        command,
    }
}

#[tokio::test]
async fn retry_preserves_the_next_model_attempt_across_claims() {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let clock = Arc::new(AtomicI64::new(10_000));
    let runtime_clock = Arc::clone(&clock);
    let runtime = LocalAgentRuntime::with_clock(
        storage,
        Arc::new(move || Ok(runtime_clock.load(Ordering::Acquire))),
    );
    runtime.initialize().await.expect("initialize");
    runtime
        .handle(envelope(
            "create",
            HostCommand::CreateRun(CreateRunCommand {
                run_id: "run-retry".to_string(),
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
        .await;
    let claimed = runtime
        .try_handle(envelope(
            "claim-1",
            HostCommand::ClaimNextRun(ClaimNextRunCommand {
                worker_id: "worker-1".to_string(),
                lease_duration_ms: 10_000,
            }),
        ))
        .await
        .expect("claim");
    let HostResult::Claim { claim: Some(claim) } = claimed else {
        panic!("expected claim")
    };
    let waiting = runtime
        .try_handle(envelope(
            "wait-user",
            HostCommand::CommitStep(CommitStepCommand {
                run_id: claim.run.run_id,
                claim_token: claim.claim_token,
                expected_version: claim.run.version,
                outcome: LocalAgentStepOutcome::WaitForUser {
                    prompt: json!({"question": "continue?"}),
                    checkpoint: json!({"response": "checkpoint"}),
                },
            }),
        ))
        .await
        .expect("wait for user");
    let HostResult::Run { run: waiting } = waiting else {
        panic!("expected waiting run")
    };
    let resumed = runtime
        .try_handle(envelope(
            "resume",
            HostCommand::ResumeRun(ResumeRunCommand {
                run_id: waiting.run_id,
                expected_version: waiting.version,
                expected_status: LocalAgentRunStatus::WaitingUser,
                reason: "user replied".to_string(),
                input: json!({"answer": "yes"}),
            }),
        ))
        .await
        .expect("resume");
    let HostResult::Run { run: resumed } = resumed else {
        panic!("expected resumed run")
    };
    let expected_continuation = resumed.continuation_input.expect("continuation");
    let claimed = runtime
        .try_handle(envelope(
            "claim-2",
            HostCommand::ClaimNextRun(ClaimNextRunCommand {
                worker_id: "worker-1".to_string(),
                lease_duration_ms: 10_000,
            }),
        ))
        .await
        .expect("claim continuation");
    let HostResult::Claim { claim: Some(claim) } = claimed else {
        panic!("expected continuation claim")
    };
    let scheduled = runtime
        .try_handle(envelope(
            "retry-1",
            HostCommand::CommitStep(CommitStepCommand {
                run_id: claim.run.run_id,
                claim_token: claim.claim_token,
                expected_version: claim.run.version,
                outcome: LocalAgentStepOutcome::Retry {
                    resume_at_unix_ms: 11_000,
                    next_model_attempt: 2,
                    reason: "provider busy".to_string(),
                },
            }),
        ))
        .await
        .expect("schedule retry");
    let HostResult::Run { run } = scheduled else {
        panic!("expected run")
    };
    assert_eq!(run.model_attempt, 2);
    assert_eq!(run.continuation_input, Some(expected_continuation.clone()));

    clock.store(11_000, Ordering::Release);
    let reclaimed = runtime
        .try_handle(envelope(
            "claim-3",
            HostCommand::ClaimNextRun(ClaimNextRunCommand {
                worker_id: "worker-1".to_string(),
                lease_duration_ms: 10_000,
            }),
        ))
        .await
        .expect("reclaim");
    let HostResult::Claim { claim: Some(claim) } = reclaimed else {
        panic!("expected retry claim")
    };
    assert_eq!(claim.run.model_attempt, 2);
    assert_eq!(claim.run.continuation_input, Some(expected_continuation));
}
