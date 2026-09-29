// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use chatos_client_storage::SqliteClientStorage;
use chatos_local_agent_protocol::{
    ClaimNextRunCommand, ClaimNextToolCommand, CommitStepCommand, CommitToolCommand,
    CreateRunCommand, DecideToolApprovalCommand, HostCommand, HostRequestEnvelope, HostResult,
    ListPendingToolApprovalsCommand, LocalAgentRunStatus, LocalAgentStepOutcome,
    LocalAgentToolApprovalDecision, LocalAgentToolApprovalStatus, LocalAgentToolCall,
    LocalAgentToolOutcome, LOCAL_AGENT_PROTOCOL_VERSION,
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
                profile_key: "task_runner".to_string(),
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
