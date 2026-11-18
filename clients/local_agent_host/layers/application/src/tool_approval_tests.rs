// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use chatos_client_storage::SqliteClientStorage;
use chatos_local_agent_protocol::{
    CancelRunCommand, ClaimNextRunCommand, ClaimNextToolCommand, CommitStepCommand,
    CommitToolCommand, CreateRunCommand, DecideToolApprovalCommand, HostCommand,
    HostRequestEnvelope, HostResult, ListPendingToolApprovalsCommand, LocalAgentRunStatus,
    LocalAgentStepOutcome, LocalAgentToolApprovalDecision, LocalAgentToolApprovalStatus,
    LocalAgentToolCall, LocalAgentToolOutcome, LOCAL_AGENT_PROTOCOL_VERSION,
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

#[tokio::test]
async fn approval_gate_blocks_claim_and_persists_approve_or_reject() {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let runtime = LocalAgentRuntime::with_clock(storage, Arc::new(|| Ok(10_000)));
    runtime
        .try_handle(request(
            "create",
            HostCommand::CreateRun(CreateRunCommand {
                run_id: "run-approval".to_string(),
                owner_user_id: "user-1".to_string(),
                owner_entity_type: "task".to_string(),
                owner_entity_id: "task-1".to_string(),
                profile_key: "task_execution".to_string(),
                model_config_ref: "model-1".to_string(),
                model_config_revision: "revision-1".to_string(),
                capability_policy_revision: "policy-1".to_string(),
                input: json!({"prompt": "write files"}),
                max_iterations: 4,
            }),
        ))
        .await
        .expect("create");
    let claim = runtime
        .try_handle(request(
            "claim-run",
            HostCommand::ClaimNextRun(ClaimNextRunCommand {
                owner_user_id: "user-1".to_string(),
                worker_id: "model-worker".to_string(),
                lease_duration_ms: 10_000,
            }),
        ))
        .await
        .expect("claim");
    let HostResult::Claim { claim: Some(claim) } = claim else {
        panic!("expected Run claim")
    };
    runtime
        .try_handle(request(
            "wait-tools",
            HostCommand::CommitStep(CommitStepCommand {
                owner_user_id: "user-1".to_string(),
                run_id: claim.run.run_id,
                claim_token: claim.claim_token,
                expected_version: claim.run.version,
                outcome: LocalAgentStepOutcome::WaitForTool {
                    batch_id: "batch-approval".to_string(),
                    tool_calls: vec![
                        LocalAgentToolCall {
                            call_id: "call-approved".to_string(),
                            tool_name: "write_file".to_string(),
                            arguments: json!({"path": "approved.txt"}),
                            side_effecting: true,
                            requires_approval: true,
                        },
                        LocalAgentToolCall {
                            call_id: "call-rejected".to_string(),
                            tool_name: "run_terminal".to_string(),
                            arguments: json!({"command": "unsafe"}),
                            side_effecting: true,
                            requires_approval: true,
                        },
                    ],
                    checkpoint: json!({"response_id": "response-1"}),
                },
            }),
        ))
        .await
        .expect("wait tools");

    let blocked = runtime
        .try_handle(request(
            "claim-before-approval",
            HostCommand::ClaimNextTool(ClaimNextToolCommand {
                owner_user_id: "user-1".to_string(),
                worker_id: "native-worker".to_string(),
                lease_duration_ms: 10_000,
                include_tool_names: None,
                exclude_tool_names: Vec::new(),
                execution_route: None,
            }),
        ))
        .await
        .expect("blocked claim");
    assert!(matches!(blocked, HostResult::ToolClaim { claim: None }));

    let pending = runtime
        .try_handle(request(
            "list-approvals",
            HostCommand::ListPendingToolApprovals(ListPendingToolApprovalsCommand {
                owner_user_id: "user-1".to_string(),
                limit: 10,
            }),
        ))
        .await
        .expect("list approvals");
    let HostResult::PendingToolApprovals { invocations } = pending else {
        panic!("expected approvals")
    };
    assert_eq!(invocations.len(), 2);
    assert!(invocations
        .iter()
        .all(|item| item.approval_status == LocalAgentToolApprovalStatus::Pending));
    let approved = invocations
        .iter()
        .find(|item| item.call_id == "call-approved")
        .expect("approved candidate");
    let rejected = invocations
        .iter()
        .find(|item| item.call_id == "call-rejected")
        .expect("rejected candidate");

    let approval = HostCommand::DecideToolApproval(DecideToolApprovalCommand {
        owner_user_id: "user-1".to_string(),
        invocation_id: approved.invocation_id.clone(),
        expected_version: approved.version,
        decision: LocalAgentToolApprovalDecision::Approve,
        decided_by: "user-1".to_string(),
        reason: "user approved this file write".to_string(),
    });
    let approved_result = runtime
        .try_handle(request("approve-tool", approval.clone()))
        .await
        .expect("approve");
    let replay = runtime
        .try_handle(request("approve-tool", approval))
        .await
        .expect("approval replay");
    assert_eq!(approved_result, replay);

    let rejected_result = runtime
        .try_handle(request(
            "reject-tool",
            HostCommand::DecideToolApproval(DecideToolApprovalCommand {
                owner_user_id: "user-1".to_string(),
                invocation_id: rejected.invocation_id.clone(),
                expected_version: rejected.version,
                decision: LocalAgentToolApprovalDecision::Reject,
                decided_by: "user-1".to_string(),
                reason: "terminal command was rejected".to_string(),
            }),
        ))
        .await
        .expect("reject");
    assert!(matches!(
        rejected_result,
        HostResult::ToolApproval { result }
            if result.invocation.approval_status == LocalAgentToolApprovalStatus::Rejected
                && result.invocation.status == chatos_local_agent_protocol::LocalAgentToolStatus::Failed
    ));

    let claimed = runtime
        .try_handle(request(
            "claim-after-approval",
            HostCommand::ClaimNextTool(ClaimNextToolCommand {
                owner_user_id: "user-1".to_string(),
                worker_id: "native-worker".to_string(),
                lease_duration_ms: 10_000,
                include_tool_names: None,
                exclude_tool_names: Vec::new(),
                execution_route: None,
            }),
        ))
        .await
        .expect("approved claim");
    let HostResult::ToolClaim { claim: Some(claim) } = claimed else {
        panic!("expected approved claim")
    };
    assert_eq!(claim.invocation.call_id, "call-approved");
    assert_eq!(
        claim.invocation.approval_status,
        LocalAgentToolApprovalStatus::Approved
    );
    let committed = runtime
        .try_handle(request(
            "commit-approved",
            HostCommand::CommitTool(CommitToolCommand {
                owner_user_id: "user-1".to_string(),
                invocation_id: claim.invocation.invocation_id,
                claim_token: claim.claim_token,
                expected_version: claim.invocation.version,
                outcome: LocalAgentToolOutcome::Succeeded {
                    output: json!({"path": "approved.txt"}),
                },
            }),
        ))
        .await
        .expect("commit");
    assert!(matches!(
        committed,
        HostResult::ToolCommit { result }
            if result.run.status == LocalAgentRunStatus::ContinuationReady
    ));

    let other = runtime
        .try_handle(request(
            "list-other-user",
            HostCommand::ListPendingToolApprovals(ListPendingToolApprovalsCommand {
                owner_user_id: "user-2".to_string(),
                limit: 10,
            }),
        ))
        .await
        .expect("other user");
    assert!(matches!(
        other,
        HostResult::PendingToolApprovals { invocations } if invocations.is_empty()
    ));
}

#[tokio::test]
async fn cancelling_run_removes_its_pending_tool_approval() {
    let storage = Arc::new(
        SqliteClientStorage::connect_memory()
            .await
            .expect("storage"),
    );
    let runtime = LocalAgentRuntime::with_clock(storage, Arc::new(|| Ok(10_000)));
    runtime
        .try_handle(request(
            "create-cancel-approval",
            HostCommand::CreateRun(CreateRunCommand {
                run_id: "run-cancel-approval".to_string(),
                owner_user_id: "user-1".to_string(),
                owner_entity_type: "conversation".to_string(),
                owner_entity_id: "conversation-cancel-approval".to_string(),
                profile_key: "task_execution".to_string(),
                model_config_ref: "model-1".to_string(),
                model_config_revision: "revision-1".to_string(),
                capability_policy_revision: "policy-1".to_string(),
                input: json!({"prompt": "wait for approval"}),
                max_iterations: 4,
            }),
        ))
        .await
        .expect("create");
    let claimed = runtime
        .try_handle(request(
            "claim-cancel-approval",
            HostCommand::ClaimNextRun(ClaimNextRunCommand {
                owner_user_id: "user-1".to_string(),
                worker_id: "model-worker".to_string(),
                lease_duration_ms: 10_000,
            }),
        ))
        .await
        .expect("claim");
    let HostResult::Claim { claim: Some(claim) } = claimed else {
        panic!("expected Run claim")
    };
    runtime
        .try_handle(request(
            "wait-cancel-approval",
            HostCommand::CommitStep(CommitStepCommand {
                owner_user_id: "user-1".to_string(),
                run_id: claim.run.run_id.clone(),
                claim_token: claim.claim_token,
                expected_version: claim.run.version,
                outcome: LocalAgentStepOutcome::WaitForTool {
                    batch_id: "batch-cancel-approval".to_string(),
                    tool_calls: vec![LocalAgentToolCall {
                        call_id: "call-cancel-approval".to_string(),
                        tool_name: "write_file".to_string(),
                        arguments: json!({"path": "cancelled.txt"}),
                        side_effecting: true,
                        requires_approval: true,
                    }],
                    checkpoint: json!({"response_id": "response-cancel-approval"}),
                },
            }),
        ))
        .await
        .expect("wait for approval");

    let pending = runtime
        .try_handle(request(
            "list-before-cancel",
            HostCommand::ListPendingToolApprovals(ListPendingToolApprovalsCommand {
                owner_user_id: "user-1".to_string(),
                limit: 10,
            }),
        ))
        .await
        .expect("list before cancel");
    let HostResult::PendingToolApprovals { invocations } = pending else {
        panic!("expected pending approvals")
    };
    assert_eq!(invocations.len(), 1);
    let stale = invocations[0].clone();

    let cancelled = runtime
        .try_handle(request(
            "cancel-pending-approval",
            HostCommand::CancelRun(CancelRunCommand {
                owner_user_id: "user-1".to_string(),
                run_id: claim.run.run_id,
                expected_version: None,
                reason: "task was cancelled".to_string(),
            }),
        ))
        .await
        .expect("cancel Run");
    assert!(matches!(
        cancelled,
        HostResult::Run { run } if run.status == LocalAgentRunStatus::Cancelled
    ));

    let pending = runtime
        .try_handle(request(
            "list-after-cancel",
            HostCommand::ListPendingToolApprovals(ListPendingToolApprovalsCommand {
                owner_user_id: "user-1".to_string(),
                limit: 10,
            }),
        ))
        .await
        .expect("list after cancel");
    assert!(matches!(
        pending,
        HostResult::PendingToolApprovals { invocations } if invocations.is_empty()
    ));

    let stale_decision = runtime
        .try_handle(request(
            "decide-after-cancel",
            HostCommand::DecideToolApproval(DecideToolApprovalCommand {
                owner_user_id: "user-1".to_string(),
                invocation_id: stale.invocation_id,
                expected_version: stale.version,
                decision: LocalAgentToolApprovalDecision::Approve,
                decided_by: "user-1".to_string(),
                reason: "stale approval must not revive cancelled work".to_string(),
            }),
        ))
        .await;
    assert!(stale_decision.is_err());
}
