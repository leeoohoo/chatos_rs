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

pub(super) struct TeamRunReconciliation {
    pub(super) delivery_id: String,
    pub(super) turn_id: String,
    pub(super) run_id: String,
    pub(super) status: LocalConversationTurnStatus,
    pub(super) created_at_unix_ms: i64,
    pub(super) updated_at_unix_ms: i64,
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

    #[allow(clippy::too_many_arguments)]
    pub(super) fn create_project_team_todo(
        &self,
        owner_user_id: &str,
        team_id: &str,
        agent_id: &str,
        title: &str,
        detail: &str,
        priority: i64,
        dependency_ids: &[String],
        execution_contract: Option<AgentTodoExecutionContract>,
        now_unix_ms: i64,
    ) -> Result<AgentTeamTodo, String> {
        validate_todo_mutation(
            team_id,
            Some(agent_id),
            title,
            detail,
            priority,
            dependency_ids,
            now_unix_ms,
        )?;
        let contract = normalized_contract(execution_contract, title, detail);
        validate_execution_contract(&contract).map_err(|_| invalid_team_todo_input())?;
        reject_link_or_non_file(&self.database)?;
        let mut connection = open_database(&self.database, false)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| team_todo_store_error())?;
        require_active_project_team(&transaction, owner_user_id, team_id)?;
        require_active_team_member(&transaction, owner_user_id, team_id, agent_id)?;
        validate_dependencies(&transaction, owner_user_id, team_id, None, dependency_ids)?;
        let sort_order = transaction
            .query_row(
                "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM local_agent_todos \
                 WHERE owner_user_id = ?1 AND agent_id = ?2",
                params![owner_user_id, agent_id],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|_| team_todo_store_error())?;
        let todo_id = identifier("todo");
        let execution_plan = default_execution_plan(now_unix_ms);
        let execution_plan_json =
            serde_json::to_string(&execution_plan).map_err(|_| team_todo_store_error())?;
        let execution_contract_json =
            serde_json::to_string(&contract).map_err(|_| team_todo_store_error())?;
        transaction
            .execute(
                r#"
                INSERT INTO local_agent_todos (
                    owner_user_id, id, agent_id, team_room_id, source_room_id,
                    source_message_id, request_key, title, detail, priority, sort_order,
                    status, blocked_reason, result, created_at_unix_ms, updated_at_unix_ms,
                    execution_plan_json, execution_contract_json
                ) VALUES (?1, ?2, ?3, ?4, NULL, NULL, ?5, ?6, ?7, ?8, ?9,
                          'pending', '', '', ?10, ?10, ?11, ?12)
                "#,
                params![
                    owner_user_id,
                    todo_id,
                    agent_id,
                    team_id,
                    format!("human:{todo_id}"),
                    title,
                    detail,
                    priority,
                    sort_order,
                    now_unix_ms,
                    execution_plan_json,
                    execution_contract_json,
                ],
            )
            .map_err(|_| team_todo_store_error())?;
        replace_dependencies(
            &transaction,
            owner_user_id,
            &todo_id,
            dependency_ids,
            now_unix_ms,
        )?;
        transaction.commit().map_err(|_| team_todo_store_error())?;
        self.load_project_team_todo(owner_user_id, team_id, &todo_id)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn update_project_team_todo(
        &self,
        owner_user_id: &str,
        team_id: &str,
        todo_id: &str,
        expected_updated_at_unix_ms: i64,
        title: &str,
        detail: &str,
        priority: i64,
        dependency_ids: &[String],
        execution_contract: Option<AgentTodoExecutionContract>,
        now_unix_ms: i64,
    ) -> Result<AgentTeamTodo, String> {
        validate_identifier(todo_id).map_err(|_| invalid_team_todo_input())?;
        validate_todo_mutation(
            team_id,
            None,
            title,
            detail,
            priority,
            dependency_ids,
            now_unix_ms,
        )?;
        if expected_updated_at_unix_ms < 0 {
            return Err(invalid_team_todo_input());
        }
        reject_link_or_non_file(&self.database)?;
        let mut connection = open_database(&self.database, false)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| team_todo_store_error())?;
        require_active_project_team(&transaction, owner_user_id, team_id)?;
        validate_dependencies(
            &transaction,
            owner_user_id,
            team_id,
            Some(todo_id),
            dependency_ids,
        )?;
        let existing_contract_json: Option<String> = transaction
            .query_row(
                "SELECT execution_contract_json FROM local_agent_todos \
                 WHERE owner_user_id = ?1 AND team_room_id = ?2 AND id = ?3",
                params![owner_user_id, team_id, todo_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| team_todo_store_error())?;
        let Some(existing_contract_json) = existing_contract_json else {
            return Err("团队任务不存在。".into());
        };
        let contract = if let Some(contract) = execution_contract {
            contract
        } else {
            serde_json::from_str(&existing_contract_json).map_err(|_| team_todo_store_error())?
        };
        validate_execution_contract(&contract).map_err(|_| invalid_team_todo_input())?;
        let contract_json =
            serde_json::to_string(&contract).map_err(|_| team_todo_store_error())?;
        let updated_at_unix_ms = now_unix_ms.max(expected_updated_at_unix_ms.saturating_add(1));
        let changed = transaction
            .execute(
                r#"
                UPDATE local_agent_todos
                SET title = ?1, detail = ?2, priority = ?3,
                    execution_contract_json = ?4, updated_at_unix_ms = ?5
                WHERE owner_user_id = ?6 AND team_room_id = ?7 AND id = ?8
                  AND updated_at_unix_ms = ?9
                "#,
                params![
                    title,
                    detail,
                    priority,
                    contract_json,
                    updated_at_unix_ms,
                    owner_user_id,
                    team_id,
                    todo_id,
                    expected_updated_at_unix_ms,
                ],
            )
            .map_err(|_| team_todo_store_error())?;
        if changed != 1 {
            return Err(team_todo_conflict());
        }
        transaction
            .execute(
                "DELETE FROM local_agent_todo_dependencies \
                 WHERE owner_user_id = ?1 AND todo_id = ?2",
                params![owner_user_id, todo_id],
            )
            .map_err(|_| team_todo_store_error())?;
        replace_dependencies(
            &transaction,
            owner_user_id,
            todo_id,
            dependency_ids,
            updated_at_unix_ms,
        )?;
        transaction.commit().map_err(|_| team_todo_store_error())?;
        self.load_project_team_todo(owner_user_id, team_id, todo_id)
    }

