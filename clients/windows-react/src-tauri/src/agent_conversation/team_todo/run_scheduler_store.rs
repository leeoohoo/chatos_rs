use super::*;

impl AgentConversationStore {
    pub(super) fn load_project_team_todo(
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

    pub(in crate::agent_conversation) fn enqueue_ready_project_team_todos(
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
            transaction
                .execute(
                    r#"
                    INSERT OR IGNORE INTO local_agent_todo_asset_snapshots (
                        owner_user_id, todo_id, asset_id, team_room_id, category,
                        title, markdown, revision, captured_at_unix_ms
                    )
                    SELECT owner_user_id, ?1, id, team_room_id, category,
                           title, markdown, revision, ?2
                    FROM local_agent_team_assets
                    WHERE owner_user_id = ?3 AND team_room_id = ?4 AND status = 'active'
                    "#,
                    params![todo.id, now_unix_ms, owner_user_id, team_id],
                )
                .map_err(|_| team_todo_store_error())?;
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

    pub(in crate::agent_conversation) fn active_project_team_run_bindings(
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
                    super::agent_team_conversation_id(team_id, &agent_id)
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

    pub(in crate::agent_conversation) fn reconcile_project_team_runs(
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
            let changed = update_delivery_from_run(
                &transaction,
                owner_user_id,
                team_id,
                &update.delivery_id,
                &update.turn_id,
                &update.run_id,
                &update.status,
                update.created_at_unix_ms,
                update.updated_at_unix_ms,
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
