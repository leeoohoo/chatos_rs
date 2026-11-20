use super::*;

impl AgentConversationStore {
    pub(crate) fn send_team_chat_message(
        &self,
        owner_user_id: &str,
        run_id: &str,
        request_key: &str,
        content: &str,
        reply_reference: Option<&str>,
        target_agent_reference: Option<&str>,
        team_reference: Option<&str>,
        document_references: &[String],
        mention_references: &[String],
        now_unix_ms: i64,
    ) -> Result<(AgentCommunicationToolBinding, Value), String> {
        validate_identifier(run_id).map_err(|_| team_todo_store_error())?;
        validate_text(request_key, 512, false).map_err(|_| invalid_team_todo_input())?;
        validate_text(content, 2_000, false).map_err(|_| invalid_team_todo_input())?;
        if let Some(reference) = reply_reference {
            validate_text(reference, 600, false).map_err(|_| invalid_team_todo_input())?;
        }
        if let Some(reference) = target_agent_reference {
            validate_text(reference, 600, false).map_err(|_| invalid_team_todo_input())?;
        }
        if let Some(reference) = team_reference {
            validate_text(reference, 600, false).map_err(|_| invalid_team_todo_input())?;
        }
        if [
            reply_reference.is_some(),
            target_agent_reference.is_some(),
            team_reference.is_some(),
        ]
        .into_iter()
        .filter(|selected| *selected)
        .count()
            > 1
        {
            return Err(
                "reply_to_message_ref, target_agent_ref and team_ref are mutually exclusive."
                    .into(),
            );
        }
        validate_reference_list(mention_references, 64)?;
        validate_reference_list(
            document_references,
            documents::MAXIMUM_DOCUMENTS_PER_MESSAGE,
        )?;
        if now_unix_ms < 0 {
            return Err(invalid_team_todo_input());
        }
        reject_link_or_non_file(&self.database)?;
        let mut connection = open_database(&self.database, false)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| team_todo_store_error())?;
        let binding = load_agent_communication_tool_binding(&transaction, owner_user_id, run_id)?;
        let trigger_context: (String, String, String, i64) = transaction
            .query_row(
                r#"
                SELECT delivery.room_id, delivery.message_id,
                       delivery.root_message_id, delivery.hop_count
                FROM windows_agent_delivery_runs run
                JOIN project_agent_deliveries delivery
                  ON delivery.owner_user_id = run.owner_user_id
                 AND delivery.id = run.delivery_id
                WHERE run.owner_user_id = ?1 AND run.run_id = ?2
                  AND run.status IN ('ready', 'running')
                  AND delivery.status = 'running'
                  AND delivery.room_id = ?3 AND delivery.target_agent_id = ?4
                  AND delivery.trigger_kind != 'todo'
                "#,
                params![owner_user_id, run_id, binding.room_id, binding.agent_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .map_err(|_| team_todo_store_error())?;
        let (target_room_id, reply_message_id, inherited_root_message_id, hop_count) =
            if let Some(reference) = reply_reference {
                let (room_id, message_id, root_message_id, message_hop_count) =
                    resolve_team_message_reference(
                        &transaction,
                        owner_user_id,
                        run_id,
                        &binding.agent_id,
                        reference,
                    )?;
                (
                    room_id,
                    Some(message_id),
                    Some(root_message_id),
                    message_hop_count,
                )
            } else if let Some(reference) = target_agent_reference {
                let target_agent_id =
                    resolve_agent_reference(&transaction, owner_user_id, run_id, reference)?;
                if target_agent_id == binding.agent_id {
                    return Err("The target Agent reference points to the current Agent.".into());
                }
                (
                    open_agent_direct(
                        &transaction,
                        owner_user_id,
                        &binding.agent_id,
                        &target_agent_id,
                        now_unix_ms,
                    )?,
                    None,
                    None,
                    trigger_context.3,
                )
            } else if let Some(reference) = team_reference {
                (
                    resolve_team_reference(
                        &transaction,
                        owner_user_id,
                        run_id,
                        &binding.agent_id,
                        reference,
                    )?,
                    None,
                    None,
                    trigger_context.3,
                )
            } else {
                (
                    trigger_context.0,
                    Some(trigger_context.1),
                    Some(trigger_context.2),
                    trigger_context.3,
                )
            };
        let mention_agent_ids = resolve_team_agent_references(
            &transaction,
            owner_user_id,
            run_id,
            &target_room_id,
            mention_references,
        )?
        .into_iter()
        .filter(|agent_id| agent_id != &binding.agent_id)
        .collect::<Vec<_>>();
        let target_conversation_kind: String = transaction
            .query_row(
                "SELECT conversation_kind FROM project_agent_rooms WHERE owner_user_id = ?1 \
                 AND id = ?2 AND status = 'active'",
                params![owner_user_id, target_room_id],
                |row| row.get(0),
            )
            .map_err(|_| team_todo_store_error())?;
        let delivery_agent_ids = if target_conversation_kind == "agent_agent_direct" {
            active_room_member_ids(&transaction, owner_user_id, &target_room_id)?
                .into_iter()
                .filter(|agent_id| agent_id != &binding.agent_id)
                .collect::<Vec<_>>()
        } else if target_conversation_kind == "project_team" {
            mention_agent_ids.clone()
        } else {
            return Err("The target conversation does not support Agent messaging.".into());
        };
        let existing_message_id = transaction
            .query_row(
                r#"
                SELECT id, room_id, content, reply_to_message_id,
                       root_message_id, hop_count
                FROM project_agent_messages
                WHERE owner_user_id = ?1 AND sender_kind = 'agent'
                  AND sender_id = ?2 AND source_run_id = ?3 AND causation_id = ?4
                ORDER BY created_at_unix_ms, id LIMIT 1
                "#,
                params![owner_user_id, binding.agent_id, run_id, request_key,],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, i64>(5)?,
                    ))
                },
            )
            .optional()
            .map_err(|_| team_todo_store_error())?;
        if let Some((
            message_id,
            existing_room_id,
            existing_content,
            existing_reply_id,
            existing_root_message_id,
            existing_hop_count,
        )) = existing_message_id
        {
            let existing_mentions = {
                let mut statement = transaction
                    .prepare(
                        "SELECT agent_id FROM project_agent_message_mentions \
                         WHERE owner_user_id = ?1 AND message_id = ?2 ORDER BY position",
                    )
                    .map_err(|_| team_todo_store_error())?;
                let rows = statement
                    .query_map(params![owner_user_id, message_id], |row| {
                        row.get::<_, String>(0)
                    })
                    .map_err(|_| team_todo_store_error())?;
                rows.collect::<Result<Vec<_>, _>>()
                    .map_err(|_| team_todo_store_error())?
            };
            let existing_documents =
                documents::existing_document_references(&transaction, owner_user_id, &message_id)?;
            if existing_room_id != target_room_id
                || existing_content != content
                || existing_reply_id != reply_message_id
                || existing_mentions != mention_agent_ids
                || existing_documents != document_references
            {
                return Err(
                    "The tool invocation ID was reused with different message input.".into(),
                );
            }
            let spawned_delivery_count: i64 = transaction
                .query_row(
                    "SELECT count(*) FROM project_agent_deliveries WHERE owner_user_id = ?1 \
                     AND message_id = ?2",
                    params![owner_user_id, message_id],
                    |row| row.get(0),
                )
                .map_err(|_| team_todo_store_error())?;
            let routing_stop_reason = if spawned_delivery_count == 0 {
                team_chat_routing_stop_reason(
                    &transaction,
                    owner_user_id,
                    &existing_root_message_id,
                    existing_hop_count,
                    delivery_agent_ids.len(),
                )?
            } else {
                None
            };
            let message_reference = persist_team_chat_message_reference(
                &transaction,
                owner_user_id,
                run_id,
                &existing_room_id,
                &message_id,
                now_unix_ms,
            )?;
            transaction.commit().map_err(|_| team_todo_store_error())?;
            return Ok((
                binding,
                json!({
                    "message_ref": message_reference,
                    "completed": false,
                    "spawned_delivery_count": spawned_delivery_count,
                    "routing_stop_reason": routing_stop_reason,
                }),
            ));
        }
        let document_drafts = documents::load_document_drafts(
            &transaction,
            &self.database,
            owner_user_id,
            run_id,
            document_references,
        )?;
        let message_id = identifier("message");
        let root_message_id = inherited_root_message_id.unwrap_or_else(|| message_id.clone());
        let message_hop_count = hop_count.saturating_add(1);
        let routing_stop_reason = team_chat_routing_stop_reason(
            &transaction,
            owner_user_id,
            &root_message_id,
            message_hop_count,
            delivery_agent_ids.len(),
        )?;
        transaction
            .execute(
                r#"
                INSERT INTO project_agent_messages (
                    owner_user_id, id, room_id, sender_kind, sender_id, content,
                    reply_to_message_id, source_run_id, causation_id, root_message_id,
                    hop_count, created_at_unix_ms
                ) VALUES (?1, ?2, ?3, 'agent', ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                "#,
                params![
                    owner_user_id,
                    message_id,
                    target_room_id,
                    binding.agent_id,
                    content,
                    reply_message_id,
                    run_id,
                    request_key,
                    root_message_id,
                    message_hop_count,
                    now_unix_ms,
                ],
            )
            .map_err(|_| team_todo_store_error())?;
        for (position, target_agent_id) in mention_agent_ids.iter().enumerate() {
            transaction
                .execute(
                    "INSERT INTO project_agent_message_mentions (owner_user_id, message_id, \
                     agent_id, position) VALUES (?1, ?2, ?3, ?4)",
                    params![owner_user_id, message_id, target_agent_id, position as i64],
                )
                .map_err(|_| team_todo_store_error())?;
        }
        if routing_stop_reason.is_none() {
            for target_agent_id in &delivery_agent_ids {
                let delivery_id = identifier("delivery");
                let trigger_kind = if mention_agent_ids.contains(target_agent_id) {
                    "agent_mention"
                } else {
                    "default_agent"
                };
                transaction
                    .execute(
                        r#"
                    INSERT INTO project_agent_deliveries (
                        owner_user_id, id, room_id, message_id, root_message_id,
                        target_agent_id, trigger_kind, status, attempt, hop_count,
                        deduplication_key, response_message_id, last_error,
                        claimed_at_unix_ms, completed_at_unix_ms, created_at_unix_ms
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'pending', 0, ?8,
                              ?9, NULL, NULL, NULL, NULL, ?10)
                    "#,
                        params![
                            owner_user_id,
                            delivery_id,
                            target_room_id,
                            message_id,
                            root_message_id,
                            target_agent_id,
                            trigger_kind,
                            message_hop_count,
                            format!("{message_id}:{target_agent_id}"),
                            now_unix_ms,
                        ],
                    )
                    .map_err(|_| team_todo_store_error())?;
            }
        }
        let message_reference = persist_team_chat_message_reference(
            &transaction,
            owner_user_id,
            run_id,
            &target_room_id,
            &message_id,
            now_unix_ms,
        )?;
        let persisted_document_paths = documents::persist_document_attachments(
            &transaction,
            &self.database,
            owner_user_id,
            run_id,
            &message_id,
            &document_drafts,
            now_unix_ms,
        )?;
        if transaction.commit().is_err() {
            documents::remove_files(&persisted_document_paths);
            return Err(team_todo_store_error());
        }
        documents::remove_consumed_draft_files(&self.database, &document_drafts);
        Ok((
            binding,
            json!({
                "message_ref": message_reference,
                "completed": false,
                "spawned_delivery_count": if routing_stop_reason.is_some() {
                    0
                } else {
                    delivery_agent_ids.len()
                },
                "routing_stop_reason": routing_stop_reason,
            }),
        ))
    }
}
