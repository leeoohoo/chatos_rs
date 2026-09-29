// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use chatos_client_storage::SqliteClientStorage;
use chatos_local_agent_protocol::{
    CancelRunCommand, ClaimNextRunCommand, ClaimNextToolCommand, CommitStepCommand,
    CommitToolCommand, CreateRunCommand, HostCommand, HostRequestEnvelope, HostResult,
    LocalAgentRunClaim, LocalAgentToolCall, LocalAgentToolClaim, LocalAgentToolOutcome,
    ResumeRunCommand, LOCAL_AGENT_PROTOCOL_VERSION,
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
                        requires_approval: false,
                    }],
                    checkpoint: json!({"response_id": "response-1"}),
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
                include_tool_names: None,
                exclude_tool_names: Vec::new(),
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
                include_tool_names: None,
                exclude_tool_names: Vec::new(),
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

#[tokio::test]
async fn cancelling_run_invalidates_outstanding_tool_claim() {
    let (runtime, _, tool_claim) = prepare_claimed_tool(false).await;
    let cancelled = runtime
        .handle(envelope(
            "cancel-run-with-tool",
            HostCommand::CancelRun(CancelRunCommand {
                run_id: "run-tool-recovery".to_string(),
                expected_version: None,
                reason: "task was restarted".to_string(),
            }),
        ))
        .await;
    let run = match cancelled.result.expect("cancelled Run") {
        HostResult::Run { run } => run,
        result => panic!("unexpected result: {result:?}"),
    };
    assert_eq!(run.status, LocalAgentRunStatus::Cancelled);

    let late_commit = runtime
        .handle(envelope(
            "late-tool-commit",
            HostCommand::CommitTool(CommitToolCommand {
                invocation_id: tool_claim.invocation.invocation_id,
                claim_token: tool_claim.claim_token,
                expected_version: tool_claim.invocation.version,
                outcome: LocalAgentToolOutcome::Succeeded {
                    output: json!({"content": "late"}),
                },
            }),
        ))
        .await;
    assert!(!late_commit.ok);
    assert_eq!(late_commit.error.expect("conflict").code, "conflict");
}

#[tokio::test]
async fn tool_claim_filters_partition_reserved_and_platform_tools() {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let runtime = LocalAgentRuntime::with_clock(storage, Arc::new(|| Ok(10_000)));
    runtime.initialize().await.expect("initialize");
    runtime
        .handle(envelope(
            "create-filtered-run",
            HostCommand::CreateRun(CreateRunCommand {
                run_id: "run-filtered-tools".to_string(),
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
    let claim = runtime
        .handle(envelope(
            "claim-filtered-run",
            HostCommand::ClaimNextRun(ClaimNextRunCommand {
                worker_id: "model-worker".to_string(),
                lease_duration_ms: 10_000,
            }),
        ))
        .await;
    let claim = match claim.result.expect("run claim") {
        HostResult::Claim { claim: Some(claim) } => claim,
        result => panic!("unexpected result: {result:?}"),
    };
    runtime
        .handle(envelope(
            "commit-filtered-run",
            HostCommand::CommitStep(CommitStepCommand {
                run_id: claim.run.run_id,
                claim_token: claim.claim_token,
                expected_version: claim.run.version,
                outcome: LocalAgentStepOutcome::WaitForTool {
                    batch_id: "batch-filtered".to_string(),
                    tool_calls: vec![
                        LocalAgentToolCall {
                            call_id: "call-platform".to_string(),
                            tool_name: "read_file".to_string(),
                            arguments: json!({}),
                            side_effecting: false,
                            requires_approval: false,
                        },
                        LocalAgentToolCall {
                            call_id: "call-reserved".to_string(),
                            tool_name: "create_task".to_string(),
                            arguments: json!({}),
                            side_effecting: true,
                            requires_approval: false,
                        },
                    ],
                    checkpoint: json!({}),
                },
            }),
        ))
        .await;

    let reserved = runtime
        .handle(envelope(
            "claim-reserved-tool",
            HostCommand::ClaimNextTool(ClaimNextToolCommand {
                worker_id: "rust-worker".to_string(),
                lease_duration_ms: 10_000,
                include_tool_names: Some(vec!["create_task".to_string()]),
                exclude_tool_names: Vec::new(),
            }),
        ))
        .await;
    let reserved = match reserved.result.expect("reserved claim") {
        HostResult::ToolClaim { claim: Some(claim) } => claim,
        result => panic!("unexpected result: {result:?}"),
    };
    assert_eq!(reserved.invocation.tool_name, "create_task");

    let platform = runtime
        .handle(envelope(
            "claim-platform-tool",
            HostCommand::ClaimNextTool(ClaimNextToolCommand {
                worker_id: "native-worker".to_string(),
                lease_duration_ms: 10_000,
                include_tool_names: None,
                exclude_tool_names: vec!["create_task".to_string()],
            }),
        ))
        .await;
    let platform = match platform.result.expect("platform claim") {
        HostResult::ToolClaim { claim: Some(claim) } => claim,
        result => panic!("unexpected result: {result:?}"),
    };
    assert_eq!(platform.invocation.tool_name, "read_file");
}

#[tokio::test]
async fn waiting_user_resume_preserves_checkpoint_and_supplies_input() {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let runtime = LocalAgentRuntime::with_clock(storage, Arc::new(|| Ok(20_000)));
    runtime.initialize().await.expect("initialize");
    runtime
        .handle(envelope(
            "create-user-wait",
            HostCommand::CreateRun(CreateRunCommand {
                run_id: "run-user-wait".to_string(),
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
        .handle(envelope(
            "claim-user-wait",
            HostCommand::ClaimNextRun(ClaimNextRunCommand {
                worker_id: "model-worker".to_string(),
                lease_duration_ms: 10_000,
            }),
        ))
        .await;
    let claim = match claimed.result.expect("claim") {
        HostResult::Claim { claim: Some(claim) } => claim,
        result => panic!("unexpected result: {result:?}"),
    };
    let waiting = runtime
        .handle(envelope(
            "wait-user",
            HostCommand::CommitStep(CommitStepCommand {
                run_id: claim.run.run_id,
                claim_token: claim.claim_token,
                expected_version: claim.run.version,
                outcome: LocalAgentStepOutcome::WaitForUser {
                    prompt: json!({"question": "Continue?"}),
                    checkpoint: json!({"response_id": "response-user-wait"}),
                },
            }),
        ))
        .await;
    let waiting = match waiting.result.expect("waiting") {
        HostResult::Run { run } => run,
        result => panic!("unexpected result: {result:?}"),
    };
    assert_eq!(waiting.status, LocalAgentRunStatus::WaitingUser);

    let resumed = runtime
        .handle(envelope(
            "resume-user",
            HostCommand::ResumeRun(ResumeRunCommand {
                run_id: waiting.run_id,
                expected_version: waiting.version,
                expected_status: LocalAgentRunStatus::WaitingUser,
                reason: "user replied".to_string(),
                input: json!({"answer": "yes"}),
            }),
        ))
        .await;
    let resumed = match resumed.result.expect("resumed") {
        HostResult::Run { run } => run,
        result => panic!("unexpected result: {result:?}"),
    };
    assert_eq!(resumed.status, LocalAgentRunStatus::ContinuationReady);
    assert_eq!(
        resumed.checkpoint,
        json!({"response_id": "response-user-wait"})
    );
    assert_eq!(
        resumed
            .continuation_input
            .as_ref()
            .and_then(|value| value.get("input")),
        Some(&json!({"answer": "yes"}))
    );
}
