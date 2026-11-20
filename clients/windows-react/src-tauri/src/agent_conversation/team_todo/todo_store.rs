use super::*;

impl AgentConversationStore {
    pub(in crate::agent_conversation) fn list_project_team_todos(
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
    pub(in crate::agent_conversation) fn create_project_team_todo(
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
    pub(in crate::agent_conversation) fn update_project_team_todo(
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

    pub(in crate::agent_conversation) fn reorder_project_team_todos(
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

    pub(in crate::agent_conversation) fn cancel_project_team_todo(
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
}
