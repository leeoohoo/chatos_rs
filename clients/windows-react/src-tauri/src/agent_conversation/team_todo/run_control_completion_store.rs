use super::*;

impl AgentConversationStore {
    pub(crate) fn complete_team_run_cancel_intent(
        &self,
        intent: &TeamRunControlIntent,
        turn: &LocalConversationTurnRecord,
    ) -> Result<AgentTeamTodo, String> {
        if intent.action != "cancel"
            || turn.turn_id != intent.turn_id
            || turn.run_id != intent.run_id
            || turn.status == LocalConversationTurnStatus::Running
        {
            return Err(team_todo_conflict());
        }
        reject_link_or_non_file(&self.database)?;
        let mut connection = open_database(&self.database, false)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| team_todo_store_error())?;
        let journal_matches: bool = transaction
            .query_row(
                r#"
                SELECT EXISTS(
                    SELECT 1 FROM windows_agent_run_control_journal
                    WHERE owner_user_id = ?1 AND id = ?2 AND action = 'cancel'
                      AND team_room_id = ?3 AND todo_id = ?4 AND delivery_id = ?5
                      AND conversation_id = ?6 AND turn_id = ?7 AND run_id = ?8
                      AND status = 'pending'
                )
                "#,
                params![
                    intent.owner_user_id,
                    intent.id,
                    intent.team_id,
                    intent.todo_id,
                    intent.delivery_id,
                    intent.conversation_id,
                    intent.turn_id,
                    intent.run_id,
                ],
                |row| row.get(0),
            )
            .map_err(|_| team_todo_store_error())?;
        if !journal_matches {
            return Err(team_todo_conflict());
        }
        let (delivery_status, todo_status, blocked_reason, result) = match turn.status {
            LocalConversationTurnStatus::Succeeded => (
                "completed",
                "completed",
                "",
                "Todo 已由 Local Agent Host 完成；详细过程保留在运行详情中。",
            ),
            LocalConversationTurnStatus::Failed => (
                "failed",
                "blocked",
                "Local Agent Host 在停止请求生效前执行失败，请检查运行详情。",
                "",
            ),
            LocalConversationTurnStatus::Cancelled => {
                ("cancelled", "cancelled", "", "Todo 已由用户停止。")
            }
            LocalConversationTurnStatus::Running => unreachable!(),
        };
        let current_todo_status: String = transaction
            .query_row(
                "SELECT status FROM local_agent_todos WHERE owner_user_id = ?1 \
                 AND team_room_id = ?2 AND id = ?3",
                params![intent.owner_user_id, intent.team_id, intent.todo_id],
                |row| row.get(0),
            )
            .map_err(|_| team_todo_store_error())?;
        let reconcilable = match turn.status {
            LocalConversationTurnStatus::Succeeded => {
                matches!(current_todo_status.as_str(), "in_progress" | "completed")
            }
            LocalConversationTurnStatus::Failed => {
                matches!(current_todo_status.as_str(), "in_progress" | "blocked")
            }
            LocalConversationTurnStatus::Cancelled => matches!(
                current_todo_status.as_str(),
                "in_progress" | "blocked" | "cancelled"
            ),
            LocalConversationTurnStatus::Running => false,
        };
        if !reconcilable {
            return Err(team_todo_conflict());
        }
        transaction
            .execute(
                r#"
                UPDATE project_agent_deliveries
                SET status = ?1, last_error = CASE WHEN ?1 = 'failed' THEN ?2 ELSE NULL END,
                    completed_at_unix_ms = ?3
                WHERE owner_user_id = ?4 AND id = ?5 AND room_id = ?6
                  AND status IN ('pending', 'running')
                "#,
                params![
                    delivery_status,
                    blocked_reason,
                    turn.updated_at_unix_ms,
                    intent.owner_user_id,
                    intent.delivery_id,
                    intent.team_id,
                ],
            )
            .map_err(|_| team_todo_store_error())?;
        transaction
            .execute(
                "UPDATE windows_agent_delivery_runs SET status = ?1, updated_at_unix_ms = ?2 \
                 WHERE owner_user_id = ?3 AND delivery_id = ?4 AND turn_id = ?5 AND run_id = ?6",
                params![
                    turn.status.as_str(),
                    turn.updated_at_unix_ms,
                    intent.owner_user_id,
                    intent.delivery_id,
                    intent.turn_id,
                    intent.run_id,
                ],
            )
            .map_err(|_| team_todo_store_error())?;
        let todo_changed = transaction
            .execute(
                r#"
                UPDATE local_agent_todos
                SET status = ?1, blocked_reason = ?2, result = ?3,
                    updated_at_unix_ms = CASE
                      WHEN updated_at_unix_ms >= ?4 THEN updated_at_unix_ms + 1
                      ELSE ?4
                    END
                WHERE owner_user_id = ?5 AND team_room_id = ?6 AND id = ?7
                "#,
                params![
                    todo_status,
                    blocked_reason,
                    result,
                    turn.updated_at_unix_ms,
                    intent.owner_user_id,
                    intent.team_id,
                    intent.todo_id,
                ],
            )
            .map_err(|_| team_todo_store_error())?;
        if todo_changed != 1 {
            return Err(team_todo_conflict());
        }
        let journal_changed = transaction
            .execute(
                r#"
                UPDATE windows_agent_run_control_journal
                SET status = 'completed', host_status = ?1, completed_at_unix_ms = ?2
                WHERE owner_user_id = ?3 AND id = ?4 AND status = 'pending'
                "#,
                params![
                    turn.status.as_str(),
                    turn.updated_at_unix_ms,
                    intent.owner_user_id,
                    intent.id,
                ],
            )
            .map_err(|_| team_todo_store_error())?;
        if journal_changed != 1 {
            return Err(team_todo_conflict());
        }
        transaction.commit().map_err(|_| team_todo_store_error())?;
        self.load_project_team_todo(&intent.owner_user_id, &intent.team_id, &intent.todo_id)
    }

    pub(crate) fn complete_team_run_retry_intent(
        &self,
        intent: &TeamRunControlIntent,
        turn: &LocalConversationTurnRecord,
        run_status: LocalAgentRunStatus,
        now_unix_ms: i64,
    ) -> Result<AgentTeamTodo, String> {
        if intent.action != "retry"
            || turn.turn_id != intent.turn_id
            || turn.run_id != intent.run_id
            || now_unix_ms < 0
            || (turn.status == LocalConversationTurnStatus::Running
                && matches!(
                    run_status,
                    LocalAgentRunStatus::WaitingUser
                        | LocalAgentRunStatus::Paused
                        | LocalAgentRunStatus::NeedsReview
                ))
        {
            return Err(team_todo_conflict());
        }
        reject_link_or_non_file(&self.database)?;
        let mut connection = open_database(&self.database, false)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| team_todo_store_error())?;
        let journal_matches: bool = transaction
            .query_row(
                r#"
                SELECT EXISTS(
                    SELECT 1 FROM windows_agent_run_control_journal
                    WHERE owner_user_id = ?1 AND id = ?2 AND action = 'retry'
                      AND team_room_id = ?3 AND todo_id = ?4 AND delivery_id = ?5
                      AND conversation_id = ?6 AND turn_id = ?7 AND run_id = ?8
                      AND status = 'pending'
                )
                "#,
                params![
                    intent.owner_user_id,
                    intent.id,
                    intent.team_id,
                    intent.todo_id,
                    intent.delivery_id,
                    intent.conversation_id,
                    intent.turn_id,
                    intent.run_id,
                ],
                |row| row.get(0),
            )
            .map_err(|_| team_todo_store_error())?;
        if !journal_matches {
            return Err(team_todo_conflict());
        }
        let (delivery_status, local_run_status, todo_status, blocked_reason, result) = match turn
            .status
        {
            LocalConversationTurnStatus::Running => ("running", "running", "in_progress", "", ""),
            LocalConversationTurnStatus::Succeeded => (
                "completed",
                "succeeded",
                "completed",
                "",
                "Todo 已由 Local Agent Host 完成；详细过程保留在运行详情中。",
            ),
            LocalConversationTurnStatus::Failed => (
                "failed",
                "failed",
                "blocked",
                "Local Agent Host 在重试前已执行失败，请检查运行详情。",
                "",
            ),
            LocalConversationTurnStatus::Cancelled => (
                "cancelled",
                "cancelled",
                "blocked",
                "Local Agent Host Run 已取消，不能继续重试。",
                "",
            ),
        };
        let current_todo_status: String = transaction
            .query_row(
                "SELECT status FROM local_agent_todos WHERE owner_user_id = ?1 \
                 AND team_room_id = ?2 AND id = ?3",
                params![intent.owner_user_id, intent.team_id, intent.todo_id],
                |row| row.get(0),
            )
            .map_err(|_| team_todo_store_error())?;
        if !matches!(current_todo_status.as_str(), "in_progress" | "blocked")
            && current_todo_status != todo_status
        {
            return Err(team_todo_conflict());
        }
        transaction
            .execute(
                r#"
                UPDATE project_agent_deliveries
                SET status = ?1,
                    last_error = CASE WHEN ?1 IN ('failed', 'cancelled') THEN ?2 ELSE NULL END,
                    completed_at_unix_ms = CASE WHEN ?1 = 'running' THEN NULL ELSE ?3 END
                WHERE owner_user_id = ?4 AND id = ?5 AND room_id = ?6
                "#,
                params![
                    delivery_status,
                    blocked_reason,
                    now_unix_ms,
                    intent.owner_user_id,
                    intent.delivery_id,
                    intent.team_id,
                ],
            )
            .map_err(|_| team_todo_store_error())?;
        transaction
            .execute(
                "UPDATE windows_agent_delivery_runs SET status = ?1, updated_at_unix_ms = ?2 \
                 WHERE owner_user_id = ?3 AND delivery_id = ?4 AND turn_id = ?5 AND run_id = ?6",
                params![
                    local_run_status,
                    now_unix_ms,
                    intent.owner_user_id,
                    intent.delivery_id,
                    intent.turn_id,
                    intent.run_id,
                ],
            )
            .map_err(|_| team_todo_store_error())?;
        let todo_changed = transaction
            .execute(
                r#"
                UPDATE local_agent_todos
                SET status = ?1, blocked_reason = ?2, result = ?3,
                    updated_at_unix_ms = CASE
                      WHEN updated_at_unix_ms >= ?4 THEN updated_at_unix_ms + 1
                      ELSE ?4
                    END
                WHERE owner_user_id = ?5 AND team_room_id = ?6 AND id = ?7
                "#,
                params![
                    todo_status,
                    blocked_reason,
                    result,
                    now_unix_ms,
                    intent.owner_user_id,
                    intent.team_id,
                    intent.todo_id,
                ],
            )
            .map_err(|_| team_todo_store_error())?;
        if todo_changed != 1 {
            return Err(team_todo_conflict());
        }
        let journal_changed = transaction
            .execute(
                r#"
                UPDATE windows_agent_run_control_journal
                SET status = 'completed', host_status = ?1, completed_at_unix_ms = ?2
                WHERE owner_user_id = ?3 AND id = ?4 AND status = 'pending'
                "#,
                params![
                    run_status.as_str(),
                    now_unix_ms,
                    intent.owner_user_id,
                    intent.id,
                ],
            )
            .map_err(|_| team_todo_store_error())?;
        if journal_changed != 1 {
            return Err(team_todo_conflict());
        }
        transaction.commit().map_err(|_| team_todo_store_error())?;
        self.load_project_team_todo(&intent.owner_user_id, &intent.team_id, &intent.todo_id)
    }
}