    pub(super) fn reorder_project_team_todos(
        &self,
        owner_user_id: &str,
        team_id: &str,
        todo_ids: &[String],
        now_unix_ms: i64,
    ) -> Result<Vec<AgentTeamTodo>, String> {
        validate_identifier(team_id).map_err(|_| invalid_team_todo_input())?;
        validate_identifier_list(todo_ids, 500)?;
        if now_unix_ms < 0 {
            return Err(invalid_team_todo_input());
        }
        reject_link_or_non_file(&self.database)?;
        let mut connection = open_database(&self.database, false)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| team_todo_store_error())?;
        require_active_project_team(&transaction, owner_user_id, team_id)?;
        let active_ids = {
            let mut statement = transaction
                .prepare(
                    "SELECT id FROM local_agent_todos WHERE owner_user_id = ?1 \
                     AND team_room_id = ?2 AND status NOT IN ('completed', 'cancelled') \
                     ORDER BY priority DESC, sort_order, created_at_unix_ms, id",
                )
                .map_err(|_| team_todo_store_error())?;
            let rows = statement
                .query_map(params![owner_user_id, team_id], |row| {
                    row.get::<_, String>(0)
                })
                .map_err(|_| team_todo_store_error())?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|_| team_todo_store_error())?
        };
        let active_set: HashSet<&str> = active_ids.iter().map(String::as_str).collect();
        if todo_ids.iter().any(|id| !active_set.contains(id.as_str())) {
            return Err("只能排序当前团队中的未完成任务。".into());
        }
        let requested: HashSet<&str> = todo_ids.iter().map(String::as_str).collect();
        let ordered = todo_ids
            .iter()
            .cloned()
            .chain(
                active_ids
                    .into_iter()
                    .filter(|id| !requested.contains(id.as_str())),
            )
            .collect::<Vec<_>>();
        for (offset, todo_id) in ordered.iter().enumerate() {
            let priority = 100_i64.saturating_sub(offset as i64).max(0);
            let changed = transaction
                .execute(
                    r#"
                    UPDATE local_agent_todos
                    SET priority = ?1, sort_order = ?2,
                        updated_at_unix_ms = CASE
                          WHEN updated_at_unix_ms >= ?3 THEN updated_at_unix_ms + 1
                          ELSE ?3
                        END
                    WHERE owner_user_id = ?4 AND team_room_id = ?5 AND id = ?6
                      AND status NOT IN ('completed', 'cancelled')
                    "#,
                    params![
                        priority,
                        offset as i64,
                        now_unix_ms,
                        owner_user_id,
                        team_id,
                        todo_id,
                    ],
                )
                .map_err(|_| team_todo_store_error())?;
            if changed != 1 {
                return Err(team_todo_conflict());
            }
        }
        transaction.commit().map_err(|_| team_todo_store_error())?;
        self.list_project_team_todos(owner_user_id, team_id, false)
    }

    pub(super) fn cancel_project_team_todo(
        &self,
        owner_user_id: &str,
        team_id: &str,
        todo_id: &str,
        expected_updated_at_unix_ms: i64,
        now_unix_ms: i64,
    ) -> Result<AgentTeamTodo, String> {
        validate_identifier(team_id).map_err(|_| invalid_team_todo_input())?;
        validate_identifier(todo_id).map_err(|_| invalid_team_todo_input())?;
        if expected_updated_at_unix_ms < 0 || now_unix_ms < 0 {
            return Err(invalid_team_todo_input());
        }
        reject_link_or_non_file(&self.database)?;
        let mut connection = open_database(&self.database, false)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| team_todo_store_error())?;
        require_active_project_team(&transaction, owner_user_id, team_id)?;
        let state: Option<String> = transaction
            .query_row(
                "SELECT status FROM local_agent_todos WHERE owner_user_id = ?1 \
                 AND team_room_id = ?2 AND id = ?3 AND updated_at_unix_ms = ?4",
                params![owner_user_id, team_id, todo_id, expected_updated_at_unix_ms],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| team_todo_store_error())?;
        let Some(state) = state else {
            return Err(team_todo_conflict());
        };
        if state == "in_progress" || todo_has_active_host_run(&transaction, owner_user_id, todo_id)?
        {
            return Err(
                "任务正在 Host 中执行；安全停止需要先完成跨 Host 恢复日志，当前未伪装为已取消。"
                    .into(),
            );
        }
        if !matches!(state.as_str(), "pending" | "blocked") {
            return Err("只有待处理或已阻塞的任务可以直接取消。".into());
        }
        let updated_at_unix_ms = now_unix_ms.max(expected_updated_at_unix_ms.saturating_add(1));
        let changed = transaction
            .execute(
                "UPDATE local_agent_todos SET status = 'cancelled', updated_at_unix_ms = ?1 \
                 WHERE owner_user_id = ?2 AND team_room_id = ?3 AND id = ?4 \
                 AND updated_at_unix_ms = ?5 AND status IN ('pending', 'blocked')",
                params![
                    updated_at_unix_ms,
                    owner_user_id,
                    team_id,
                    todo_id,
                    expected_updated_at_unix_ms,
                ],
            )
            .map_err(|_| team_todo_store_error())?;
        if changed != 1 {
            return Err(team_todo_conflict());
        }
        transaction
            .execute(
                r#"
                UPDATE project_agent_deliveries
                SET status = 'cancelled', last_error = 'Todo 已由用户取消。',
                    completed_at_unix_ms = ?1
                WHERE owner_user_id = ?2 AND trigger_kind = 'todo' AND status = 'pending'
                  AND (deduplication_key = ?3 OR deduplication_key LIKE ?4)
                "#,
                params![
                    updated_at_unix_ms,
                    owner_user_id,
                    format!("todo:{todo_id}"),
                    format!("todo:{todo_id}:attempt:%"),
                ],
            )
            .map_err(|_| team_todo_store_error())?;
        transaction.commit().map_err(|_| team_todo_store_error())?;
        self.load_project_team_todo(owner_user_id, team_id, todo_id)
    }

    fn load_project_team_todo(
        &self,
        owner_user_id: &str,
        team_id: &str,
        todo_id: &str,
    ) -> Result<AgentTeamTodo, String> {
        self.list_project_team_todos(owner_user_id, team_id, true)?
            .into_iter()
            .find(|todo| todo.id == todo_id)
            .ok_or_else(|| "团队任务不存在。".into())
    }

    pub(super) fn enqueue_ready_project_team_todos(
        &self,
        owner_user_id: &str,
        team_id: &str,
        now_unix_ms: i64,
    ) -> Result<usize, String> {
        validate_identifier(team_id).map_err(|_| invalid_team_todo_input())?;
        if now_unix_ms < 0 {
            return Err(invalid_team_todo_input());
        }
        reject_link_or_non_file(&self.database)?;
        let mut connection = open_database(&self.database, false)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| team_todo_store_error())?;
        require_active_project_team(&transaction, owner_user_id, team_id)?;
        let candidates = {
            let mut statement = transaction
                .prepare(
                    r#"
                    SELECT todo.id, todo.agent_id, todo.title, todo.detail,
                           todo.execution_contract_json
                    FROM local_agent_todos todo
                    JOIN project_agent_room_members member
                      ON member.owner_user_id = todo.owner_user_id
                     AND member.room_id = todo.team_room_id
                     AND member.agent_id = todo.agent_id
                    JOIN local_agent_profiles profile
                      ON profile.owner_user_id = member.owner_user_id
                     AND profile.id = member.agent_id
                    WHERE todo.owner_user_id = ?1 AND todo.team_room_id = ?2
                      AND todo.status = 'pending' AND member.status = 'active'
                      AND profile.status = 'active'
                      AND NOT EXISTS (
                        SELECT 1 FROM local_agent_todos running
                        WHERE running.owner_user_id = todo.owner_user_id
                          AND running.agent_id = todo.agent_id
                          AND running.status = 'in_progress'
                      )
                      AND NOT EXISTS (
                        SELECT 1
                        FROM local_agent_todo_dependencies dependency
                        JOIN local_agent_todos prerequisite
                          ON prerequisite.owner_user_id = dependency.owner_user_id
                         AND prerequisite.id = dependency.prerequisite_todo_id
                        WHERE dependency.owner_user_id = todo.owner_user_id
                          AND dependency.todo_id = todo.id
                          AND prerequisite.status != 'completed'
                      )
                      AND NOT EXISTS (
                        SELECT 1 FROM project_agent_deliveries delivery
                        WHERE delivery.owner_user_id = todo.owner_user_id
                          AND delivery.trigger_kind = 'todo'
                          AND (delivery.deduplication_key = 'todo:' || todo.id
                               OR delivery.deduplication_key LIKE
                                  'todo:' || todo.id || ':attempt:%')
                          AND delivery.status IN ('pending', 'running')
                      )
                    ORDER BY todo.priority DESC, todo.sort_order,
                             todo.created_at_unix_ms, todo.id
                    LIMIT 128
                    "#,
                )
                .map_err(|_| team_todo_store_error())?;
            let rows = statement
                .query_map(params![owner_user_id, team_id], |row| {
                    Ok(ReadyTeamTodo {
                        id: row.get(0)?,
                        agent_id: row.get(1)?,
                        title: row.get(2)?,
                        detail: row.get(3)?,
                        execution_contract_json: row.get(4)?,
                    })
                })
                .map_err(|_| team_todo_store_error())?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|_| team_todo_store_error())?
        };
        let mut scheduled_agents = HashSet::new();
        let mut scheduled = 0;
        for todo in candidates {
            if !scheduled_agents.insert(todo.agent_id.clone()) {
                continue;
            }
            let contract: AgentTodoExecutionContract =
                serde_json::from_str(&todo.execution_contract_json)
                    .map_err(|_| team_todo_store_error())?;
            validate_execution_contract(&contract)?;
            let content = executor_todo_message(&todo.title, &todo.detail, &contract);
            validate_text(&content, 1_000_000, false).map_err(|_| team_todo_store_error())?;
            let message_id = identifier("message");
            let delivery_id = identifier("delivery");
            transaction
                .execute(
                    r#"
                    INSERT INTO project_agent_messages (
                        owner_user_id, id, room_id, sender_kind, sender_id, content,
                        reply_to_message_id, source_run_id, causation_id, root_message_id,
                        hop_count, created_at_unix_ms
                    ) VALUES (?1, ?2, ?3, 'system', 'system', ?4, NULL, NULL,
                              'todo', ?2, 0, ?5)
                    "#,
                    params![owner_user_id, message_id, team_id, content, now_unix_ms],
                )
                .map_err(|_| team_todo_store_error())?;
            transaction
                .execute(
                    r#"
                    INSERT INTO project_agent_deliveries (
                        owner_user_id, id, room_id, message_id, root_message_id,
                        target_agent_id, trigger_kind, status, attempt, hop_count,
                        deduplication_key, response_message_id, last_error,
                        claimed_at_unix_ms, completed_at_unix_ms, created_at_unix_ms
                    ) VALUES (?1, ?2, ?3, ?4, ?4, ?5, 'todo', 'pending', 0, 0,
                              ?6, NULL, NULL, NULL, NULL, ?7)
                    "#,
                    params![
                        owner_user_id,
                        delivery_id,
                        team_id,
                        message_id,
                        todo.agent_id,
                        format!("todo:{}", todo.id),
                        now_unix_ms,
                    ],
                )
                .map_err(|_| team_todo_store_error())?;
            let changed = transaction
                .execute(
                    "UPDATE local_agent_todos SET status = 'in_progress', \
                     updated_at_unix_ms = CASE WHEN updated_at_unix_ms >= ?1 \
                       THEN updated_at_unix_ms + 1 ELSE ?1 END \
                     WHERE owner_user_id = ?2 AND team_room_id = ?3 AND id = ?4 \
                       AND agent_id = ?5 AND status = 'pending'",
                    params![now_unix_ms, owner_user_id, team_id, todo.id, todo.agent_id,],
                )
                .map_err(|_| team_todo_store_error())?;
            if changed != 1 {
                return Err(team_todo_conflict());
            }
            scheduled += 1;
        }
        transaction.commit().map_err(|_| team_todo_store_error())?;
        Ok(scheduled)
    }

    pub(super) fn active_project_team_run_bindings(
        &self,
        owner_user_id: &str,
        team_id: &str,
    ) -> Result<Vec<ActiveTeamRunBinding>, String> {
        validate_identifier(team_id).map_err(|_| invalid_team_todo_input())?;
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        require_active_project_team(&connection, owner_user_id, team_id)?;
        let mut statement = connection
            .prepare(
                r#"
                SELECT delivery.id, delivery.trigger_kind, delivery.target_agent_id,
                       run.turn_id, run.run_id
                FROM project_agent_deliveries delivery
                JOIN windows_agent_delivery_runs run
                  ON run.owner_user_id = delivery.owner_user_id
                 AND run.delivery_id = delivery.id
                WHERE delivery.owner_user_id = ?1 AND delivery.room_id = ?2
                  AND (delivery.status = 'running' OR run.status IN ('ready', 'running'))
                ORDER BY run.updated_at_unix_ms DESC, delivery.id
                LIMIT 500
                "#,
            )
            .map_err(|_| team_todo_store_error())?;
        let rows = statement
            .query_map(params![owner_user_id, team_id], |row| {
                let delivery_id: String = row.get(0)?;
                let trigger_kind: String = row.get(1)?;
                let agent_id: String = row.get(2)?;
                let conversation_id = if trigger_kind == "todo" {
                    format!("agent-team-executor-{delivery_id}")
                } else {
                    format!("agent-team-{team_id}-{agent_id}")
                };
                Ok(ActiveTeamRunBinding {
                    delivery_id,
                    conversation_id,
                    turn_id: row.get(3)?,
                    run_id: row.get(4)?,
                })
            })
            .map_err(|_| team_todo_store_error())?;
        let bindings = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| team_todo_store_error())?;
        for binding in &bindings {
            for value in [
                &binding.delivery_id,
                &binding.conversation_id,
                &binding.turn_id,
                &binding.run_id,
            ] {
                validate_identifier(value).map_err(|_| team_todo_store_error())?;
            }
        }
        Ok(bindings)
    }

    pub(super) fn reconcile_project_team_runs(
        &self,
        owner_user_id: &str,
        team_id: &str,
        updates: &[TeamRunReconciliation],
    ) -> Result<(), String> {
        validate_identifier(team_id).map_err(|_| invalid_team_todo_input())?;
        if updates.len() > 500 {
            return Err(team_todo_store_error());
        }
        reject_link_or_non_file(&self.database)?;
        let mut connection = open_database(&self.database, false)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| team_todo_store_error())?;
        require_active_project_team(&transaction, owner_user_id, team_id)?;
        for update in updates {
            for value in [&update.delivery_id, &update.turn_id, &update.run_id] {
                validate_identifier(value).map_err(|_| team_todo_store_error())?;
            }
            if update.created_at_unix_ms < 0
                || update.updated_at_unix_ms < update.created_at_unix_ms
            {
                return Err(team_todo_store_error());
            }
            let delivery_status = match update.status {
                LocalConversationTurnStatus::Running => "running",
                LocalConversationTurnStatus::Succeeded => "completed",
                LocalConversationTurnStatus::Failed => "failed",
                LocalConversationTurnStatus::Cancelled => "cancelled",
            };
            let completed_at = if update.status == LocalConversationTurnStatus::Running {
                None
            } else {
                Some(update.updated_at_unix_ms)
            };
            let changed = transaction
                .execute(
                    r#"
                    UPDATE project_agent_deliveries
                    SET status = ?1,
                        claimed_at_unix_ms = COALESCE(claimed_at_unix_ms, ?2),
                        completed_at_unix_ms = ?3
                    WHERE owner_user_id = ?4 AND room_id = ?5 AND id = ?6
                      AND status IN ('pending', 'running')
                      AND EXISTS (
                        SELECT 1 FROM windows_agent_delivery_runs run
                        WHERE run.owner_user_id = project_agent_deliveries.owner_user_id
                          AND run.delivery_id = project_agent_deliveries.id
                          AND run.turn_id = ?7 AND run.run_id = ?8
                      )
                    "#,
                    params![
                        delivery_status,
                        update.created_at_unix_ms,
                        completed_at,
                        owner_user_id,
                        team_id,
                        update.delivery_id,
                        update.turn_id,
                        update.run_id,
                    ],
                )
                .map_err(|_| team_todo_store_error())?;
            if changed > 1 {
                return Err(team_todo_store_error());
            }
            let run_changed = transaction
                .execute(
                    "UPDATE windows_agent_delivery_runs SET status = ?1, \
                     updated_at_unix_ms = ?2 WHERE owner_user_id = ?3 AND delivery_id = ?4 \
                     AND turn_id = ?5 AND run_id = ?6",
                    params![
                        update.status.as_str(),
                        update.updated_at_unix_ms,
                        owner_user_id,
                        update.delivery_id,
                        update.turn_id,
                        update.run_id,
                    ],
                )
                .map_err(|_| team_todo_store_error())?;
            if run_changed != 1 {
                return Err(team_todo_conflict());
            }
            reconcile_todo_from_delivery(&transaction, owner_user_id, update)?;
        }
        transaction.commit().map_err(|_| team_todo_store_error())
    }
}

