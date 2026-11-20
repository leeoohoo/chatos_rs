use super::*;

impl AgentConversationStore {
    pub(super) fn record_project_team_todo_cancel_intent(
        &self,
        owner_user_id: &str,
        team_id: &str,
        todo_id: &str,
        expected_updated_at_unix_ms: i64,
        now_unix_ms: i64,
    ) -> Result<Option<TeamRunControlIntent>, String> {
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
        let todo_status: Option<String> = transaction
            .query_row(
                "SELECT status FROM local_agent_todos WHERE owner_user_id = ?1 \
                 AND team_room_id = ?2 AND id = ?3 AND updated_at_unix_ms = ?4",
                params![owner_user_id, team_id, todo_id, expected_updated_at_unix_ms],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| team_todo_store_error())?;
        let Some(todo_status) = todo_status else {
            return Err(team_todo_conflict());
        };
        let binding = transaction
            .query_row(
                r#"
                SELECT delivery.id, run.turn_id, run.run_id
                FROM project_agent_deliveries delivery
                JOIN windows_agent_delivery_runs run
                  ON run.owner_user_id = delivery.owner_user_id
                 AND run.delivery_id = delivery.id
                WHERE delivery.owner_user_id = ?1 AND delivery.room_id = ?2
                  AND delivery.trigger_kind = 'todo'
                  AND (delivery.deduplication_key = ?3
                       OR delivery.deduplication_key LIKE ?4)
                  AND (delivery.status = 'running' OR run.status IN ('ready', 'running'))
                ORDER BY run.updated_at_unix_ms DESC, delivery.id
                LIMIT 1
                "#,
                params![
                    owner_user_id,
                    team_id,
                    format!("todo:{todo_id}"),
                    format!("todo:{todo_id}:attempt:%"),
                ],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(|_| team_todo_store_error())?;
        let Some((delivery_id, turn_id, run_id)) = binding else {
            if todo_status == "in_progress" {
                return Err(
                    "任务仍标记为执行中，但没有可核验的 Host Run；请刷新运行详情后重试。".into(),
                );
            }
            return Ok(None);
        };
        if todo_status != "in_progress" {
            return Err("Host Run 仍在执行，请刷新任务状态后再停止。".into());
        }
        let conversation_id = format!("agent-team-executor-{delivery_id}");
        let intent_id = identifier("run_control");
        transaction
            .execute(
                r#"
                INSERT OR IGNORE INTO windows_agent_run_control_journal (
                    owner_user_id, id, action, team_room_id, todo_id, delivery_id,
                    conversation_id, turn_id, run_id, status, host_status,
                    created_at_unix_ms, completed_at_unix_ms
                ) VALUES (?1, ?2, 'cancel', ?3, ?4, ?5, ?6, ?7, ?8,
                          'pending', NULL, ?9, NULL)
                "#,
                params![
                    owner_user_id,
                    intent_id,
                    team_id,
                    todo_id,
                    delivery_id,
                    conversation_id,
                    turn_id,
                    run_id,
                    now_unix_ms,
                ],
            )
            .map_err(|_| team_todo_store_error())?;
        let intent =
            load_pending_run_control_intent(&transaction, owner_user_id, todo_id, "cancel")?;
        transaction.commit().map_err(|_| team_todo_store_error())?;
        Ok(Some(intent))
    }

    pub(crate) fn pending_team_run_control_intents(
        &self,
        owner_user_id: &str,
    ) -> Result<Vec<TeamRunControlIntent>, String> {
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        let mut statement = connection
            .prepare(
                r#"
                SELECT id, action, team_room_id, todo_id, delivery_id,
                       conversation_id, turn_id, run_id
                FROM windows_agent_run_control_journal
                WHERE owner_user_id = ?1 AND status = 'pending'
                ORDER BY created_at_unix_ms, id
                LIMIT 500
                "#,
            )
            .map_err(|_| team_todo_store_error())?;
        let rows = statement
            .query_map(params![owner_user_id], |row| {
                Ok(TeamRunControlIntent {
                    id: row.get(0)?,
                    owner_user_id: owner_user_id.to_owned(),
                    action: row.get(1)?,
                    team_id: row.get(2)?,
                    todo_id: row.get(3)?,
                    delivery_id: row.get(4)?,
                    conversation_id: row.get(5)?,
                    turn_id: row.get(6)?,
                    run_id: row.get(7)?,
                })
            })
            .map_err(|_| team_todo_store_error())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| team_todo_store_error())
    }

