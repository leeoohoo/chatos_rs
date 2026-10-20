pub(crate) mod journal;
mod models;
mod projection;
pub(crate) mod session;
pub(crate) mod settings;
#[cfg(test)]
mod tests;

use chatos_local_agent_protocol::{
    DecideToolApprovalCommand, HostCommand, HostResult, ListPendingToolApprovalsCommand,
    LocalAgentToolApprovalDecision, LocalAgentToolApprovalStatus, LocalAgentToolInvocationRecord,
    LocalAgentToolStatus,
};
use tauri::{AppHandle, State};

use crate::host_lifecycle::HostManager;
use crate::platform_execution_context::{
    resolve_execution_directory, resolve_platform_execution_context, resolve_terminal_command,
    resolve_terminal_path, resolve_terminal_purpose,
};
use journal::{ApprovalDecisionJournal, RecordedToolApprovalDecision};
use models::{
    ToolApproval, ToolApprovalDecision, ToolApprovalMode, ToolApprovalModeMutationInput,
    ToolApprovalModelMutationInput, ToolApprovalMutationInput, ToolApprovalResolution,
    ToolApprovalSettings,
};
use projection::pending_approval;
use session::ApprovalSessionScope;
use settings::ApprovalSettingsStore;

const APPROVAL_LIMIT: u32 = 100;
const REVIEWER_ID: &str = "windows-react-human";
const SESSION_REVIEWER_ID: &str = "windows-react-session";
const SESSION_APPROVAL_REASON: &str = "用户已允许当前 Windows 客户端会话执行此类操作。";
const POLICY_REVIEWER_ID: &str = "windows-react-policy";
const POLICY_APPROVAL_REASON: &str = "当前 Windows 审批策略允许直接执行此操作。";

#[tauri::command]
pub async fn list_tool_approvals(
    app: AppHandle,
    manager: State<'_, HostManager>,
    journal: State<'_, ApprovalDecisionJournal>,
    settings: State<'_, ApprovalSettingsStore>,
    owner_user_id: String,
) -> Result<Vec<ToolApproval>, String> {
    let owner_user_id = required("owner_user_id", owner_user_id)?;
    let mode = load_approval_mode(&settings, &owner_user_id).await?;
    let invocations =
        reconcile_recorded_decisions(&manager, &journal, &settings, &owner_user_id, mode).await?;
    let invocations = reconcile_session_decisions(
        &app,
        &manager,
        &journal,
        &settings,
        &owner_user_id,
        mode,
        invocations,
    )
    .await?;
    let invocations = reconcile_policy_decisions(
        &app,
        &manager,
        &journal,
        &settings,
        &owner_user_id,
        mode,
        invocations,
    )
    .await?;
    Ok(project_pending(invocations))
}

#[tauri::command]
pub async fn decide_tool_approval(
    app: AppHandle,
    manager: State<'_, HostManager>,
    journal: State<'_, ApprovalDecisionJournal>,
    settings: State<'_, ApprovalSettingsStore>,
    input: ToolApprovalMutationInput,
) -> Result<ToolApprovalResolution, String> {
    let owner_user_id = required("owner_user_id", input.owner_user_id)?;
    let invocation_id = required("invocation_id", input.invocation_id)?;
    if input.expected_version == 0 {
        return Err("审批版本无效。".into());
    }
    let session_generation = manager.approval_sessions().generation();

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
    let presentation =
        pending_approval(&current).ok_or_else(|| "该审批已经处理或不再存在。".to_owned())?;
    if !presentation.available_decisions.contains(&input.decision) {
        return Err("当前审批不允许该操作。".into());
    }
    let session_scope = match input.decision {
        ToolApprovalDecision::Accept | ToolApprovalDecision::AcceptForSession
            if is_session_terminal_tool(&current.tool_name) =>
        {
            Some(resolve_terminal_approval_scope(&app, &manager, &owner_user_id, &current).await?)
        }
        ToolApprovalDecision::AcceptForSession => return Err("当前审批不支持本次会话允许。".into()),
        _ => None,
    };
    let reason = match input.decision {
        ToolApprovalDecision::Accept => "用户已在 Windows 客户端允许这次操作。",
        ToolApprovalDecision::AcceptForSession => {
            "用户已在 Windows 客户端允许本次会话继续执行此类操作。"
        }
        ToolApprovalDecision::Decline => "用户已在 Windows 客户端拒绝这次操作。",
    };
    let mode = load_approval_mode(&settings, &owner_user_id).await?;
    let resolved = submit_decision(
        &manager,
        &journal,
        &settings,
        &owner_user_id,
        &current,
        mode,
        input.decision,
        REVIEWER_ID,
        reason,
    )
    .await?;
    if input.decision == ToolApprovalDecision::AcceptForSession {
        let scope = session_scope.ok_or_else(|| "当前审批缺少安全会话范围。".to_owned())?;
        // The Host decision must become durable first. A concurrent Host reset changes the
        // generation and prevents this in-flight response from repopulating a new session.
        manager
            .approval_sessions()
            .insert_if_current(session_generation, scope);
    }
    Ok(ToolApprovalResolution {
        invocation_id,
        invocation_version: resolved.version,
        decision: input.decision,
    })
}