fn reconcile_todo_from_delivery(
    connection: &Connection,
    owner_user_id: &str,
    update: &TeamRunReconciliation,
) -> Result<(), String> {
    let (status, blocked_reason, result) = match update.status {
        LocalConversationTurnStatus::Running => return Ok(()),
        LocalConversationTurnStatus::Succeeded => (
            "completed",
            "",
            "Todo 已由 Local Agent Host 完成；详细过程保留在运行详情中。",
        ),
        LocalConversationTurnStatus::Failed => (
            "blocked",
            "Local Agent Host 执行失败，请在运行详情中检查失败记录。",
            "",
        ),
        LocalConversationTurnStatus::Cancelled => (
            "blocked",
            "Local Agent Host 运行已取消；任务保留为阻塞，等待明确重试或取消。",
            "",
        ),
    };
    connection
        .execute(
            r#"
            UPDATE local_agent_todos
            SET status = ?1, blocked_reason = ?2, result = ?3,
                updated_at_unix_ms = CASE
                  WHEN updated_at_unix_ms >= ?4 THEN updated_at_unix_ms + 1
                  ELSE ?4
                END
            WHERE owner_user_id = ?5 AND status = 'in_progress'
              AND EXISTS (
                SELECT 1 FROM project_agent_deliveries delivery
                WHERE delivery.owner_user_id = local_agent_todos.owner_user_id
                  AND delivery.id = ?6 AND delivery.trigger_kind = 'todo'
                  AND delivery.target_agent_id = local_agent_todos.agent_id
                  AND (delivery.deduplication_key = 'todo:' || local_agent_todos.id
                       OR delivery.deduplication_key LIKE
                          'todo:' || local_agent_todos.id || ':attempt:%')
              )
            "#,
            params![
                status,
                blocked_reason,
                result,
                update.updated_at_unix_ms,
                owner_user_id,
                update.delivery_id,
            ],
        )
        .map_err(|_| team_todo_store_error())?;
    Ok(())
}

