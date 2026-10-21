use super::*;
use std::collections::HashSet;

const MAXIMUM_TEAM_TODOS: usize = 1_000;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AgentTodoPluginSelection {
    plugin_id: String,
    component_key: String,
    display_name: String,
    reason: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AgentTodoExecutionPlan {
    requires_execution: bool,
    pub(super) builtin_capabilities: Vec<String>,
    plugins: Vec<AgentTodoPluginSelection>,
    selection_revision: String,
    selected_at_unix_ms: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AgentTodoExecutionContract {
    #[serde(default)]
    pub(super) objective: String,
    #[serde(default)]
    scope: String,
    #[serde(default)]
    expected_outputs: Vec<String>,
    #[serde(default)]
    acceptance_criteria: Vec<String>,
    #[serde(default)]
    constraints: Vec<String>,
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
    title: String,
    detail: String,
    priority: i64,
    sort_order: i64,
    status: String,
    blocked_reason: String,
    result: String,
    pub(super) execution_contract: AgentTodoExecutionContract,
    pub(super) execution_plan: AgentTodoExecutionPlan,
    pub(super) dependency_ids: Vec<String>,
    created_at_unix_ms: i64,
    updated_at_unix_ms: i64,
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

impl AgentConversationStore {
    pub(super) fn list_project_team_todos(
        &self,
        owner_user_id: &str,
        team_id: &str,
        include_terminal: bool,
    ) -> Result<Vec<AgentTeamTodo>, String> {
        validate_identifier(team_id).map_err(|_| invalid_team_input())?;
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        require_active_project_team(&connection, owner_user_id, team_id)?;
        let terminal_filter = if include_terminal {
            ""
        } else {
            "AND todo.status NOT IN ('completed', 'cancelled')"
        };
        let sql = format!(
            r#"
            SELECT todo.id, todo.agent_id, profile.name, todo.team_room_id,
                   todo.source_room_id, todo.source_message_id, todo.title, todo.detail,
                   todo.priority, todo.sort_order, todo.status, todo.blocked_reason,
                   todo.result, todo.execution_contract_json, todo.execution_plan_json,
                   todo.created_at_unix_ms, todo.updated_at_unix_ms
            FROM local_agent_todos todo
            JOIN local_agent_profiles profile
              ON profile.owner_user_id = todo.owner_user_id
             AND profile.id = todo.agent_id
            JOIN project_agent_room_members member
              ON member.owner_user_id = todo.owner_user_id
             AND member.room_id = todo.team_room_id
             AND member.agent_id = todo.agent_id
            WHERE todo.owner_user_id = ?1 AND todo.team_room_id = ?2
              AND member.status = 'active' {terminal_filter}
            ORDER BY
              CASE todo.status
                WHEN 'in_progress' THEN 0
                WHEN 'blocked' THEN 1
                WHEN 'pending' THEN 2
                WHEN 'completed' THEN 3
                ELSE 4
              END,
              todo.priority DESC, todo.sort_order, todo.created_at_unix_ms, todo.id
            LIMIT {MAXIMUM_TEAM_TODOS}
            "#,
        );
        let stored = {
            let mut statement = connection
                .prepare(&sql)
                .map_err(|_| team_todo_store_error())?;
            let rows = statement
                .query_map(params![owner_user_id, team_id], |row| {
                    Ok(StoredTeamTodo {
                        id: row.get(0)?,
                        agent_id: row.get(1)?,
                        agent_name: row.get(2)?,
                        team_id: row.get(3)?,
                        source_room_id: row.get(4)?,
                        source_message_id: row.get(5)?,
                        title: row.get(6)?,
                        detail: row.get(7)?,
                        priority: row.get(8)?,
                        sort_order: row.get(9)?,
                        status: row.get(10)?,
                        blocked_reason: row.get(11)?,
                        result: row.get(12)?,
                        execution_contract_json: row.get(13)?,
                        execution_plan_json: row.get(14)?,
                        created_at_unix_ms: row.get(15)?,
                        updated_at_unix_ms: row.get(16)?,
                    })
                })
                .map_err(|_| team_todo_store_error())?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|_| team_todo_store_error())?
        };
        let dependencies = load_dependencies(&connection, owner_user_id, team_id)?;
        stored
            .into_iter()
            .map(|todo| {
                let dependency_ids = dependencies
                    .iter()
                    .filter(|(todo_id, _)| todo_id == &todo.id)
                    .map(|(_, prerequisite_id)| prerequisite_id.clone())
                    .collect();
                project_team_todo(todo, dependency_ids)
            })
            .collect()
    }
}

fn load_dependencies(
    connection: &Connection,
    owner_user_id: &str,
    team_id: &str,
) -> Result<Vec<(String, String)>, String> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT dependency.todo_id, dependency.prerequisite_todo_id
            FROM local_agent_todo_dependencies dependency
            JOIN local_agent_todos todo
              ON todo.owner_user_id = dependency.owner_user_id
             AND todo.id = dependency.todo_id
            JOIN local_agent_todos prerequisite
              ON prerequisite.owner_user_id = dependency.owner_user_id
             AND prerequisite.id = dependency.prerequisite_todo_id
            WHERE dependency.owner_user_id = ?1
              AND todo.team_room_id = ?2
              AND prerequisite.team_room_id = ?2
            ORDER BY dependency.todo_id, dependency.created_at_unix_ms,
                     dependency.prerequisite_todo_id
            "#,
        )
        .map_err(|_| team_todo_store_error())?;
    let rows = statement
        .query_map(params![owner_user_id, team_id], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .map_err(|_| team_todo_store_error())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|_| team_todo_store_error())
}

