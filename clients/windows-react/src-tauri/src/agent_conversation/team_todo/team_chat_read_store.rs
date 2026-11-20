use super::*;

impl AgentConversationStore {
    pub(crate) fn team_chat_member_snapshot(
        &self,
        owner_user_id: &str,
        run_id: &str,
    ) -> Result<(AgentCommunicationToolBinding, Value), String> {
        validate_identifier(run_id).map_err(|_| team_todo_store_error())?;
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        let binding = load_agent_communication_tool_binding(&connection, owner_user_id, run_id)?;
        let mut statement = connection
            .prepare(
                r#"
                SELECT member.agent_id, profile.name, member.role, member.responsibility,
                       CASE WHEN room.project_manager_agent_id = member.agent_id
                            THEN 1 ELSE 0 END
                FROM project_agent_room_members member
                JOIN project_agent_rooms room
                  ON room.owner_user_id = member.owner_user_id
                 AND room.id = member.room_id
                JOIN local_agent_profiles profile
                  ON profile.owner_user_id = member.owner_user_id
                 AND profile.id = member.agent_id
                WHERE member.owner_user_id = ?1 AND member.room_id = ?2
                  AND member.status = 'active' AND profile.status = 'active'
                  AND room.status = 'active' AND room.conversation_kind = ?3
                ORDER BY member.joined_at_unix_ms, member.agent_id
                "#,
            )
            .map_err(|_| team_todo_store_error())?;
        let members = statement
            .query_map(
                params![owner_user_id, binding.room_id, binding.conversation_kind],
                |row| {
                    let agent_id = row.get::<_, String>(0)?;
                    Ok(json!({
                        "agent_ref": todo_planning_reference(
                            owner_user_id,
                            run_id,
                            "agent",
                            &agent_id,
                        ),
                        "name": row.get::<_, String>(1)?,
                        "role": row.get::<_, String>(2)?,
                        "responsibility": row.get::<_, String>(3)?,
                        "is_project_manager": row.get::<_, i64>(4)? == 1,
                        "is_current_agent": agent_id == binding.agent_id,
                    }))
                },
            )
            .map_err(|_| team_todo_store_error())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| team_todo_store_error())?;
        Ok((binding, Value::Array(members)))
    }

    pub(crate) fn read_all_unread_team_chat(
        &self,
        owner_user_id: &str,
        run_id: &str,
        request_key: &str,
        limit: usize,
        now_unix_ms: i64,
    ) -> Result<(AgentCommunicationToolBinding, Value), String> {
        validate_identifier(run_id).map_err(|_| team_todo_store_error())?;
        validate_text(request_key, 512, false).map_err(|_| invalid_team_todo_input())?;
        if !(1..=500).contains(&limit) || now_unix_ms < 0 {
            return Err(invalid_team_todo_input());
        }
        reject_link_or_non_file(&self.database)?;
        let mut connection = open_database(&self.database, false)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| team_todo_store_error())?;
        let binding = load_agent_communication_tool_binding(&transaction, owner_user_id, run_id)?;
        let existing_receipt = transaction
            .query_row(
                "SELECT requested_limit, output_json FROM windows_agent_chat_read_receipts \
                 WHERE owner_user_id = ?1 AND run_id = ?2 AND request_key = ?3",
                params![owner_user_id, run_id, request_key],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()
            .map_err(|_| team_todo_store_error())?;
        if let Some((requested_limit, output_json)) = existing_receipt {
            if requested_limit != limit as i64 {
                return Err(
                    "The tool invocation ID was reused with a different unread limit.".into(),
                );
            }
            let output = serde_json::from_str(&output_json).map_err(|_| team_todo_store_error())?;
            transaction.commit().map_err(|_| team_todo_store_error())?;
            return Ok((binding, output));
        }
        let rows = {
            let mut statement = transaction
                .prepare(
                    r#"
                    SELECT room.id, room.name, room.conversation_kind,
                           message.id, message.sender_kind, sender.name,
                           message.content, message.created_at_unix_ms
                    FROM project_agent_messages message
                    JOIN project_agent_rooms room
                      ON room.owner_user_id = message.owner_user_id
                     AND room.id = message.room_id
                    JOIN project_agent_room_members member
                      ON member.owner_user_id = message.owner_user_id
                     AND member.room_id = message.room_id AND member.agent_id = ?1
                    LEFT JOIN local_agent_profiles sender
                      ON sender.owner_user_id = message.owner_user_id
                     AND sender.id = message.sender_id AND message.sender_kind = 'agent'
                    LEFT JOIN project_agent_read_cursors cursor
                      ON cursor.owner_user_id = message.owner_user_id
                     AND cursor.room_id = message.room_id AND cursor.agent_id = ?1
                    WHERE message.owner_user_id = ?2 AND room.status = 'active'
                      AND room.conversation_kind IN ('project_team', 'agent_agent_direct')
                      AND member.status = 'active'
                      AND NOT (
                        message.sender_kind = 'system'
                        AND message.causation_id IN ('heartbeat', 'todo', 'todo_status')
                      )
                      AND NOT (message.sender_kind = 'agent' AND message.sender_id = ?1)
                      AND (
                        cursor.message_id IS NULL
                        OR message.created_at_unix_ms > cursor.message_created_at_unix_ms
                        OR (
                          message.created_at_unix_ms = cursor.message_created_at_unix_ms
                          AND message.id > cursor.message_id
                        )
                      )
                    ORDER BY message.created_at_unix_ms, message.id
                    LIMIT ?3
                    "#,
                )
                .map_err(|_| team_todo_store_error())?;
            let rows = statement
                .query_map(
                    params![binding.agent_id, owner_user_id, limit as i64],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, String>(4)?,
                            row.get::<_, Option<String>>(5)?,
                            row.get::<_, String>(6)?,
                            row.get::<_, i64>(7)?,
                        ))
                    },
                )
                .map_err(|_| team_todo_store_error())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| team_todo_store_error())?;
            rows
        };
        let attachment_messages = rows
            .iter()
            .map(|row| (row.0.clone(), row.3.clone()))
            .collect::<Vec<_>>();
        let mut attachments_by_message = documents::attachment_summaries_for_messages(
            &transaction,
            owner_user_id,
            run_id,
            &attachment_messages,
            now_unix_ms,
        )?;
        let mut conversations: Vec<(String, String, String, Vec<Value>)> = Vec::new();
        let mut cursor_updates: Vec<(String, String, i64)> = Vec::new();
        for (
            room_id,
            room_name,
            conversation_kind,
            message_id,
            sender_kind,
            sender_name,
            content,
            created_at_unix_ms,
        ) in rows
        {
            let message_reference = persist_team_chat_message_reference(
                &transaction,
                owner_user_id,
                run_id,
                &room_id,
                &message_id,
                now_unix_ms,
            )?;
            let sender = match sender_kind.as_str() {
                "human" => "Human".to_owned(),
                "system" => "System".to_owned(),
                "agent" => sender_name.unwrap_or_else(|| "Agent".into()),
                _ => return Err(team_todo_store_error()),
            };
            let attachments = attachments_by_message
                .remove(&message_id)
                .unwrap_or_default();
            let message = json!({
                "message_ref": message_reference,
                "sender": sender,
                "content": content,
                "attachments": attachments,
                "created_at_unix_ms": created_at_unix_ms,
            });
            if let Some(conversation) = conversations
                .iter_mut()
                .find(|conversation| conversation.0 == room_id)
            {
                conversation.3.push(message);
            } else {
                conversations.push((room_id.clone(), room_name, conversation_kind, vec![message]));
            }
            if let Some(cursor) = cursor_updates.iter_mut().find(|cursor| cursor.0 == room_id) {
                cursor.1 = message_id;
                cursor.2 = created_at_unix_ms;
            } else {
                cursor_updates.push((room_id, message_id, created_at_unix_ms));
            }
        }
        for (room_id, message_id, message_created_at_unix_ms) in &cursor_updates {
            transaction
                .execute(
                    r#"
                    INSERT INTO project_agent_read_cursors (
                        owner_user_id, room_id, agent_id, message_id,
                        message_created_at_unix_ms, updated_at_unix_ms
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                    ON CONFLICT(owner_user_id, room_id, agent_id) DO UPDATE SET
                        message_id = excluded.message_id,
                        message_created_at_unix_ms = excluded.message_created_at_unix_ms,
                        updated_at_unix_ms = excluded.updated_at_unix_ms
                    WHERE excluded.message_created_at_unix_ms
                              > project_agent_read_cursors.message_created_at_unix_ms
                       OR (excluded.message_created_at_unix_ms
                              = project_agent_read_cursors.message_created_at_unix_ms
                           AND excluded.message_id > project_agent_read_cursors.message_id)
                    "#,
                    params![
                        owner_user_id,
                        room_id,
                        binding.agent_id,
                        message_id,
                        message_created_at_unix_ms,
                        now_unix_ms.max(*message_created_at_unix_ms),
                    ],
                )
                .map_err(|_| team_todo_store_error())?;
        }
        let message_count = conversations
            .iter()
            .map(|conversation| conversation.3.len())
            .sum::<usize>();
        let output = json!({
            "conversations": conversations
                .into_iter()
                .map(|(_, name, kind, messages)| json!({
                    "name": name,
                    "kind": kind,
                    "messages": messages,
                }))
                .collect::<Vec<_>>(),
            "message_count": message_count,
            "marked_read": true,
        });
        let output_json = serde_json::to_string(&output).map_err(|_| team_todo_store_error())?;
        transaction
            .execute(
                "INSERT INTO windows_agent_chat_read_receipts (owner_user_id, run_id, \
                 request_key, requested_limit, output_json, created_at_unix_ms) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    owner_user_id,
                    run_id,
                    request_key,
                    limit as i64,
                    output_json,
                    now_unix_ms,
                ],
            )
            .map_err(|_| team_todo_store_error())?;
        transaction.commit().map_err(|_| team_todo_store_error())?;
        Ok((binding, output))
    }
}
