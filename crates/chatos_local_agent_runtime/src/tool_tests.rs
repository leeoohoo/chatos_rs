// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use chatos_client_storage::SqliteClientStorage;
use chatos_local_agent_protocol::{
    ClaimNextRunCommand, ClaimNextToolCommand, CommitStepCommand, CreateRunCommand, HostCommand,
    HostRequestEnvelope, HostResult, LocalAgentRunClaim, LocalAgentToolCall, LocalAgentToolClaim,
    LOCAL_AGENT_PROTOCOL_VERSION,
};
use serde_json::json;
use std::sync::atomic::AtomicI64;

fn envelope(command_id: &str, command: HostCommand) -> HostRequestEnvelope {
    HostRequestEnvelope {
        protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
        command_id: command_id.to_string(),
        command,
    }
}

async fn prepare_claimed_tool(
    side_effecting: bool,
) -> (LocalAgentRuntime, Arc<AtomicI64>, LocalAgentToolClaim) {
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
                run_id: "run-tool-recovery".to_string(),
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
    let run_claim = runtime
        .handle(envelope(
            "claim-run",
            HostCommand::ClaimNextRun(ClaimNextRunCommand {
                worker_id: "model-worker".to_string(),
                lease_duration_ms: 10_000,
            }),
        ))
        .await;
    let run_claim: LocalAgentRunClaim = match run_claim.result.expect("run claim") {
        HostResult::Claim { claim: Some(claim) } => claim,
        result => panic!("unexpected result: {result:?}"),
    };
    runtime
        .handle(envelope(
            "commit-run",
            HostCommand::CommitStep(CommitStepCommand {
                run_id: run_claim.run.run_id,
                claim_token: run_claim.claim_token,
                expected_version: run_claim.run.version,
                outcome: LocalAgentStepOutcome::WaitForTool {
                    batch_id: "batch-1".to_string(),
                    tool_calls: vec![LocalAgentToolCall {
                        call_id: "call-1".to_string(),
                        tool_name: if side_effecting {
                            "write_file".to_string()
                        } else {
                            "read_file".to_string()
                        },
                        arguments: json!({"path": "README.md"}),
                        side_effecting,
                    }],
                },
            }),
        ))
        .await;
    let tool_claim = runtime
        .handle(envelope(
            "claim-tool",
            HostCommand::ClaimNextTool(ClaimNextToolCommand {
                worker_id: "tool-worker".to_string(),
                lease_duration_ms: 1_000,
            }),
        ))
        .await;
    let tool_claim = match tool_claim.result.expect("tool claim") {
        HostResult::ToolClaim { claim: Some(claim) } => claim,
        result => panic!("unexpected result: {result:?}"),
    };
    (runtime, clock, tool_claim)
}

#[tokio::test]
async fn expired_side_effecting_tool_requires_review() {
    let (runtime, clock, _) = prepare_claimed_tool(true).await;
    clock.store(11_001, Ordering::Release);
    assert_eq!(runtime.initialize().await.expect("recover"), 1);
    let response = runtime
        .handle(envelope(
            "get-run",
            HostCommand::GetRun {
                run_id: "run-tool-recovery".to_string(),
            },
        ))
        .await;
    let run = match response.result.expect("run") {
        HostResult::Run { run } => run,
        result => panic!("unexpected result: {result:?}"),
    };
    assert_eq!(run.status, LocalAgentRunStatus::NeedsReview);
}

#[tokio::test]
async fn expired_read_only_tool_is_requeued() {
    let (runtime, clock, first_claim) = prepare_claimed_tool(false).await;
    clock.store(11_001, Ordering::Release);
    assert_eq!(runtime.initialize().await.expect("recover"), 1);
    let response = runtime
        .handle(envelope(
            "claim-tool-again",
            HostCommand::ClaimNextTool(ClaimNextToolCommand {
                worker_id: "tool-worker-2".to_string(),
                lease_duration_ms: 1_000,
            }),
        ))
        .await;
    let second_claim = match response.result.expect("tool claim") {
        HostResult::ToolClaim { claim: Some(claim) } => claim,
        result => panic!("unexpected result: {result:?}"),
    };
    assert_eq!(
        second_claim.invocation.invocation_id,
        first_claim.invocation.invocation_id
    );
    assert!(second_claim.invocation.version > first_claim.invocation.version);
}