#[tauri::command]
pub async fn load_tool_approval_settings(
    manager: State<'_, HostManager>,
    settings: State<'_, ApprovalSettingsStore>,
    owner_user_id: String,
) -> Result<ToolApprovalSettings, String> {
    let owner_user_id = required("owner_user_id", owner_user_id)?;
    approval_settings_snapshot(&manager, &settings, &owner_user_id).await
}

#[tauri::command]
pub async fn update_tool_approval_mode(
    manager: State<'_, HostManager>,
    settings: State<'_, ApprovalSettingsStore>,
    input: ToolApprovalModeMutationInput,
) -> Result<ToolApprovalSettings, String> {
    let owner_user_id = required("owner_user_id", input.owner_user_id)?;
    if !manager.is_running_for(&owner_user_id).await {
        return Err("Local Agent Host 尚未就绪。".into());
    }
    let store = settings.inner().clone();
    let owner = owner_user_id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        store.update_mode(&owner, input.mode, input.risk_acknowledged)
    })
    .await
    .map_err(|_| "审批设置任务异常退出。".to_owned())??;
    approval_settings_snapshot(&manager, &settings, &owner_user_id).await
}

#[tauri::command]
pub async fn update_tool_approval_model(
    manager: State<'_, HostManager>,
    settings: State<'_, ApprovalSettingsStore>,
    input: ToolApprovalModelMutationInput,
) -> Result<ToolApprovalSettings, String> {
    let owner_user_id = required("owner_user_id", input.owner_user_id)?;
    let models = manager.model_options(&owner_user_id).await?;
    let store = settings.inner().clone();
    let owner = owner_user_id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        store.update_model(
            &owner,
            input.selected_model_id,
            input.selected_thinking_level,
            &models,
        )
    })
    .await
    .map_err(|_| "审批模型设置任务异常退出。".to_owned())??;
    approval_settings_snapshot(&manager, &settings, &owner_user_id).await
}

async fn resolve_terminal_approval_scope(
    app: &AppHandle,
    manager: &HostManager,
    owner_user_id: &str,
    invocation: &LocalAgentToolInvocationRecord,
) -> Result<String, String> {
    let context =
        resolve_platform_execution_context(app, manager, owner_user_id, &invocation.run_id).await?;
    if !context.authorization.allows_terminal()
        || context.workspace_scope_id.is_empty()
        || context.device_id.is_empty()
        || context.conversation_id.is_empty()
    {
        return Err("这个 Run 没有选择终端执行能力。".into());
    }
    let (cwd, purpose, permission, command, arguments) = match invocation.tool_name.as_str() {
        "execute_command" => {
            let values = invocation
                .arguments
                .as_object()
                .ok_or_else(|| "终端命令参数无效。".to_owned())?;
            let terminal_command =
                resolve_terminal_command(values).ok_or_else(|| "终端命令不能为空。".to_owned())?;
            let purpose = resolve_terminal_purpose(values)
                .ok_or_else(|| "终端命令必须提供有效的运行目的。".to_owned())?;
            let cwd = resolve_execution_directory(
                &context.execution_root,
                resolve_terminal_path(values),
            )?;
            (
                cwd,
                Some(purpose.to_owned()),
                "在当前本地项目中执行命令".to_owned(),
                "powershell.exe".to_owned(),
                vec![
                    "-NoLogo".to_owned(),
                    "-NoProfile".to_owned(),
                    "-Command".to_owned(),
                    terminal_command.to_owned(),
                ],
            )
        }
        "process_write" => (
            context.execution_root.clone(),
            None,
            "向当前任务启动的本地命令写入输入".to_owned(),
            "process_write".to_owned(),
            vec!["interactive input hidden".to_owned()],
        ),
        "process_kill" => (
            context.execution_root.clone(),
            None,
            "终止当前任务启动的本地命令".to_owned(),
            "process_kill".to_owned(),
            vec!["task-owned process".to_owned()],
        ),
        _ => return Err("当前审批不支持本次会话允许。".into()),
    };
    Ok(ApprovalSessionScope {
        owner_user_id: owner_user_id.to_owned(),
        device_id: context.device_id,
        workspace_id: context.workspace_scope_id,
        project_root: context.execution_root,
        cwd,
        base: format!("local-agent-terminal:{}", context.conversation_id),
        purpose,
        requested_permissions_description: permission,
        command,
        arguments,
    }
    .key())
}