fn executor_todo_message(
    title: &str,
    detail: &str,
    contract: &AgentTodoExecutionContract,
) -> String {
    fn list(values: &[String]) -> String {
        values
            .iter()
            .map(|value| format!("- {value}"))
            .collect::<Vec<_>>()
            .join("\n")
    }
    format!(
        "你正在执行项目经理分配的独立 Todo。只处理以下工作契约，不要把团队聊天当作任务范围。\n\n\
         # {title}\n\n{detail}\n\n## 目标\n{}\n\n## 范围\n{}\n\n\
         ## 预期产出\n{}\n\n## 验收标准\n{}\n\n## 约束\n{}",
        contract.objective,
        contract.scope,
        list(&contract.expected_outputs),
        list(&contract.acceptance_criteria),
        if contract.constraints.is_empty() {
            "- 无额外约束".into()
        } else {
            list(&contract.constraints)
        },
    )
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

fn validate_todo_mutation(
    team_id: &str,
    agent_id: Option<&str>,
    title: &str,
    detail: &str,
    priority: i64,
    dependency_ids: &[String],
    now_unix_ms: i64,
) -> Result<(), String> {
    validate_identifier(team_id).map_err(|_| invalid_team_todo_input())?;
    if let Some(agent_id) = agent_id {
        validate_agent_identifier(agent_id).map_err(|_| invalid_team_todo_input())?;
    }
    validate_text(title, 500, false).map_err(|_| invalid_team_todo_input())?;
    validate_text(detail, 16_000, true).map_err(|_| invalid_team_todo_input())?;
    if !(0..=100).contains(&priority) || now_unix_ms < 0 {
        return Err(invalid_team_todo_input());
    }
    validate_identifier_list(dependency_ids, 64)
}

fn validate_identifier_list(values: &[String], maximum: usize) -> Result<(), String> {
    if values.len() > maximum {
        return Err(invalid_team_todo_input());
    }
    let mut unique = HashSet::new();
    for value in values {
        validate_identifier(value).map_err(|_| invalid_team_todo_input())?;
        if !unique.insert(value) {
            return Err(invalid_team_todo_input());
        }
    }
    Ok(())
}

fn normalized_contract(
    contract: Option<AgentTodoExecutionContract>,
    title: &str,
    detail: &str,
) -> AgentTodoExecutionContract {
    let mut contract = contract.unwrap_or(AgentTodoExecutionContract {
        objective: String::new(),
        scope: String::new(),
        expected_outputs: Vec::new(),
        acceptance_criteria: Vec::new(),
        constraints: Vec::new(),
    });
    if contract.objective.is_empty() {
        contract.objective = title.to_owned();
    }
    if contract.scope.is_empty() {
        contract.scope = detail.to_owned();
    }
    if contract.expected_outputs.is_empty() {
        contract.expected_outputs = vec![title.to_owned()];
    }
    if contract.acceptance_criteria.is_empty() {
        contract.acceptance_criteria = vec!["完成任务目标并输出可核验的结果总结。".into()];
    }
    contract
}

fn default_execution_plan(now_unix_ms: i64) -> AgentTodoExecutionPlan {
    AgentTodoExecutionPlan {
        requires_execution: true,
        builtin_capabilities: vec!["project_read".into()],
        plugins: Vec::new(),
        selection_revision: "local-v1".into(),
        selected_at_unix_ms: now_unix_ms,
    }
}

fn require_active_team_member(
    connection: &Connection,
    owner_user_id: &str,
    team_id: &str,
    agent_id: &str,
) -> Result<(), String> {
    let exists = connection
        .query_row(
            r#"
            SELECT EXISTS(
                SELECT 1
                FROM project_agent_room_members member
                JOIN local_agent_profiles profile
                  ON profile.owner_user_id = member.owner_user_id
                 AND profile.id = member.agent_id
                WHERE member.owner_user_id = ?1 AND member.room_id = ?2
                  AND member.agent_id = ?3 AND member.status = 'active'
                  AND profile.status = 'active'
            )
            "#,
            params![owner_user_id, team_id, agent_id],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|_| team_todo_store_error())?;
    if !exists {
        return Err("任务执行 Agent 不是当前团队的活跃成员。".into());
    }
    Ok(())
}

fn validate_dependencies(
    connection: &Connection,
    owner_user_id: &str,
    team_id: &str,
    todo_id: Option<&str>,
    dependency_ids: &[String],
) -> Result<(), String> {
    for dependency_id in dependency_ids {
        if todo_id == Some(dependency_id.as_str()) {
            return Err("团队任务不能依赖自身。".into());
        }
        let exists = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM local_agent_todos WHERE owner_user_id = ?1 \
                 AND team_room_id = ?2 AND id = ?3)",
                params![owner_user_id, team_id, dependency_id],
                |row| row.get::<_, bool>(0),
            )
            .map_err(|_| team_todo_store_error())?;
        if !exists {
            return Err("任务依赖项不属于当前团队。".into());
        }
    }
    if let Some(todo_id) = todo_id {
        for dependency_id in dependency_ids {
            if dependency_reaches(connection, owner_user_id, dependency_id, todo_id)? {
                return Err("任务依赖关系会形成循环。".into());
            }
        }
    }
    Ok(())
}