    pub(super) fn record_project_team_run_retry_intent(
        &self,
        owner_user_id: &str,
        team_id: &str,
        run_id: &str,
        now_unix_ms: i64,
    ) -> Result<TeamRunControlIntent, String> {
        validate_identifier(team_id).map_err(|_| invalid_team_todo_input())?;
        validate_identifier(run_id).map_err(|_| invalid_team_todo_input())?;
        if now_unix_ms < 0 {
            return Err(invalid_team_todo_input());
        }
        reject_link_or_non_file(&self.database)?;
        let mut connection = open_database(&self.database, false)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| team_todo_store_error())?;
        require_active_project_team(&transaction, owner_user_id, team_id)?;
        let binding = transaction
            .query_row(
                r#"
                SELECT todo.id, delivery.id, run.turn_id
                FROM windows_agent_delivery_runs run
                JOIN project_agent_deliveries delivery
                  ON delivery.owner_user_id = run.owner_user_id
                 AND delivery.id = run.delivery_id
                JOIN local_agent_todos todo
                  ON todo.owner_user_id = delivery.owner_user_id
                 AND todo.team_room_id = delivery.room_id
                 AND todo.agent_id = delivery.target_agent_id
                 AND (delivery.deduplication_key = 'todo:' || todo.id
                      OR delivery.deduplication_key LIKE
                         'todo:' || todo.id || ':attempt:%')
                WHERE run.owner_user_id = ?1 AND delivery.room_id = ?2 AND run.run_id = ?3
                  AND delivery.trigger_kind = 'todo' AND delivery.status = 'running'
                  AND run.status IN ('ready', 'running')
                  AND todo.status IN ('in_progress', 'blocked')
                "#,
                params![owner_user_id, team_id, run_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(|_| team_todo_store_error())?
            .ok_or_else(|| "只有等待 Human 处理的团队 Todo Run 可以重试。".to_owned())?;
        let (todo_id, delivery_id, turn_id) = binding;
        let has_cancel_intent: bool = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM windows_agent_run_control_journal \
                 WHERE owner_user_id = ?1 AND todo_id = ?2 AND action = 'cancel' \
                   AND status = 'pending')",
                params![owner_user_id, todo_id],
                |row| row.get(0),
            )
            .map_err(|_| team_todo_store_error())?;
        if has_cancel_intent {
            return Err("这个团队 Todo 正在停止，不能同时重试。".into());
        }
        let conversation_id = format!("agent-team-executor-{delivery_id}");
        transaction
            .execute(
                r#"
                INSERT OR IGNORE INTO windows_agent_run_control_journal (
                    owner_user_id, id, action, team_room_id, todo_id, delivery_id,
                    conversation_id, turn_id, run_id, status, host_status,
                    created_at_unix_ms, completed_at_unix_ms
                ) VALUES (?1, ?2, 'retry', ?3, ?4, ?5, ?6, ?7, ?8,
                          'pending', NULL, ?9, NULL)
                "#,
                params![
                    owner_user_id,
                    identifier("run_control"),
                    team_id,
                    todo_id,
                    delivery_id,
                    conversation_id,
                    turn_id,
                    run_id,
                    now_unix_ms,
                ],
            )
            .map_err(|_| team_todo_store_error())?;
        let intent =
            load_pending_run_control_intent(&transaction, owner_user_id, &todo_id, "retry")?;
        transaction.commit().map_err(|_| team_todo_store_error())?;
        Ok(intent)
    }
}