fn project_team_todo(
    stored: StoredTeamTodo,
    dependency_ids: Vec<String>,
) -> Result<AgentTeamTodo, String> {
    let execution_contract: AgentTodoExecutionContract =
        serde_json::from_str(&stored.execution_contract_json)
            .map_err(|_| team_todo_store_error())?;
    let execution_plan: AgentTodoExecutionPlan =
        serde_json::from_str(&stored.execution_plan_json).map_err(|_| team_todo_store_error())?;
    let todo = AgentTeamTodo {
        id: stored.id,
        agent_id: stored.agent_id,
        agent_name: stored.agent_name,
        team_id: stored.team_id,
        source_room_id: stored.source_room_id,
        source_message_id: stored.source_message_id,
        title: stored.title,
        detail: stored.detail,
        priority: stored.priority,
        sort_order: stored.sort_order,
        status: stored.status,
        blocked_reason: stored.blocked_reason,
        result: stored.result,
        execution_contract,
        execution_plan,
        dependency_ids,
        created_at_unix_ms: stored.created_at_unix_ms,
        updated_at_unix_ms: stored.updated_at_unix_ms,
    };
    validate_team_todo(&todo)?;
    Ok(todo)
}

fn validate_team_todo(todo: &AgentTeamTodo) -> Result<(), String> {
    validate_identifier(&todo.id).map_err(|_| team_todo_store_error())?;
    validate_agent_identifier(&todo.agent_id).map_err(|_| team_todo_store_error())?;
    validate_text(&todo.agent_name, 120, false).map_err(|_| team_todo_store_error())?;
    validate_identifier(&todo.team_id).map_err(|_| team_todo_store_error())?;
    if let Some(room_id) = todo.source_room_id.as_deref() {
        validate_identifier(room_id).map_err(|_| team_todo_store_error())?;
    }
    if let Some(message_id) = todo.source_message_id.as_deref() {
        validate_identifier(message_id).map_err(|_| team_todo_store_error())?;
    }
    validate_text(&todo.title, 500, false).map_err(|_| team_todo_store_error())?;
    validate_text(&todo.detail, 16_000, true).map_err(|_| team_todo_store_error())?;
    validate_text(&todo.blocked_reason, 8_000, true).map_err(|_| team_todo_store_error())?;
    validate_text(&todo.result, 16_000, true).map_err(|_| team_todo_store_error())?;
    if !(0..=100).contains(&todo.priority)
        || todo.sort_order < 0
        || !matches!(
            todo.status.as_str(),
            "pending" | "in_progress" | "blocked" | "completed" | "cancelled"
        )
        || todo.created_at_unix_ms < 0
        || todo.updated_at_unix_ms < todo.created_at_unix_ms
        || todo.dependency_ids.len() > 64
    {
        return Err(team_todo_store_error());
    }
    let mut dependencies = HashSet::new();
    for dependency_id in &todo.dependency_ids {
        validate_identifier(dependency_id).map_err(|_| team_todo_store_error())?;
        if dependency_id == &todo.id || !dependencies.insert(dependency_id) {
            return Err(team_todo_store_error());
        }
    }
    validate_execution_contract(&todo.execution_contract)?;
    validate_execution_plan(&todo.execution_plan)
}

fn validate_execution_contract(contract: &AgentTodoExecutionContract) -> Result<(), String> {
    validate_text(&contract.objective, 8_000, false).map_err(|_| team_todo_store_error())?;
    validate_text(&contract.scope, 16_000, true).map_err(|_| team_todo_store_error())?;
    validate_contract_list(&contract.expected_outputs, false)?;
    validate_contract_list(&contract.acceptance_criteria, false)?;
    validate_contract_list(&contract.constraints, true)
}

fn validate_contract_list(values: &[String], allow_empty: bool) -> Result<(), String> {
    if values.len() > 64 || (!allow_empty && values.is_empty()) {
        return Err(team_todo_store_error());
    }
    for value in values {
        validate_text(value, 4_000, false).map_err(|_| team_todo_store_error())?;
    }
    Ok(())
}

fn validate_execution_plan(plan: &AgentTodoExecutionPlan) -> Result<(), String> {
    if plan.selected_at_unix_ms < 0
        || plan.builtin_capabilities.len() > 3
        || plan.plugins.len() > 32
    {
        return Err(team_todo_store_error());
    }
    validate_identifier(&plan.selection_revision).map_err(|_| team_todo_store_error())?;
    let mut capabilities = HashSet::new();
    for capability in &plan.builtin_capabilities {
        if !matches!(
            capability.as_str(),
            "project_read" | "project_write" | "terminal"
        ) || !capabilities.insert(capability)
            || (!plan.requires_execution && capability != "project_read")
        {
            return Err(team_todo_store_error());
        }
    }
    let mut plugins = HashSet::new();
    for plugin in &plan.plugins {
        validate_identifier(&plugin.plugin_id).map_err(|_| team_todo_store_error())?;
        validate_identifier(&plugin.component_key).map_err(|_| team_todo_store_error())?;
        validate_text(&plugin.display_name, 240, false).map_err(|_| team_todo_store_error())?;
        validate_text(&plugin.reason, 1_000, true).map_err(|_| team_todo_store_error())?;
        if !plugins.insert((&plugin.plugin_id, &plugin.component_key)) {
            return Err(team_todo_store_error());
        }
    }
    Ok(())
}

fn team_todo_store_error() -> String {
    "团队任务数据不可用，请检查本机存储后重试。".into()
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