fn dependency_reaches(
    connection: &Connection,
    owner_user_id: &str,
    start_id: &str,
    target_id: &str,
) -> Result<bool, String> {
    connection
        .query_row(
            r#"
            WITH RECURSIVE dependency_path(todo_id) AS (
                SELECT prerequisite_todo_id
                FROM local_agent_todo_dependencies
                WHERE owner_user_id = ?1 AND todo_id = ?2
                UNION
                SELECT dependency.prerequisite_todo_id
                FROM local_agent_todo_dependencies dependency
                JOIN dependency_path path ON path.todo_id = dependency.todo_id
                WHERE dependency.owner_user_id = ?1
            )
            SELECT EXISTS(SELECT 1 FROM dependency_path WHERE todo_id = ?3)
            "#,
            params![owner_user_id, start_id, target_id],
            |row| row.get(0),
        )
        .map_err(|_| team_todo_store_error())
}

fn replace_dependencies(
    connection: &Connection,
    owner_user_id: &str,
    todo_id: &str,
    dependency_ids: &[String],
    now_unix_ms: i64,
) -> Result<(), String> {
    for dependency_id in dependency_ids {
        connection
            .execute(
                "INSERT INTO local_agent_todo_dependencies (owner_user_id, todo_id, \
                 prerequisite_todo_id, created_at_unix_ms) VALUES (?1, ?2, ?3, ?4)",
                params![owner_user_id, todo_id, dependency_id, now_unix_ms],
            )
            .map_err(|_| team_todo_store_error())?;
    }
    Ok(())
}