async fn reconcile_session_decisions(
    app: &AppHandle,
    manager: &HostManager,
    journal: &ApprovalDecisionJournal,
    settings: &ApprovalSettingsStore,
    owner_user_id: &str,
    mode: ToolApprovalMode,
    mut invocations: Vec<LocalAgentToolInvocationRecord>,
) -> Result<Vec<LocalAgentToolInvocationRecord>, String> {
    if manager.approval_sessions().is_empty() {
        return Ok(invocations);
    }
    let generation = manager.approval_sessions().generation();
    let mut changed = false;
    for invocation in invocations.iter().filter(|invocation| {
        is_pending(invocation) && is_session_terminal_tool(&invocation.tool_name)
    }) {
        let Ok(scope) =
            resolve_terminal_approval_scope(app, manager, owner_user_id, invocation).await
        else {
            continue;
        };
        if !manager.approval_sessions().contains(generation, &scope) {
            continue;
        }
        submit_decision(
            manager,
            journal,
            settings,
            owner_user_id,
            invocation,
            mode,
            ToolApprovalDecision::Accept,
            SESSION_REVIEWER_ID,
            SESSION_APPROVAL_REASON,
        )
        .await?;
        changed = true;
    }
    if changed {
        invocations = load_pending(manager, owner_user_id).await?;
    }
    Ok(invocations)
}

async fn reconcile_policy_decisions(
    app: &AppHandle,
    manager: &HostManager,
    journal: &ApprovalDecisionJournal,
    settings: &ApprovalSettingsStore,
    owner_user_id: &str,
    mode: ToolApprovalMode,
    mut invocations: Vec<LocalAgentToolInvocationRecord>,
) -> Result<Vec<LocalAgentToolInvocationRecord>, String> {
    if mode != ToolApprovalMode::FullControl {
        // The Windows approval Agent runtime has not crossed its source-of-truth gate yet.
        // auto_approval therefore follows macOS's unavailable-reviewer fallback and remains
        // in the Human queue instead of silently granting access.
        return Ok(invocations);
    }
    let mut changed = false;
    for invocation in invocations
        .iter()
        .filter(|invocation| policy_auto_approval_candidate(mode, invocation))
    {
        if resolve_terminal_approval_scope(app, manager, owner_user_id, invocation)
            .await
            .is_err()
        {
            continue;
        }
        submit_decision(
            manager,
            journal,
            settings,
            owner_user_id,
            invocation,
            mode,
            ToolApprovalDecision::Accept,
            POLICY_REVIEWER_ID,
            POLICY_APPROVAL_REASON,
        )
        .await?;
        changed = true;
    }
    if changed {
        invocations = load_pending(manager, owner_user_id).await?;
    }
    Ok(invocations)
}

async fn submit_decision(
    manager: &HostManager,
    journal: &ApprovalDecisionJournal,
    settings: &ApprovalSettingsStore,
    owner_user_id: &str,
    invocation: &LocalAgentToolInvocationRecord,
    mode: ToolApprovalMode,
    decision: ToolApprovalDecision,
    decided_by: &str,
    reason: &str,
) -> Result<LocalAgentToolInvocationRecord, String> {
    let record = RecordedToolApprovalDecision {
        owner_user_id: owner_user_id.to_owned(),
        invocation_id: invocation.invocation_id.clone(),
        invocation_version: invocation.version,
        mode: Some(mode),
        decision,
        decided_by: decided_by.to_owned(),
        reason: reason.to_owned(),
        recorded_at_unix_ms: now_unix_ms(),
    };
    let approval =
        pending_approval(invocation).ok_or_else(|| "该审批已经处理或不再存在。".to_owned())?;
    let record = journal.record(record)?;
    record_approval_history(
        settings,
        owner_user_id,
        approval,
        record.mode.unwrap_or(mode),
        decision,
        record.recorded_at_unix_ms,
    )
    .await?;
    let result = manager
        .execute(HostCommand::DecideToolApproval(DecideToolApprovalCommand {
            owner_user_id: owner_user_id.to_owned(),
            invocation_id: invocation.invocation_id.clone(),
            expected_version: invocation.version,
            decision: host_decision(decision),
            decided_by: decided_by.to_owned(),
            reason: reason.to_owned(),
        }))
        .await?;
    let HostResult::ToolApproval { result } = result else {
        return Err("Local Agent Host 返回了错误的审批响应。".into());
    };
    if result.invocation.invocation_id != invocation.invocation_id
        || result.invocation.run_id != invocation.run_id
        || !result.invocation.requires_approval
        || result.invocation.version <= invocation.version
        || result.invocation.approval_status != expected_status(decision)
    {
        return Err("Local Agent Host 返回了不匹配的审批结果。".into());
    }
    let _ = journal.remove(&record);
    Ok(result.invocation)
}

