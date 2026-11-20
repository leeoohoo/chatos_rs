use super::*;

impl AgentConversationStore {
    #[cfg(test)]
    pub(super) fn queue_project_team_message(
        &self,
        owner_user_id: &str,
        team_id: &str,
        content: &str,
        mentioned_agent_ids: &[String],
    ) -> Result<QueueAgentTeamMessageResult, String> {
        self.queue_project_team_message_with_attachments(
            owner_user_id,
            team_id,
            content,
            mentioned_agent_ids,
            &[],
        )
    }

    pub(super) fn queue_project_team_message_with_attachments(
        &self,
        owner_user_id: &str,
        team_id: &str,
        content: &str,
        mentioned_agent_ids: &[String],
        attachments: &[crate::conversation_attachments::VerifiedAttachmentPayload],
    ) -> Result<QueueAgentTeamMessageResult, String> {
        validate_identifier(team_id).map_err(|_| invalid_team_input())?;
        validate_text(content, 1_000_000, true).map_err(|_| invalid_team_message())?;
        if content.is_empty() && attachments.is_empty() {
            return Err(invalid_team_message());
        }
        if mentioned_agent_ids.len() > 100 {
            return Err(invalid_team_message());
        }
        let mut unique_mentions = std::collections::HashSet::new();
        for agent_id in mentioned_agent_ids {
            validate_agent_identifier(agent_id).map_err(|_| invalid_team_message())?;
            if !unique_mentions.insert(agent_id.as_str()) {
                return Err(invalid_team_message());
            }
        }
        reject_link_or_non_file(&self.database)?;
        let mut connection = open_database(&self.database, false)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| team_store_error())?;
        let default_agent_id = transaction
            .query_row(
                "SELECT default_agent_id FROM project_agent_rooms \
                 WHERE owner_user_id = ?1 AND id = ?2 AND status = 'active' \
                   AND conversation_kind = 'project_team'",
                params![owner_user_id, team_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()
            .map_err(|_| team_store_error())?
            .ok_or_else(|| "Agent 团队不存在或已停用。".to_owned())?;
        let active_member_ids = {
            let mut statement = transaction
                .prepare(
                    "SELECT agent_id FROM project_agent_room_members \
                     WHERE owner_user_id = ?1 AND room_id = ?2 AND status = 'active'",
                )
                .map_err(|_| team_store_error())?;
            let rows = statement
                .query_map(params![owner_user_id, team_id], |row| {
                    row.get::<_, String>(0)
                })
                .map_err(|_| team_store_error())?;
            rows.collect::<Result<std::collections::HashSet<_>, _>>()
                .map_err(|_| team_store_error())?
        };
        if mentioned_agent_ids
            .iter()
            .any(|agent_id| !active_member_ids.contains(agent_id))
        {
            return Err("@Agent 必须是当前团队的活动成员。".into());
        }
        let targets = if mentioned_agent_ids.is_empty() {
            default_agent_id.into_iter().collect::<Vec<_>>()
        } else {
            mentioned_agent_ids.to_vec()
        };
        let message_id = uuid::Uuid::new_v4().to_string().to_lowercase();
        let now = chrono::Utc::now().timestamp_millis().max(0);
        transaction
            .execute(
                r#"
                INSERT INTO project_agent_messages (
                    owner_user_id, id, room_id, sender_kind, sender_id, content,
                    reply_to_message_id, source_run_id, causation_id, root_message_id,
                    hop_count, created_at_unix_ms
                ) VALUES (?1, ?2, ?3, 'human', ?1, ?4, NULL, NULL, NULL, ?2, 0, ?5)
                "#,
                params![owner_user_id, message_id, team_id, content, now],
            )
            .map_err(|_| team_store_error())?;
        for (position, agent_id) in mentioned_agent_ids.iter().enumerate() {
            transaction
                .execute(
                    "INSERT INTO project_agent_message_mentions (\
                     owner_user_id, message_id, agent_id, position) VALUES (?1, ?2, ?3, ?4)",
                    params![owner_user_id, message_id, agent_id, position as i64],
                )
                .map_err(|_| team_store_error())?;
        }
        let trigger_kind = if mentioned_agent_ids.is_empty() {
            "default_agent"
        } else {
            "mention"
        };
        let mut deliveries = Vec::with_capacity(targets.len());
        for target_agent_id in targets {
            let delivery_id = uuid::Uuid::new_v4().to_string().to_lowercase();
            transaction
                .execute(
                    r#"
                    INSERT INTO project_agent_deliveries (
                        owner_user_id, id, room_id, message_id, root_message_id,
                        target_agent_id, trigger_kind, status, attempt, hop_count,
                        deduplication_key, response_message_id, last_error,
                        claimed_at_unix_ms, completed_at_unix_ms, created_at_unix_ms
                    ) VALUES (?1, ?2, ?3, ?4, ?4, ?5, ?6, 'pending', 0, 0,
                              ?7, NULL, NULL, NULL, NULL, ?8)
                    "#,
                    params![
                        owner_user_id,
                        delivery_id,
                        team_id,
                        message_id,
                        target_agent_id,
                        trigger_kind,
                        format!("{message_id}:{target_agent_id}"),
                        now,
                    ],
                )
                .map_err(|_| team_store_error())?;
            deliveries.push(QueuedAgentTeamDelivery {
                delivery_id,
                target_agent_id,
                trigger_kind: trigger_kind.to_owned(),
            });
        }
        let persisted_attachment_paths = documents::persist_external_attachments(
            &transaction,
            &self.database,
            owner_user_id,
            &message_id,
            attachments,
        )?;
        if transaction.commit().is_err() {
            documents::remove_files(&persisted_attachment_paths);
            return Err(team_store_error());
        }
        Ok(QueueAgentTeamMessageResult {
            message_id,
            team_id: team_id.to_owned(),
            created_at_unix_ms: now,
            deliveries,
        })
    }

    pub(super) fn load_project_team_conversation(
        &self,
        owner_user_id: &str,
        team_id: &str,
        before_message_id: Option<&str>,
        limit: u32,
    ) -> Result<AgentTeamConversationSnapshot, String> {
        validate_identifier(team_id).map_err(|_| invalid_team_input())?;
        if let Some(message_id) = before_message_id {
            validate_identifier(message_id).map_err(|_| invalid_team_input())?;
        }
        let limit = limit.clamp(1, 200) as i64;
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
        let before_key = if let Some(message_id) = before_message_id {
            Some(
                connection
                    .query_row(
                        "SELECT created_at_unix_ms, id FROM project_agent_messages \
                         WHERE owner_user_id = ?1 AND room_id = ?2 AND id = ?3",
                        params![owner_user_id, team_id, message_id],
                        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
                    )
                    .optional()
                    .map_err(|_| team_store_error())?
                    .ok_or_else(|| "要加载的历史消息不存在。".to_owned())?,
            )
        } else {
            None
        };
        let message_rows = {
            let mut statement = connection
                .prepare(
                    r#"
                    SELECT id, sender_kind, sender_id, content, created_at_unix_ms
                    FROM (
                        SELECT id, sender_kind, sender_id, content, created_at_unix_ms
                        FROM project_agent_messages
                        WHERE owner_user_id = ?1 AND room_id = ?2
                          AND (
                            ?3 IS NULL
                            OR created_at_unix_ms < ?3
                            OR (created_at_unix_ms = ?3 AND id < ?4)
                          )
                        ORDER BY created_at_unix_ms DESC, id DESC LIMIT ?5
                    ) recent
                    ORDER BY created_at_unix_ms, id
                    "#,
                )
                .map_err(|_| team_store_error())?;
            let (before_created_at, before_id) = before_key
                .as_ref()
                .map(|(created_at, id)| (Some(*created_at), Some(id.as_str())))
                .unwrap_or((None, None));
            let rows = statement
                .query_map(
                    params![
                        owner_user_id,
                        team_id,
                        before_created_at,
                        before_id,
                        limit + 1
                    ],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, i64>(4)?,
                        ))
                    },
                )
                .map_err(|_| team_store_error())?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|_| team_store_error())?
        };
        let has_older = message_rows.len() as i64 > limit;
        let message_rows = if has_older {
            message_rows.into_iter().skip(1).collect::<Vec<_>>()
        } else {
            message_rows
        };
        let message_ids = message_rows
            .iter()
            .map(|row| row.0.as_str())
            .collect::<std::collections::HashSet<_>>();
        let mut mentions_by_message: std::collections::HashMap<String, Vec<(i64, String)>> =
            std::collections::HashMap::new();
        let mut attachments_by_message =
            std::collections::HashMap::<String, Vec<AgentTeamMessageAttachmentSummary>>::new();
        {
            let mut statement = connection
                .prepare(
                    r#"
                    SELECT mention.message_id, mention.position, mention.agent_id
                    FROM project_agent_message_mentions mention
                    JOIN project_agent_messages message
                      ON message.owner_user_id = mention.owner_user_id
                     AND message.id = mention.message_id
                    WHERE mention.owner_user_id = ?1 AND message.room_id = ?2
                    ORDER BY mention.message_id, mention.position, mention.agent_id
                    LIMIT 20000
                    "#,
                )
                .map_err(|_| team_store_error())?;
            let rows = statement
                .query_map(params![owner_user_id, team_id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })
                .map_err(|_| team_store_error())?;
            for row in rows {
                let (message_id, position, agent_id) = row.map_err(|_| team_store_error())?;
                if message_ids.contains(message_id.as_str()) {
                    mentions_by_message
                        .entry(message_id)
                        .or_default()
                        .push((position, agent_id));
                }
            }
        }
        {
            let mut statement = connection
                .prepare(
                    r#"
                    SELECT attachment.message_id, attachment.id, attachment.name,
                           attachment.mime_type, attachment.size_bytes, attachment.kind
                    FROM project_agent_message_attachments attachment
                    JOIN project_agent_messages message
                      ON message.owner_user_id = attachment.owner_user_id
                     AND message.id = attachment.message_id
                    WHERE attachment.owner_user_id = ?1 AND message.room_id = ?2
                    ORDER BY attachment.message_id, attachment.position
                    LIMIT 4000
                    "#,
                )
                .map_err(|_| team_store_error())?;
            let rows = statement
                .query_map(params![owner_user_id, team_id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        AgentTeamMessageAttachmentSummary {
                            id: row.get(1)?,
                            name: row.get(2)?,
                            mime_type: row.get(3)?,
                            size: row.get(4)?,
                            kind: row.get(5)?,
                        },
                    ))
                })
                .map_err(|_| team_store_error())?;
            for row in rows {
                let (message_id, attachment) = row.map_err(|_| team_store_error())?;
                if message_ids.contains(message_id.as_str()) {
                    attachments_by_message
                        .entry(message_id)
                        .or_default()
                        .push(attachment);
                }
            }
        }
        let mut messages = Vec::with_capacity(message_rows.len());
        for (id, sender_kind, sender_id, content, created_at_unix_ms) in message_rows {
            validate_identifier(&id).map_err(|_| team_store_error())?;
            validate_text(&content, 1_000_000, true).map_err(|_| team_store_error())?;
            let mentioned_agent_ids = mentions_by_message
                .remove(&id)
                .unwrap_or_default()
                .into_iter()
                .map(|(_, agent_id)| agent_id)
                .collect();
            let attachments = attachments_by_message.remove(&id).unwrap_or_default();
            messages.push(AgentTeamMessageSummary {
                id,
                sender_kind,
                sender_id,
                content,
                mentioned_agent_ids,
                attachments,
                created_at_unix_ms,
            });
        }
        let deliveries = {
            let mut statement = connection
                .prepare(
                    r#"
                    SELECT id, message_id, target_agent_id, trigger_kind, status,
                           response_message_id, last_error, created_at_unix_ms,
                           completed_at_unix_ms
                    FROM project_agent_deliveries
                    WHERE owner_user_id = ?1 AND room_id = ?2
                    ORDER BY created_at_unix_ms DESC, id DESC LIMIT 500
                    "#,
                )
                .map_err(|_| team_store_error())?;
            let rows = statement
                .query_map(params![owner_user_id, team_id], |row| {
                    Ok(AgentTeamDeliverySummary {
                        id: row.get(0)?,
                        message_id: row.get(1)?,
                        target_agent_id: row.get(2)?,
                        trigger_kind: row.get(3)?,
                        status: row.get(4)?,
                        response_message_id: row.get(5)?,
                        last_error: row.get(6)?,
                        created_at_unix_ms: row.get(7)?,
                        completed_at_unix_ms: row.get(8)?,
                    })
                })
                .map_err(|_| team_store_error())?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|_| team_store_error())?
        };
        Ok(AgentTeamConversationSnapshot {
            team_id: team_id.to_owned(),
            messages,
            deliveries,
            has_older,
        })
    }
}
