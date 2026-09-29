// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use chatos_client_storage::SqliteClientStorage;
use chatos_local_agent_protocol::{
    ClaimNextRunCommand, ClaimNextToolCommand, CommitStepCommand, CommitToolCommand,
    CreateRunCommand, HostCommand, HostRequestEnvelope, HostResult, LocalAgentRunStatus,
    LocalAgentStepOutcome, LocalAgentToolCall, LocalAgentToolOutcome, LOCAL_AGENT_PROTOCOL_VERSION,
};
use serde_json::json;
use std::sync::Arc;

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
async fn tool_batch_is_durable_and_resumes_after_all_results() {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let runtime = LocalAgentRuntime::with_clock(storage, Arc::new(|| Ok(10_000)));
    runtime.initialize("user-1").await.expect("initialize");
    runtime.handle(envelope("create-1", create_command())).await;
    let claimed = runtime
        .handle(envelope(
            "claim-run-1",
            HostCommand::ClaimNextRun(ClaimNextRunCommand {
                owner_user_id: "user-1".to_string(),
                worker_id: "model-worker".to_string(),
                lease_duration_ms: 10_000,
            }),
        ))
        .await;
    let claim = match claimed.result.expect("claim result") {
        HostResult::Claim { claim: Some(claim) } => claim,
        result => panic!("unexpected result: {result:?}"),
    };
    let waiting = runtime
        .handle(envelope(
            "commit-model-1",
            HostCommand::CommitStep(CommitStepCommand {
                owner_user_id: "user-1".to_string(),
                run_id: claim.run.run_id,
                claim_token: claim.claim_token,
                expected_version: claim.run.version,
                outcome: LocalAgentStepOutcome::WaitForTool {
                    batch_id: "batch-1".to_string(),
                    tool_calls: vec![
                        LocalAgentToolCall {
                            call_id: "call-read".to_string(),
                            tool_name: "read_file".to_string(),
                            arguments: json!({"path": "README.md"}),
                            side_effecting: false,
                            requires_approval: false,
                        },
                        LocalAgentToolCall {
                            call_id: "call-write".to_string(),
                            tool_name: "write_file".to_string(),
                            arguments: json!({"path": "result.txt"}),
                            side_effecting: true,
                            requires_approval: false,
                        },
                    ],
                    checkpoint: json!({"response_id": "response-1"}),
                },
            }),
        ))
        .await;
    let waiting_run = match waiting.result.expect("waiting result") {
        HostResult::Run { run } => run,
        result => panic!("unexpected result: {result:?}"),
    };
    assert_eq!(waiting_run.status, LocalAgentRunStatus::WaitingToolResult);

    for index in 0..2 {
        let claimed = runtime
            .handle(envelope(
                &format!("claim-tool-{index}"),
                HostCommand::ClaimNextTool(ClaimNextToolCommand {
                    owner_user_id: "user-1".to_string(),
                    worker_id: "tool-worker".to_string(),
                    lease_duration_ms: 10_000,
                    include_tool_names: None,
                    exclude_tool_names: Vec::new(),
                }),
            ))
            .await;
        let claim = match claimed.result.expect("tool claim result") {
            HostResult::ToolClaim { claim: Some(claim) } => claim,
            result => panic!("unexpected result: {result:?}"),
        };
        let outcome = if claim.invocation.tool_name == "read_file" {
            LocalAgentToolOutcome::Succeeded {
                output: json!({"content": "ok"}),
            }
        } else {
            LocalAgentToolOutcome::Failed {
                error: "write rejected".to_string(),
                detail: json!({"code": "permission_denied"}),
            }
        };
        if index == 0 {
            let rejected = runtime
                .handle(envelope(
                    "commit-tool-wrong-owner",
                    HostCommand::CommitTool(CommitToolCommand {
                        owner_user_id: "user-2".to_string(),
                        invocation_id: claim.invocation.invocation_id.clone(),
                        claim_token: claim.claim_token.clone(),
                        expected_version: claim.invocation.version,
                        outcome: outcome.clone(),
                    }),
                ))
                .await;
            assert_eq!(rejected.error.expect("not found").code, "not_found");
        }
        let committed = runtime
            .handle(envelope(
                &format!("commit-tool-{index}"),
                HostCommand::CommitTool(CommitToolCommand {
                    owner_user_id: "user-1".to_string(),
                    invocation_id: claim.invocation.invocation_id,
                    claim_token: claim.claim_token,
                    expected_version: claim.invocation.version,
                    outcome,
                }),
            ))
            .await;
        let result = match committed.result.expect("tool commit result") {
            HostResult::ToolCommit { result } => result,
            result => panic!("unexpected result: {result:?}"),
        };
        if index == 0 {
            assert_eq!(result.run.status, LocalAgentRunStatus::WaitingToolResult);
        } else {
            assert_eq!(result.run.status, LocalAgentRunStatus::ContinuationReady);
            assert!(result.run.pending_tool_batch.is_none());
            assert_eq!(result.run.checkpoint, json!({"response_id": "response-1"}));
            assert_eq!(
                result
                    .run
                    .continuation_input
                    .as_ref()
                    .and_then(|value| value.get("type"))
                    .and_then(serde_json::Value::as_str),
                Some("tool_results")
            );
        }
    }

    let next = runtime
        .handle(envelope(
            "claim-run-2",
            HostCommand::ClaimNextRun(ClaimNextRunCommand {
                owner_user_id: "user-1".to_string(),
                worker_id: "model-worker".to_string(),
                lease_duration_ms: 10_000,
            }),
        ))
        .await;
    assert!(matches!(
        next.result,
        Some(HostResult::Claim { claim: Some(_) })
    ));
}