async fn approval_settings_snapshot(
    manager: &HostManager,
    settings: &ApprovalSettingsStore,
    owner_user_id: &str,
) -> Result<ToolApprovalSettings, String> {
    let models = manager.model_options(owner_user_id).await?;
    let store = settings.clone();
    let owner = owner_user_id.to_owned();
    tauri::async_runtime::spawn_blocking(move || store.load(&owner, &models))
        .await
        .map_err(|_| "审批设置任务异常退出。".to_owned())?
}

async fn load_approval_mode(
    settings: &ApprovalSettingsStore,
    owner_user_id: &str,
) -> Result<ToolApprovalMode, String> {
    let store = settings.clone();
    let owner = owner_user_id.to_owned();
    tauri::async_runtime::spawn_blocking(move || store.mode(&owner))
        .await
        .map_err(|_| "审批设置任务异常退出。".to_owned())?
}

async fn record_approval_history(
    settings: &ApprovalSettingsStore,
    owner_user_id: &str,
    approval: ToolApproval,
    mode: ToolApprovalMode,
    decision: ToolApprovalDecision,
    decided_at_unix_ms: i64,
) -> Result<(), String> {
    let store = settings.clone();
    let owner = owner_user_id.to_owned();
    tauri::async_runtime::spawn_blocking(move || {
        store.record_decision(&owner, &approval, mode, decision, decided_at_unix_ms)
    })
    .await
    .map_err(|_| "审批记录任务异常退出。".to_owned())?
}

async fn reconcile_recorded_decisions(
    manager: &HostManager,
    journal: &ApprovalDecisionJournal,
    settings: &ApprovalSettingsStore,
    owner_user_id: &str,
    fallback_mode: ToolApprovalMode,
) -> Result<Vec<LocalAgentToolInvocationRecord>, String> {
    let mut invocations = load_pending(manager, owner_user_id).await?;
    let mut changed = false;
    for record in journal.snapshot(owner_user_id) {
        let matching = invocations.iter().find(|invocation| {
            record.matches(owner_user_id, &invocation.invocation_id, invocation.version)
                && is_pending(invocation)
        });
        let Some(invocation) = matching else {
            // Host is authoritative: a missing exact identity/version means the write either
            // succeeded before transport loss or this bounded record is stale.
            let _ = journal.remove(&record);
            continue;
        };
        let approval =
            pending_approval(invocation).ok_or_else(|| "该审批已经处理或不再存在。".to_owned())?;
        record_approval_history(
            settings,
            owner_user_id,
            approval,
            record.mode.unwrap_or(fallback_mode),
            record.decision,
            record.recorded_at_unix_ms,
        )
        .await?;
        let result = manager
            .execute(HostCommand::DecideToolApproval(DecideToolApprovalCommand {
                owner_user_id: owner_user_id.to_owned(),
                invocation_id: invocation.invocation_id.clone(),
                expected_version: invocation.version,
                decision: host_decision(record.decision),
                decided_by: record.decided_by.clone(),
                reason: record.reason.clone(),
            }))
            .await?;
        let HostResult::ToolApproval { result } = result else {
            return Err("Local Agent Host 返回了错误的审批恢复响应。".into());
        };
        if result.invocation.invocation_id != record.invocation_id
            || result.invocation.run_id != invocation.run_id
            || !result.invocation.requires_approval
            || result.invocation.version <= record.invocation_version
            || result.invocation.approval_status != expected_status(record.decision)
        {
            return Err("Local Agent Host 返回了不匹配的审批恢复结果。".into());
        }
        let _ = journal.remove(&record);
        changed = true;
    }
    if changed {
        invocations = load_pending(manager, owner_user_id).await?;
    }
    Ok(invocations)
}

fn host_decision(decision: ToolApprovalDecision) -> LocalAgentToolApprovalDecision {
    match decision {
        ToolApprovalDecision::Accept | ToolApprovalDecision::AcceptForSession => {
            LocalAgentToolApprovalDecision::Approve
        }
        ToolApprovalDecision::Decline => LocalAgentToolApprovalDecision::Reject,
    }
}

fn expected_status(decision: ToolApprovalDecision) -> LocalAgentToolApprovalStatus {
    match decision {
        ToolApprovalDecision::Accept | ToolApprovalDecision::AcceptForSession => {
            LocalAgentToolApprovalStatus::Approved
        }
        ToolApprovalDecision::Decline => LocalAgentToolApprovalStatus::Rejected,
    }
}

fn is_session_terminal_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "execute_command" | "process_write" | "process_kill"
    )
}

fn policy_auto_approval_candidate(
    mode: ToolApprovalMode,
    invocation: &LocalAgentToolInvocationRecord,
) -> bool {
    mode == ToolApprovalMode::FullControl
        && is_pending(invocation)
        && is_session_terminal_tool(&invocation.tool_name)
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

fn now_unix_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or(1)
}
