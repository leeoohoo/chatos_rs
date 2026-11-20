use super::*;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashSet;

mod domain_support;
mod execution_progress_store;
mod execution_support;
mod planning_mutation_store;
mod planning_projection;
mod planning_read_store;
mod planning_reference_support;
mod run_control_completion_store;
mod run_control_intent_store;
mod run_reconciliation_support;
mod run_scheduler_store;
mod scheduler_store;
mod team_chat_read_store;
mod team_chat_support;
mod team_chat_write_store;
mod todo_store;
mod workspace_store;

use domain_support::*;
pub(super) use execution_support::insert_todo_progress;
use execution_support::{
    executor_todo_message, load_dependencies, load_todo_asset_summaries, load_todo_prerequisites,
    load_todo_progress, project_team_todo, require_running_todo_tool_binding,
    todo_has_terminal_receipt, todo_progress_for_ui, validate_asset_update_suggestions,
};
pub(crate) use planning_projection::todo_planning_reference;
use planning_projection::{load_todo_assignee_options, todo_planning_response};
use planning_reference_support::*;
use run_reconciliation_support::*;
use team_chat_support::{
    active_room_member_ids, open_agent_direct, persist_team_chat_message_reference,
    resolve_team_message_reference, team_chat_routing_stop_reason,
};
pub(super) use team_chat_support::{resolve_agent_reference, resolve_team_reference};

