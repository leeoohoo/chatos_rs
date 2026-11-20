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
mod run_control_runtime;
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
pub(super) use run_control_runtime::cancel_active_team_run;
use run_control_runtime::{
    execute_team_run_cancel_intent, execute_team_run_retry_intent, finish_team_run_cancel_intent,
    finish_team_run_retry_intent,
};
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
