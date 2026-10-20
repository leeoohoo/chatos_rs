mod models;
mod projection;
#[cfg(test)]
mod tests;

use chatos_local_agent_protocol::{
    DecideToolApprovalCommand, HostCommand, HostResult, ListPendingToolApprovalsCommand,
    LocalAgentToolApprovalDecision, LocalAgentToolApprovalStatus, LocalAgentToolInvocationRecord,
    LocalAgentToolStatus,
};
use tauri::State;

use crate::host_lifecycle::HostManager;
use models::{
    ToolApproval, ToolApprovalDecision, ToolApprovalMutationInput, ToolApprovalResolution,
};
use projection::pending_approval;

const APPROVAL_LIMIT: u32 = 100;
const REVIEWER_ID: &str = "windows-react-human";

#[tauri::command]
pub async fn list_tool_approvals(
    manager: State<'_, HostManager>,
    owner_user_id: String,
) -> Result<Vec<ToolApproval>, String> {
    let owner_user_id = required("owner_user_id", owner_user_id)?;
    let invocations = load_pending(&manager, &owner_user_id).await?;
    Ok(project_pending(invocations))
}

#[tauri::command]
pub async fn decide_tool_approval(
    manager: State<'_, HostManager>,
    input: ToolApprovalMutationInput,
) -> Result<ToolApprovalResolution, String> {
    let owner_user_id = required("owner_user_id", input.owner_user_id)?;
    let invocation_id = required("invocation_id", input.invocation_id)?;
    if input.expected_version == 0 {
        return Err("审批版本无效。".into());
    }

    // Re-read durable truth immediately before deciding. This prevents a stale overlay from
    // applying a decision to a newer invocation version or to work that is no longer pending.
    let current = load_pending(&manager, &owner_user_id)
        .await?
        .into_iter()
        .find(|invocation| invocation.invocation_id == invocation_id)
        .filter(is_pending)
        .ok_or_else(|| "该审批已经处理或不再存在。".to_owned())?;
    if current.version != input.expected_version {
        return Err("该审批已经更新，请查看最新内容后重新决定。".into());
    }

    let (decision, reason) = match input.decision {
        ToolApprovalDecision::Accept => (
            LocalAgentToolApprovalDecision::Approve,
            "用户已在 Windows 客户端允许这次操作。",
        ),
        ToolApprovalDecision::Decline => (
            LocalAgentToolApprovalDecision::Reject,
            "用户已在 Windows 客户端拒绝这次操作。",
        ),
    };
    let result = manager
        .execute(HostCommand::DecideToolApproval(DecideToolApprovalCommand {
            owner_user_id,
            invocation_id: invocation_id.clone(),
            expected_version: current.version,
            decision,
            decided_by: REVIEWER_ID.into(),
            reason: reason.into(),
        }))
        .await?;
    let HostResult::ToolApproval { result } = result else {
        return Err("Local Agent Host 返回了错误的审批响应。".into());
    };
    if result.invocation.invocation_id != invocation_id
        || result.invocation.run_id != current.run_id
        || !result.invocation.requires_approval
        || result.invocation.version <= current.version
        || result.invocation.approval_status
            != match input.decision {
                ToolApprovalDecision::Accept => LocalAgentToolApprovalStatus::Approved,
                ToolApprovalDecision::Decline => LocalAgentToolApprovalStatus::Rejected,
            }
    {
        return Err("Local Agent Host 返回了不匹配的审批结果。".into());
    }
    Ok(ToolApprovalResolution {
        invocation_id,
        invocation_version: result.invocation.version,
        decision: input.decision,
    })
}

async fn load_pending(
    manager: &HostManager,
    owner_user_id: &str,
) -> Result<Vec<LocalAgentToolInvocationRecord>, String> {
    let result = manager
        .execute(HostCommand::ListPendingToolApprovals(
            ListPendingToolApprovalsCommand {
                owner_user_id: owner_user_id.to_owned(),
                limit: APPROVAL_LIMIT,
            },
        ))
        .await?;
    let HostResult::PendingToolApprovals { invocations } = result else {
        return Err("Local Agent Host 返回了错误的审批队列。".into());
    };
    Ok(invocations)
}

fn project_pending(invocations: Vec<LocalAgentToolInvocationRecord>) -> Vec<ToolApproval> {
    let mut approvals = invocations
        .iter()
        .filter_map(pending_approval)
        .collect::<Vec<_>>();
    approvals.sort_by(|left, right| {
        (left.created_at_unix_ms, left.invocation_id.as_str())
            .cmp(&(right.created_at_unix_ms, right.invocation_id.as_str()))
    });
    approvals
}

fn is_pending(invocation: &LocalAgentToolInvocationRecord) -> bool {
    invocation.requires_approval
        && invocation.approval_status == LocalAgentToolApprovalStatus::Pending
        && invocation.status == LocalAgentToolStatus::Pending
        && invocation.version > 0
}

fn required(field: &str, value: String) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err(format!("{field} is required"))
    } else {
        Ok(trimmed.to_owned())
    }
}