const MAXIMUM_TEAM_TODOS: usize = 1_000;
const MAXIMUM_TEAM_CHAT_HOP_COUNT: i64 = 4;
const MAXIMUM_TEAM_CHAT_RUNS_PER_ROOT: i64 = 12;
const TODO_BUILTIN_CAPABILITIES: [&str; 5] = [
    "project_read",
    "project_write",
    "terminal",
    "requirement_survey_read",
    "requirement_survey_write",
];

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AgentTodoPluginSelection {
    pub(crate) installation_id: String,
    pub(crate) plugin_id: String,
    pub(crate) component_key: String,
    pub(crate) component_revision: String,
    pub(crate) release_id: String,
    pub(crate) release_digest: String,
    pub(crate) manifest_version: String,
    pub(crate) display_name: String,
    pub(crate) reason: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AgentTodoExecutionPlan {
    pub(super) requires_execution: bool,
    pub(super) builtin_capabilities: Vec<String>,
    pub(super) plugins: Vec<AgentTodoPluginSelection>,
    selection_revision: String,
    selected_at_unix_ms: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AgentTodoExecutionContract {
    #[serde(default)]
    pub(crate) objective: String,
    #[serde(default)]
    pub(crate) scope: String,
    #[serde(default)]
    pub(crate) expected_outputs: Vec<String>,
    #[serde(default)]
    pub(crate) acceptance_criteria: Vec<String>,
    #[serde(default)]
    pub(crate) constraints: Vec<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentTeamTodo {
    pub(super) id: String,
    agent_id: String,
    pub(super) agent_name: String,
    team_id: String,
    source_room_id: Option<String>,
    source_message_id: Option<String>,
    pub(super) title: String,
    detail: String,
    pub(super) priority: i64,
    sort_order: i64,
    pub(super) status: String,
    pub(super) blocked_reason: String,
    result: String,
    pub(super) execution_contract: AgentTodoExecutionContract,
    pub(super) execution_plan: AgentTodoExecutionPlan,
    pub(super) dependency_ids: Vec<String>,
    created_at_unix_ms: i64,
    pub(super) updated_at_unix_ms: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CreateAgentTeamTodoInput {
    owner_user_id: String,
    team_id: String,
    agent_id: String,
    title: String,
    detail: String,
    priority: i64,
    #[serde(default)]
    dependency_ids: Vec<String>,
    execution_contract: Option<AgentTodoExecutionContract>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct UpdateAgentTeamTodoInput {
    owner_user_id: String,
    team_id: String,
    todo_id: String,
    expected_updated_at_unix_ms: i64,
    title: String,
    detail: String,
    priority: i64,
    #[serde(default)]
    dependency_ids: Vec<String>,
    execution_contract: Option<AgentTodoExecutionContract>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ReorderAgentTeamTodosInput {
    owner_user_id: String,
    team_id: String,
    todo_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CancelAgentTeamTodoInput {
    owner_user_id: String,
    team_id: String,
    todo_id: String,
    expected_updated_at_unix_ms: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RetryAgentTeamRunInput {
    owner_user_id: String,
    team_id: String,
    run_id: String,
}

#[derive(Debug)]
pub(crate) struct TodoPlanningCreateInput {
    pub(crate) request_key: String,
    pub(crate) team_reference: String,
    pub(crate) assignee_reference: String,
    pub(crate) title: String,
    pub(crate) detail: String,
    pub(crate) priority: i64,
    pub(crate) dependency_references: Vec<String>,
    pub(crate) execution_contract: AgentTodoExecutionContract,
    pub(crate) requires_execution: bool,
    pub(crate) builtin_capabilities: Vec<String>,
    pub(crate) plugins: Vec<AgentTodoPluginSelection>,
}

#[derive(Debug)]
pub(crate) struct TodoPlanningUpdateInput {
    pub(crate) todo_reference: String,
    pub(crate) title: Option<String>,
    pub(crate) detail: Option<String>,
    pub(crate) priority: Option<i64>,
    pub(crate) status: Option<String>,
    pub(crate) dependency_references: Option<Vec<String>>,
    pub(crate) objective: Option<String>,
    pub(crate) scope: Option<String>,
    pub(crate) expected_outputs: Option<Vec<String>>,
    pub(crate) acceptance_criteria: Option<Vec<String>>,
    pub(crate) constraints: Option<Vec<String>>,
}

struct StoredTeamTodo {
    id: String,
    agent_id: String,
    agent_name: String,
    team_id: String,
    source_room_id: Option<String>,
    source_message_id: Option<String>,
    title: String,
    detail: String,
    priority: i64,
    sort_order: i64,
    status: String,
    blocked_reason: String,
    result: String,
    execution_contract_json: String,
    execution_plan_json: String,
    created_at_unix_ms: i64,
    updated_at_unix_ms: i64,
}

struct WorkspaceTeamSnapshot {
    id: String,
    project_id: String,
    name: String,
    goal: String,
    project_manager_id: Option<String>,
    members: Vec<Value>,
}

struct ReadyTeamTodo {
    id: String,
    agent_id: String,
    title: String,
    detail: String,
    execution_contract_json: String,
}

#[derive(Clone)]
pub(super) struct ActiveTeamRunBinding {
    pub(super) delivery_id: String,
    pub(super) conversation_id: String,
    pub(super) turn_id: String,
    pub(super) run_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TeamRunControlIntent {
    pub(crate) id: String,
    pub(crate) owner_user_id: String,
    pub(crate) action: String,
    pub(crate) team_id: String,
    pub(crate) todo_id: String,
    pub(crate) delivery_id: String,
    pub(crate) conversation_id: String,
    pub(crate) turn_id: String,
    pub(crate) run_id: String,
}

pub(super) struct TeamRunReconciliation {
    pub(super) delivery_id: String,
    pub(super) turn_id: String,
    pub(super) run_id: String,
    pub(super) status: LocalConversationTurnStatus,
    pub(super) created_at_unix_ms: i64,
    pub(super) updated_at_unix_ms: i64,
    pub(super) project_write_evidence_verified: bool,
}

#[tauri::command]
pub(crate) async fn list_agent_team_todos(
    owner_user_id: String,
    team_id: String,
    include_terminal: bool,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
) -> Result<Vec<AgentTeamTodo>, String> {
    validate_owner(&owner_user_id)?;
    validate_identifier(&team_id).map_err(|_| invalid_team_input())?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let store = conversations.inner().clone();
    let owner = owner_user_id.clone();
    let team = team_id.clone();
    let todos = tauri::async_runtime::spawn_blocking(move || {
        store.list_project_team_todos(&owner, &team, include_terminal)
    })
    .await
    .map_err(|_| "团队任务读取任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    Ok(todos)
}

#[tauri::command]
pub(crate) async fn list_agent_team_todo_progress(
    owner_user_id: String,
    team_id: String,
    todo_id: String,
    limit: usize,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
) -> Result<Vec<Value>, String> {
    validate_owner(&owner_user_id)?;
    validate_identifier(&team_id).map_err(|_| invalid_team_input())?;
    validate_identifier(&todo_id).map_err(|_| invalid_team_todo_input())?;
    if !(1..=500).contains(&limit) {
        return Err(invalid_team_todo_input());
    }
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let store = conversations.inner().clone();
    let owner = owner_user_id.clone();
    let team = team_id.clone();
    let todo = todo_id.clone();
    let progress = tauri::async_runtime::spawn_blocking(move || {
        store.list_project_team_todo_progress(&owner, &team, &todo, limit)
    })
    .await
    .map_err(|_| "团队任务进度读取任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    Ok(progress)
}

#[tauri::command]
pub(crate) async fn create_agent_team_todo(
    input: CreateAgentTeamTodoInput,
    app: AppHandle,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
) -> Result<AgentTeamTodo, String> {
    validate_owner(&input.owner_user_id)?;
    let title = input.title.trim().to_owned();
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let store = conversations.inner().clone();
    let session_owner = input.owner_user_id.clone();
    let owner = session_owner.clone();
    let now = chrono::Utc::now().timestamp_millis().max(0);
    let todo = tauri::async_runtime::spawn_blocking(move || {
        store.create_project_team_todo(
            &owner,
            &input.team_id,
            &input.agent_id,
            &title,
            &input.detail,
            input.priority,
            &input.dependency_ids,
            input.execution_contract,
            now,
        )
    })
    .await
    .map_err(|_| "团队任务创建任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&session_owner, generation)
        .await?;
    crate::attachment_tool_worker::schedule_team_todo_drain(
        app,
        session_owner,
        todo.team_id.clone(),
    );
    Ok(todo)
}

#[tauri::command]
pub(crate) async fn update_agent_team_todo(
    input: UpdateAgentTeamTodoInput,
    app: AppHandle,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
) -> Result<AgentTeamTodo, String> {
    validate_owner(&input.owner_user_id)?;
    let title = input.title.trim().to_owned();
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let store = conversations.inner().clone();
    let session_owner = input.owner_user_id.clone();
    let owner = session_owner.clone();
    let now = chrono::Utc::now().timestamp_millis().max(0);
    let todo = tauri::async_runtime::spawn_blocking(move || {
        store.update_project_team_todo(
            &owner,
            &input.team_id,
            &input.todo_id,
            input.expected_updated_at_unix_ms,
            &title,
            &input.detail,
            input.priority,
            &input.dependency_ids,
            input.execution_contract,
            now,
        )
    })
    .await
    .map_err(|_| "团队任务更新任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&session_owner, generation)
        .await?;
    crate::attachment_tool_worker::schedule_team_todo_drain(
        app,
        session_owner,
        todo.team_id.clone(),
    );
    Ok(todo)
}

#[tauri::command]
pub(crate) async fn reorder_agent_team_todos(
    input: ReorderAgentTeamTodosInput,
    app: AppHandle,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
) -> Result<Vec<AgentTeamTodo>, String> {
    validate_owner(&input.owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let store = conversations.inner().clone();
    let session_owner = input.owner_user_id.clone();
    let owner = session_owner.clone();
    let team_id = input.team_id.clone();
    let schedule_team_id = team_id.clone();
    let now = chrono::Utc::now().timestamp_millis().max(0);
    let todos = tauri::async_runtime::spawn_blocking(move || {
        store.reorder_project_team_todos(&owner, &team_id, &input.todo_ids, now)
    })
    .await
    .map_err(|_| "团队任务排序任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&session_owner, generation)
        .await?;
    crate::attachment_tool_worker::schedule_team_todo_drain(app, session_owner, schedule_team_id);
    Ok(todos)
}

pub(super) async fn cancel_active_team_run(
    manager: &HostManager,
    owner_user_id: &str,
    conversation_id: &str,
    turn_id: &str,
    run_id: &str,
    reason: &str,
) -> Result<LocalConversationTurnRecord, String> {
    let load_conversation = || {
        manager.execute(HostCommand::GetConversationContext(
            GetConversationCommand {
                owner_user_id: owner_user_id.to_owned(),
                conversation_id: conversation_id.to_owned(),
            },
        ))
    };
    let result = load_conversation().await?;
    let HostResult::Conversation { conversation } = result else {
        return Err("Local Agent Host 返回了错误的团队运行响应。".into());
    };
    if conversation.conversation.owner_user_id != owner_user_id
        || conversation.conversation.conversation_id != conversation_id
    {
        return Err("Local Agent Host 返回了不匹配的团队运行。".into());
    }
    let turn = conversation
        .turns
        .iter()
        .find(|turn| turn.turn_id == turn_id && turn.run_id == run_id)
        .cloned()
        .ok_or_else(|| "Local Agent Host 中找不到待停止的团队运行。".to_owned())?;
    if turn.status != LocalConversationTurnStatus::Running {
        return Ok(turn);
    }
    let run_result = manager
        .execute(HostCommand::GetRun(GetRunCommand {
            owner_user_id: owner_user_id.to_owned(),
            run_id: run_id.to_owned(),
        }))
        .await?;
    let HostResult::Run { run } = run_result else {
        return Err("Local Agent Host 返回了错误的 Run 响应。".into());
    };
    if run.owner_user_id != owner_user_id || run.run_id != run_id {
        return Err("Local Agent Host 返回了不匹配的 Run。".into());
    }
    let cancelled = manager
        .execute_detailed(HostCommand::CancelConversationTurn(
            CancelConversationTurnCommand {
                owner_user_id: owner_user_id.to_owned(),
                conversation_id: conversation_id.to_owned(),
                expected_conversation_version: conversation.conversation.version,
                turn_id: turn_id.to_owned(),
                expected_run_version: Some(run.version),
                reason: reason.to_owned(),
            },
        ))
        .await;
    match cancelled {
        Ok(HostResult::ConversationTurnUpdated { result }) => {
            if result.conversation.owner_user_id != owner_user_id
                || result.conversation.conversation_id != conversation_id
                || result.turn.turn_id != turn_id
                || result.turn.run_id != run_id
                || result.turn.status == LocalConversationTurnStatus::Running
            {
                return Err("Local Agent Host 返回了不匹配的停止结果。".into());
            }
            Ok(result.turn)
        }
        Ok(_) => Err("Local Agent Host 返回了错误的停止响应。".into()),
        Err(original) => {
            // The cancellation may have committed before the transport failed, or the Run may
            // have reached a terminal state concurrently. Read the authoritative Host state
            // before deciding whether this journal intent still needs replay.
            let retry = load_conversation()
                .await
                .map_err(|_| original.message.clone())?;
            let HostResult::Conversation { conversation } = retry else {
                return Err(original.message);
            };
            conversation
                .turns
                .into_iter()
                .find(|turn| {
                    turn.turn_id == turn_id
                        && turn.run_id == run_id
                        && turn.status != LocalConversationTurnStatus::Running
                })
                .ok_or(original.message)
        }
    }
}

async fn execute_team_run_cancel_intent(
    manager: &HostManager,
    intent: &TeamRunControlIntent,
) -> Result<LocalConversationTurnRecord, String> {
    cancel_active_team_run(
        manager,
        &intent.owner_user_id,
        &intent.conversation_id,
        &intent.turn_id,
        &intent.run_id,
        "用户已停止这个团队 Todo。",
    )
    .await
}

async fn finish_team_run_cancel_intent(
    store: AgentConversationStore,
    intent: TeamRunControlIntent,
    turn: LocalConversationTurnRecord,
) -> Result<AgentTeamTodo, String> {
    tauri::async_runtime::spawn_blocking(move || {
        store.complete_team_run_cancel_intent(&intent, &turn)
    })
    .await
    .map_err(|_| "团队任务停止对账任务异常退出。".to_owned())?
}

struct TeamRunRetryOutcome {
    turn: LocalConversationTurnRecord,
    run_status: LocalAgentRunStatus,
}

async fn execute_team_run_retry_intent(
    manager: &HostManager,
    intent: &TeamRunControlIntent,
    allow_already_resumed: bool,
) -> Result<TeamRunRetryOutcome, String> {
    let load_state = || async {
        let conversation_result = manager
            .execute(HostCommand::GetConversationContext(
                GetConversationCommand {
                    owner_user_id: intent.owner_user_id.clone(),
                    conversation_id: intent.conversation_id.clone(),
                },
            ))
            .await?;
        let HostResult::Conversation { conversation } = conversation_result else {
            return Err("Local Agent Host 返回了错误的团队运行响应。".to_owned());
        };
        if conversation.conversation.owner_user_id != intent.owner_user_id
            || conversation.conversation.conversation_id != intent.conversation_id
        {
            return Err("Local Agent Host 返回了不匹配的团队运行。".to_owned());
        }
        let turn = conversation
            .turns
            .iter()
            .find(|turn| turn.turn_id == intent.turn_id && turn.run_id == intent.run_id)
            .cloned()
            .ok_or_else(|| "Local Agent Host 中找不到待重试的团队运行。".to_owned())?;
        let run_result = manager
            .execute(HostCommand::GetRun(GetRunCommand {
                owner_user_id: intent.owner_user_id.clone(),
                run_id: intent.run_id.clone(),
            }))
            .await?;
        let HostResult::Run { run } = run_result else {
            return Err("Local Agent Host 返回了错误的 Run 响应。".to_owned());
        };
        if run.owner_user_id != intent.owner_user_id || run.run_id != intent.run_id {
            return Err("Local Agent Host 返回了不匹配的 Run。".to_owned());
        }
        Ok((conversation.conversation.version, turn, run))
    };
    let (conversation_version, turn, run) = load_state().await?;
    if run.status.is_terminal() {
        let mut terminal_turn = turn;
        terminal_turn.status = match run.status {
            LocalAgentRunStatus::Succeeded => LocalConversationTurnStatus::Succeeded,
            LocalAgentRunStatus::Failed => LocalConversationTurnStatus::Failed,
            LocalAgentRunStatus::Cancelled => LocalConversationTurnStatus::Cancelled,
            _ => unreachable!(),
        };
        terminal_turn.updated_at_unix_ms =
            terminal_turn.updated_at_unix_ms.max(run.updated_at_unix_ms);
        return Ok(TeamRunRetryOutcome {
            turn: terminal_turn,
            run_status: run.status,
        });
    }
    if turn.status != LocalConversationTurnStatus::Running {
        return Ok(TeamRunRetryOutcome {
            turn,
            run_status: run.status,
        });
    }
    let retryable = matches!(
        run.status,
        LocalAgentRunStatus::WaitingUser
            | LocalAgentRunStatus::Paused
            | LocalAgentRunStatus::NeedsReview
    );
    if !retryable {
        return if allow_already_resumed {
            Ok(TeamRunRetryOutcome {
                turn,
                run_status: run.status,
            })
        } else {
            Err("这个 Host Run 当前不需要 Human 重试。".into())
        };
    }
    let resumed = manager
        .execute_detailed(HostCommand::ResumeConversationTurn(
            ResumeConversationTurnCommand {
                owner_user_id: intent.owner_user_id.clone(),
                conversation_id: intent.conversation_id.clone(),
                expected_conversation_version: conversation_version,
                turn_id: intent.turn_id.clone(),
                expected_run_version: run.version,
                expected_run_status: run.status,
                message_id: intent.id.clone(),
                message: "继续执行这个团队 Todo，并先核验中断步骤是否已经产生副作用。".into(),
                message_metadata: json!({
                    "type": "human_retry_authorization",
                    "team_id": intent.team_id,
                    "todo_id": intent.todo_id,
                    "delivery_id": intent.delivery_id,
                }),
                attachments: Vec::new(),
                reason: "Human 已在运行详情中明确授权重试中断步骤。".into(),
            },
        ))
        .await;
    match resumed {
        Ok(HostResult::ConversationTurnUpdated { result }) => {
            if result.conversation.owner_user_id != intent.owner_user_id
                || result.conversation.conversation_id != intent.conversation_id
                || result.turn.turn_id != intent.turn_id
                || result.turn.run_id != intent.run_id
                || result.run.run_id != intent.run_id
            {
                return Err("Local Agent Host 返回了不匹配的重试结果。".into());
            }
            Ok(TeamRunRetryOutcome {
                turn: result.turn,
                run_status: result.run.status,
            })
        }
        Ok(_) => Err("Local Agent Host 返回了错误的重试响应。".into()),
        Err(original) => {
            // A response can be lost after Host committed the resume transaction. Re-read the
            // authoritative Run; leaving the review state proves that the stable journaled
            // message id was accepted, so local reconciliation may safely complete.
            let (_, turn, run) = load_state().await.map_err(|_| original.message.clone())?;
            if turn.status != LocalConversationTurnStatus::Running
                || !matches!(
                    run.status,
                    LocalAgentRunStatus::WaitingUser
                        | LocalAgentRunStatus::Paused
                        | LocalAgentRunStatus::NeedsReview
                )
            {
                Ok(TeamRunRetryOutcome {
                    turn,
                    run_status: run.status,
                })
            } else {
                Err(original.message)
            }
        }
    }
}

async fn finish_team_run_retry_intent(
    store: AgentConversationStore,
    intent: TeamRunControlIntent,
    outcome: TeamRunRetryOutcome,
) -> Result<AgentTeamTodo, String> {
    let now = chrono::Utc::now().timestamp_millis().max(0);
    tauri::async_runtime::spawn_blocking(move || {
        store.complete_team_run_retry_intent(&intent, &outcome.turn, outcome.run_status, now)
    })
    .await
    .map_err(|_| "团队任务重试对账任务异常退出。".to_owned())?
}

pub(crate) async fn recover_agent_team_run_control_journal(
    manager: &HostManager,
    conversations: &AgentConversationStore,
    owner_user_id: &str,
) -> Result<usize, String> {
    let store = conversations.clone();
    let owner = owner_user_id.to_owned();
    let intents = tauri::async_runtime::spawn_blocking(move || {
        store.pending_team_run_control_intents(&owner)
    })
    .await
    .map_err(|_| "团队运行恢复日志读取任务异常退出。".to_owned())??;
    let mut completed = 0;
    let mut first_error = None;
    for intent in intents {
        let result = async {
            match intent.action.as_str() {
                "cancel" => {
                    let turn = execute_team_run_cancel_intent(manager, &intent).await?;
                    finish_team_run_cancel_intent(conversations.clone(), intent, turn).await?;
                }
                "retry" => {
                    let outcome = execute_team_run_retry_intent(manager, &intent, true).await?;
                    finish_team_run_retry_intent(conversations.clone(), intent, outcome).await?;
                }
                _ => return Err(team_todo_store_error()),
            }
            Ok::<(), String>(())
        }
        .await;
        match result {
            Ok(()) => completed += 1,
            Err(error) if first_error.is_none() => first_error = Some(error),
            Err(_) => {}
        }
    }
    if let Some(error) = first_error {
        return Err(error);
    }
    Ok(completed)
}

#[tauri::command]
pub(crate) async fn retry_agent_team_run(
    input: RetryAgentTeamRunInput,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
    manager: State<'_, HostManager>,
) -> Result<AgentTeamTodo, String> {
    validate_owner(&input.owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let store = conversations.inner().clone();
    let owner = input.owner_user_id.clone();
    let team_id = input.team_id.clone();
    let run_id = input.run_id.clone();
    let now = chrono::Utc::now().timestamp_millis().max(0);
    let intent_store = store.clone();
    let intent = tauri::async_runtime::spawn_blocking(move || {
        intent_store.record_project_team_run_retry_intent(&owner, &team_id, &run_id, now)
    })
    .await
    .map_err(|_| "团队任务重试日志写入任务异常退出。".to_owned())??;
    let outcome = execute_team_run_retry_intent(manager.inner(), &intent, false).await?;
    let todo = finish_team_run_retry_intent(store, intent, outcome).await?;
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    Ok(todo)
}

#[tauri::command]
pub(crate) async fn cancel_agent_team_todo(
    input: CancelAgentTeamTodoInput,
    app: AppHandle,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
    manager: State<'_, HostManager>,
) -> Result<AgentTeamTodo, String> {
    validate_owner(&input.owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let store = conversations.inner().clone();
    let session_owner = input.owner_user_id.clone();
    let owner = session_owner.clone();
    let team_id = input.team_id.clone();
    let schedule_team_id = team_id.clone();
    let todo_id = input.todo_id.clone();
    let expected_updated_at_unix_ms = input.expected_updated_at_unix_ms;
    let now = chrono::Utc::now().timestamp_millis().max(0);
    let preparation_store = store.clone();
    let intent = tauri::async_runtime::spawn_blocking(move || {
        preparation_store.record_project_team_todo_cancel_intent(
            &owner,
            &team_id,
            &todo_id,
            expected_updated_at_unix_ms,
            now,
        )
    })
    .await
    .map_err(|_| "团队任务停止日志写入任务异常退出。".to_owned())??;
    let todo = if let Some(intent) = intent {
        let turn = execute_team_run_cancel_intent(manager.inner(), &intent).await?;
        finish_team_run_cancel_intent(store, intent, turn).await?
    } else {
        let owner = input.owner_user_id.clone();
        let team_id = input.team_id.clone();
        let todo_id = input.todo_id.clone();
        tauri::async_runtime::spawn_blocking(move || {
            store.cancel_project_team_todo(
                &owner,
                &team_id,
                &todo_id,
                input.expected_updated_at_unix_ms,
                now,
            )
        })
        .await
        .map_err(|_| "团队任务取消任务异常退出。".to_owned())??
    };
    authentication
        .require_session_generation(&session_owner, generation)
        .await?;
    crate::attachment_tool_worker::schedule_team_todo_drain(app, session_owner, schedule_team_id);
    Ok(todo)
}

#[cfg(test)]
mod capability_tests {
    use super::*;

    #[test]
    fn requirement_survey_write_completes_its_read_dependency() {
        let normalized =
            normalized_planning_capabilities(&["requirement_survey_write".to_owned()], true)
                .unwrap();
        assert_eq!(
            normalized,
            vec![
                "requirement_survey_read".to_owned(),
                "requirement_survey_write".to_owned(),
            ]
        );
        assert!(
            normalized_planning_capabilities(&["requirement_survey_read".to_owned()], false,)
                .is_err()
        );
    }

    #[test]
    fn persisted_requirement_survey_plan_requires_the_read_dependency() {
        let mut plan = AgentTodoExecutionPlan {
            requires_execution: true,
            builtin_capabilities: vec![
                "requirement_survey_read".into(),
                "requirement_survey_write".into(),
            ],
            plugins: Vec::new(),
            selection_revision: "requirement-survey-v1".into(),
            selected_at_unix_ms: 1,
        };
        assert!(validate_execution_plan(&plan).is_ok());
        plan.builtin_capabilities = vec!["requirement_survey_write".into()];
        assert!(validate_execution_plan(&plan).is_err());
    }
}
