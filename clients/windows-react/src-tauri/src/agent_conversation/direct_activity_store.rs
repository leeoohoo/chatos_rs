use super::*;

impl AgentConversationStore {
    pub(super) fn record_direct_delivery(
        &self,
        owner_user_id: &str,
        binding: &AgentExecutionBinding,
        delivery_id: &str,
        message_id: &str,
        turn_id: &str,
        run_id: &str,
        content: &str,
        now_unix_ms: i64,
    ) -> Result<(), String> {
        for value in [delivery_id, message_id, turn_id, run_id] {
            validate_identifier(value)?;
        }
        validate_text(content, 1_000_000, true)?;
        if now_unix_ms < 0 {
            return Err(store_error());
        }
        reject_link_or_non_file(&self.database)?;
        let mut connection = open_database(&self.database, false)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| store_error())?;
        transaction
            .execute(
                r#"
                INSERT INTO project_agent_messages (
                    owner_user_id, id, room_id, sender_kind, sender_id, content,
                    reply_to_message_id, source_run_id, causation_id, root_message_id,
                    hop_count, created_at_unix_ms
                ) VALUES (?1, ?2, ?3, 'human', ?1, ?4, NULL, NULL, NULL, ?2, 0, ?5)
                "#,
                params![
                    owner_user_id,
                    message_id,
                    binding.room_id,
                    content,
                    now_unix_ms,
                ],
            )
            .map_err(|_| store_error())?;
        transaction
            .execute(
                r#"
                INSERT INTO project_agent_deliveries (
                    owner_user_id, id, room_id, message_id, root_message_id,
                    target_agent_id, trigger_kind, status, attempt, hop_count,
                    deduplication_key, response_message_id, last_error,
                    claimed_at_unix_ms, completed_at_unix_ms, created_at_unix_ms
                ) VALUES (?1, ?2, ?3, ?4, ?4, ?5, 'default_agent', 'pending', 0, 0,
                          ?6, NULL, NULL, NULL, NULL, ?7)
                "#,
                params![
                    owner_user_id,
                    delivery_id,
                    binding.room_id,
                    message_id,
                    binding.agent_id,
                    format!("{message_id}:{}", binding.agent_id),
                    now_unix_ms,
                ],
            )
            .map_err(|_| store_error())?;
        transaction
            .execute(
                r#"
                INSERT INTO windows_agent_delivery_runs (
                    owner_user_id, delivery_id, turn_id, run_id, status,
                    created_at_unix_ms, updated_at_unix_ms
                ) VALUES (?1, ?2, ?3, ?4, 'ready', ?5, ?5)
                "#,
                params![owner_user_id, delivery_id, turn_id, run_id, now_unix_ms],
            )
            .map_err(|_| store_error())?;
        transaction.commit().map_err(|_| store_error())
    }

    pub(super) fn reconcile_direct_activity(
        &self,
        owner_user_id: &str,
        binding: &AgentExecutionBinding,
        detail: &LocalConversationDetail,
    ) -> Result<Vec<AgentConversationActivityItem>, String> {
        reject_link_or_non_file(&self.database)?;
        let mut connection = open_database(&self.database, false)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| store_error())?;
        let mappings = {
            let mut statement = transaction
                .prepare(
                    r#"
                    SELECT delivery.id, delivery.message_id, run.turn_id, run.run_id,
                           delivery.status
                    FROM project_agent_deliveries delivery
                    JOIN windows_agent_delivery_runs run
                      ON run.owner_user_id = delivery.owner_user_id
                     AND run.delivery_id = delivery.id
                    WHERE delivery.owner_user_id = ?1 AND delivery.room_id = ?2
                      AND delivery.target_agent_id = ?3
                    ORDER BY delivery.created_at_unix_ms, delivery.id
                    "#,
                )
                .map_err(|_| store_error())?;
            let rows = statement
                .query_map(
                    params![owner_user_id, binding.room_id, binding.agent_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, String>(4)?,
                        ))
                    },
                )
                .map_err(|_| store_error())?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|_| store_error())?
        };
        for (delivery_id, root_message_id, turn_id, run_id, current_status) in mappings {
            let Some(turn) = detail
                .turns
                .iter()
                .find(|candidate| candidate.turn_id == turn_id && candidate.run_id == run_id)
            else {
                continue;
            };
            let (delivery_status, completed_at) = delivery_state(turn);
            let response = if turn.status == LocalConversationTurnStatus::Succeeded {
                detail
                    .messages
                    .iter()
                    .filter(|message| {
                        message.turn_id == turn_id
                            && message.role == LocalConversationMessageRole::Assistant
                    })
                    .max_by_key(|message| message.ordinal)
            } else {
                None
            };
            let response_message_id = if let Some(message) = response {
                validate_identifier(&message.message_id).map_err(|_| store_error())?;
                let content = projected_message_text(&message.content);
                transaction
                    .execute(
                        r#"
                        INSERT OR IGNORE INTO project_agent_messages (
                            owner_user_id, id, room_id, sender_kind, sender_id, content,
                            reply_to_message_id, source_run_id, causation_id, root_message_id,
                            hop_count, created_at_unix_ms
                        ) VALUES (?1, ?2, ?3, 'agent', ?4, ?5, ?6, ?7, ?6, ?6, 1, ?8)
                        "#,
                        params![
                            owner_user_id,
                            message.message_id,
                            binding.room_id,
                            binding.agent_id,
                            content,
                            root_message_id,
                            run_id,
                            message.created_at_unix_ms,
                        ],
                    )
                    .map_err(|_| store_error())?;
                Some(message.message_id.as_str())
            } else {
                None
            };
            if matches!(current_status.as_str(), "pending" | "running") {
                transaction
                    .execute(
                        r#"
                        UPDATE project_agent_deliveries
                        SET status = ?1,
                            attempt = CASE WHEN attempt = 0 THEN 1 ELSE attempt END,
                            response_message_id = COALESCE(?2, response_message_id),
                            claimed_at_unix_ms = COALESCE(claimed_at_unix_ms, ?3),
                            completed_at_unix_ms = ?4
                        WHERE owner_user_id = ?5 AND id = ?6
                          AND status IN ('pending', 'running')
                        "#,
                        params![
                            delivery_status,
                            response_message_id,
                            turn.created_at_unix_ms,
                            completed_at,
                            owner_user_id,
                            delivery_id,
                        ],
                    )
                    .map_err(|_| store_error())?;
            }
            transaction
                .execute(
                    r#"
                    UPDATE windows_agent_delivery_runs
                    SET status = ?1, updated_at_unix_ms = ?2
                    WHERE owner_user_id = ?3 AND delivery_id = ?4
                    "#,
                    params![
                        turn.status.as_str(),
                        turn.updated_at_unix_ms,
                        owner_user_id,
                        delivery_id,
                    ],
                )
                .map_err(|_| store_error())?;
        }
        transaction.commit().map_err(|_| store_error())?;
        self.list_direct_activity(owner_user_id, binding)
    }

    pub(super) fn list_direct_activity(
        &self,
        owner_user_id: &str,
        binding: &AgentExecutionBinding,
    ) -> Result<Vec<AgentConversationActivityItem>, String> {
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        let mut statement = connection
            .prepare(
                r#"
                SELECT delivery.id, delivery.room_id, delivery.target_agent_id,
                       run.turn_id, run.run_id, delivery.status, run.status,
                       delivery.response_message_id, delivery.created_at_unix_ms,
                       run.updated_at_unix_ms
                FROM project_agent_deliveries delivery
                JOIN windows_agent_delivery_runs run
                  ON run.owner_user_id = delivery.owner_user_id
                 AND run.delivery_id = delivery.id
                WHERE delivery.owner_user_id = ?1 AND delivery.room_id = ?2
                  AND delivery.target_agent_id = ?3
                ORDER BY run.updated_at_unix_ms DESC, run.run_id DESC
                LIMIT 100
                "#,
            )
            .map_err(|_| store_error())?;
        let rows = statement
            .query_map(
                params![owner_user_id, binding.room_id, binding.agent_id],
                |row| {
                    Ok(AgentConversationActivityItem {
                        delivery_id: row.get(0)?,
                        room_id: row.get(1)?,
                        agent_id: row.get(2)?,
                        turn_id: row.get(3)?,
                        run_id: row.get(4)?,
                        delivery_status: row.get(5)?,
                        run_status: row.get(6)?,
                        response_message_id: row.get(7)?,
                        created_at_unix_ms: row.get(8)?,
                        updated_at_unix_ms: row.get(9)?,
                    })
                },
            )
            .map_err(|_| store_error())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| store_error())
    }
}
