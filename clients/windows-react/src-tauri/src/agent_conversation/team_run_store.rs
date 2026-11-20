use super::*;

impl AgentConversationStore {
    pub(super) fn load_message_attachment_payloads(
        &self,
        owner_user_id: &str,
        message_id: &str,
    ) -> Result<Vec<crate::conversation_attachments::VerifiedAttachmentPayload>, String> {
        validate_identifier(message_id).map_err(|_| team_store_error())?;
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        documents::load_message_attachment_payloads(
            &connection,
            &self.database,
            owner_user_id,
            message_id,
        )
    }

    pub(super) fn load_project_team_runs(
        &self,
        owner_user_id: &str,
        team_id: &str,
    ) -> Result<AgentTeamRunHistorySnapshot, String> {
        validate_identifier(team_id).map_err(|_| invalid_team_input())?;
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        let exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM project_agent_rooms WHERE owner_user_id = ?1 \
                 AND id = ?2 AND status = 'active' AND conversation_kind = 'project_team')",
                params![owner_user_id, team_id],
                |row| row.get(0),
            )
            .map_err(|_| team_store_error())?;
        if !exists {
            return Err("Agent 团队不存在或已停用。".into());
        }
        let message_count = connection
            .query_row(
                "SELECT count(*) FROM project_agent_messages \
                 WHERE owner_user_id = ?1 AND room_id = ?2",
                params![owner_user_id, team_id],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|_| team_store_error())?;
        let delivery_counts = connection
            .query_row(
                r#"
                SELECT
                    COALESCE(sum(CASE status WHEN 'pending' THEN 1 ELSE 0 END), 0),
                    COALESCE(sum(CASE status WHEN 'running' THEN 1 ELSE 0 END), 0),
                    COALESCE(sum(CASE status WHEN 'completed' THEN 1 ELSE 0 END), 0),
                    COALESCE(sum(CASE status WHEN 'failed' THEN 1 ELSE 0 END), 0),
                    COALESCE(sum(CASE status WHEN 'cancelled' THEN 1 ELSE 0 END), 0)
                FROM project_agent_deliveries
                WHERE owner_user_id = ?1 AND room_id = ?2
                "#,
                params![owner_user_id, team_id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                    ))
                },
            )
            .map_err(|_| team_store_error())?;
        let mut statement = connection
            .prepare(
                r#"
                SELECT delivery.id, delivery.message_id, delivery.target_agent_id,
                       CASE WHEN delivery.trigger_kind = 'todo' THEN 'executor' ELSE 'manager' END,
                       delivery.trigger_kind, delivery.status, run.status, run.turn_id,
                       run.run_id, delivery.response_message_id, delivery.last_error,
                       delivery.created_at_unix_ms, run.updated_at_unix_ms
                FROM project_agent_deliveries delivery
                JOIN windows_agent_delivery_runs run
                  ON run.owner_user_id = delivery.owner_user_id
                 AND run.delivery_id = delivery.id
                WHERE delivery.owner_user_id = ?1 AND delivery.room_id = ?2
                ORDER BY run.updated_at_unix_ms DESC, run.run_id DESC
                LIMIT 500
                "#,
            )
            .map_err(|_| team_store_error())?;
        let rows = statement
            .query_map(params![owner_user_id, team_id], |row| {
                Ok(AgentTeamRunSummary {
                    delivery_id: row.get(0)?,
                    message_id: row.get(1)?,
                    agent_id: row.get(2)?,
                    lane: row.get(3)?,
                    trigger_kind: row.get(4)?,
                    delivery_status: row.get(5)?,
                    run_status: row.get(6)?,
                    turn_id: row.get(7)?,
                    run_id: row.get(8)?,
                    response_message_id: row.get(9)?,
                    last_error: row.get(10)?,
                    created_at_unix_ms: row.get(11)?,
                    updated_at_unix_ms: row.get(12)?,
                })
            })
            .map_err(|_| team_store_error())?;
        let runs = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| team_store_error())?;
        Ok(AgentTeamRunHistorySnapshot {
            team_id: team_id.to_owned(),
            message_count,
            pending_delivery_count: delivery_counts.0,
            running_delivery_count: delivery_counts.1,
            completed_delivery_count: delivery_counts.2,
            failed_delivery_count: delivery_counts.3,
            cancelled_delivery_count: delivery_counts.4,
            runs,
        })
    }
}