fn todo_has_active_host_run(
    connection: &Connection,
    owner_user_id: &str,
    todo_id: &str,
) -> Result<bool, String> {
    connection
        .query_row(
            r#"
            SELECT EXISTS(
                SELECT 1
                FROM project_agent_deliveries delivery
                LEFT JOIN windows_agent_delivery_runs run
                  ON run.owner_user_id = delivery.owner_user_id
                 AND run.delivery_id = delivery.id
                WHERE delivery.owner_user_id = ?1 AND delivery.trigger_kind = 'todo'
                  AND (delivery.deduplication_key = ?2
                       OR delivery.deduplication_key LIKE ?3)
                  AND (delivery.status = 'running' OR run.status IN ('ready', 'running'))
            )
            "#,
            params![
                owner_user_id,
                format!("todo:{todo_id}"),
                format!("todo:{todo_id}:attempt:%"),
            ],
            |row| row.get(0),
        )
        .map_err(|_| team_todo_store_error())
}

fn team_todo_store_error() -> String {
    "团队任务数据不可用，请检查本机存储后重试。".into()
}

fn invalid_team_todo_input() -> String {
    "团队任务内容无效，请检查后重试。".into()
}

fn team_todo_conflict() -> String {
    "团队任务已被更新，请刷新后重试。".into()
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
pub(crate) async fn create_agent_team_todo(
    input: CreateAgentTeamTodoInput,
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
    Ok(todo)
}

#[tauri::command]
pub(crate) async fn update_agent_team_todo(
    input: UpdateAgentTeamTodoInput,
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
    Ok(todo)
}

#[tauri::command]
pub(crate) async fn reorder_agent_team_todos(
    input: ReorderAgentTeamTodosInput,
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
    let now = chrono::Utc::now().timestamp_millis().max(0);
    let todos = tauri::async_runtime::spawn_blocking(move || {
        store.reorder_project_team_todos(&owner, &input.team_id, &input.todo_ids, now)
    })
    .await
    .map_err(|_| "团队任务排序任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&session_owner, generation)
        .await?;
    Ok(todos)
}

#[tauri::command]
pub(crate) async fn cancel_agent_team_todo(
    input: CancelAgentTeamTodoInput,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
) -> Result<AgentTeamTodo, String> {
    validate_owner(&input.owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    let store = conversations.inner().clone();
    let session_owner = input.owner_user_id.clone();
    let owner = session_owner.clone();
    let now = chrono::Utc::now().timestamp_millis().max(0);
    let todo = tauri::async_runtime::spawn_blocking(move || {
        store.cancel_project_team_todo(
            &owner,
            &input.team_id,
            &input.todo_id,
            input.expected_updated_at_unix_ms,
            now,
        )
    })
    .await
    .map_err(|_| "团队任务取消任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&session_owner, generation)
        .await?;
    Ok(todo)
}
