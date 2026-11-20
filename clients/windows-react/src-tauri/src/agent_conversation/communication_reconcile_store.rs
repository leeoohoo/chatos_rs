use super::*;

impl AgentConversationStore {
    pub(super) fn reconcile_agent_communication_runs(
        &self,
        owner_user_id: &str,
        room_id: &str,
        updates: &[AgentCommunicationRunReconciliation],
    ) -> Result<(), String> {
        validate_identifier(room_id)?;
        if updates.len() > 500 {
            return Err(team_store_error());
        }
        reject_link_or_non_file(&self.database)?;
        let mut connection = open_database(&self.database, false)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| team_store_error())?;
        for update in updates {
            for value in [&update.delivery_id, &update.turn_id, &update.run_id] {
                validate_identifier(value).map_err(|_| team_store_error())?;
            }
            if update.created_at_unix_ms < 0
                || update.updated_at_unix_ms < update.created_at_unix_ms
            {
                return Err(team_store_error());
            }
            let changed = update_delivery_from_run(
                &transaction,
                owner_user_id,
                room_id,
                &update.delivery_id,
                &update.turn_id,
                &update.run_id,
                &update.status,
                update.created_at_unix_ms,
                update.updated_at_unix_ms,
            )
            .map_err(|_| team_store_error())?;
            if changed > 1 {
                return Err(team_store_error());
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
                .map_err(|_| team_store_error())?;
            if run_changed != 1 {
                return Err(team_store_error());
            }
        }
        transaction.commit().map_err(|_| team_store_error())
    }

    pub(super) fn activity_room_ids(
        &self,
        owner_user_id: &str,
        agent_id: &str,
    ) -> Result<Vec<String>, String> {
        validate_agent_identifier(agent_id)?;
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        let active: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM local_agent_profiles \
                 WHERE owner_user_id = ?1 AND id = ?2 AND status = 'active')",
                params![owner_user_id, agent_id],
                |row| row.get(0),
            )
            .map_err(|_| store_error())?;
        if !active {
            return Err("Agent 不存在或已停用。".into());
        }
        let mut statement = connection
            .prepare(
                r#"
                SELECT DISTINCT room.id
                FROM project_agent_rooms room
                JOIN project_agent_room_members member
                  ON member.owner_user_id = room.owner_user_id AND member.room_id = room.id
                WHERE room.owner_user_id = ?1 AND member.agent_id = ?2
                  AND room.status = 'active' AND member.status = 'active'
                  AND room.conversation_kind = 'human_agent_direct'
                ORDER BY room.updated_at_unix_ms DESC, room.id DESC
                LIMIT 100
                "#,
            )
            .map_err(|_| store_error())?;
        let rows = statement
            .query_map(params![owner_user_id, agent_id], |row| row.get(0))
            .map_err(|_| store_error())?;
        rows.collect::<Result<Vec<String>, _>>()
            .map_err(|_| store_error())
    }
}
