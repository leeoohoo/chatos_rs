impl LocalAgentRuntime {
    async fn authoritative_conversation_history(
        &self,
        command: chatos_local_agent_protocol::GetConversationHistoryCommand,
    ) -> Result<LocalConversationHistoryPage, LocalAgentRuntimeError> {
        let source = self
            .conversation_transcript_source
            .as_ref()
            .ok_or_else(|| {
                LocalAgentRuntimeError::InvalidRunState(
                    "Memory transcript source is required for conversation history".to_string(),
                )
            })?;
        let local = self
            .store
            .get_conversation_context(&command.owner_user_id, &command.conversation_id)
            .await?
            .ok_or_else(|| ClientStorageError::NotFound(command.conversation_id.clone()))?;
        let agent_id = match local.conversation.resource.as_ref() {
            Some(LocalConversationResourceBinding {
                kind: LocalConversationResourceKind::Project,
                ..
            }) => JIGULI_AGENT_ID,
            Some(LocalConversationResourceBinding {
                kind: LocalConversationResourceKind::Contact,
                resource_id,
            }) => resource_id.as_str(),
            None => {
                return Err(LocalAgentRuntimeError::InvalidRunState(
                    "Conversation transcript requires an Agent-bound resource".to_string(),
                ))
            }
        };
        let thread_id = format!("client-agent:agent:{agent_id}");
        // Snapshot the committed outbox before reading Memory. Reading it after
        // the HTTP response could lose an update that synced between the reads.
        let pending = if command.before_cursor.is_none() {
            self.store
                .list_pending_conversation_memory_records(
                    &command.owner_user_id,
                    &thread_id,
                    &command.conversation_id,
                )
                .await?
        } else {
            Vec::new()
        };
        let remote = source
            .list_conversation_records(
                &command.owner_user_id,
                &thread_id,
                &command.conversation_id,
                command.before_cursor.as_deref(),
                command.limit,
            )
            .await
            .map_err(LocalAgentRuntimeError::InvalidRunState)?;
        let mut records = remote.records;
        for pending in pending {
            let input: chatos_ai_runtime::SaveRecordInput = serde_json::from_value(pending.payload)
                .map_err(|error| {
                    LocalAgentRuntimeError::InvalidRunState(format!(
                        "decode committed conversation update failed: {error}"
                    ))
                })?;
            let metadata = input.packed_metadata();
            let created_at = match input.created_at {
                Some(created_at) => created_at,
                None => DateTime::<Utc>::from_timestamp_millis(pending.created_at_unix_ms)
                    .ok_or_else(|| {
                        LocalAgentRuntimeError::InvalidRunState(
                            "committed conversation update has an invalid durable timestamp"
                                .to_string(),
                        )
                    })?
                    .to_rfc3339(),
            };
            records.push(AuthoritativeConversationRecord {
                record_id: input.message_id.unwrap_or(pending.record_id),
                role: input.role,
                content: input.content,
                structured_payload: input.structured_payload,
                metadata,
                created_at,
            });
        }
        let mut messages = authoritative_messages(&command.conversation_id, &records)?;
        let mut attachments = authoritative_attachments(&command.conversation_id, &messages)?;
        if command.before_cursor.is_none() {
            let local_page = self
                .store
                .get_conversation_history(
                    &command.owner_user_id,
                    &command.conversation_id,
                    None,
                    command.limit,
                )
                .await?;
            merge_current_optimistic_input(
                &mut messages,
                &mut attachments,
                local_page,
                local.turns.as_slice(),
            );
        }
        let turns = authoritative_turns(
            &command.conversation_id,
            local.turns.as_slice(),
            messages.as_slice(),
        );
        Ok(LocalConversationHistoryPage {
            conversation: local.conversation,
            turns,
            messages,
            attachments,
            next_before_cursor: remote.next_before_cursor,
        })
    }
}

fn authoritative_attachments(
    conversation_id: &str,
    messages: &[LocalConversationMessageRecord],
) -> Result<Vec<LocalConversationAttachmentRecord>, LocalAgentRuntimeError> {
    let mut attachments = Vec::new();
    for message in messages {
        let Some(value) = message.metadata.get("ui_attachments") else {
            continue;
        };
        let specs = serde_json::from_value::<Vec<LocalConversationAttachmentSpec>>(value.clone())
            .map_err(|error| {
            LocalAgentRuntimeError::InvalidRunState(format!(
                "Memory transcript attachment metadata is invalid for {}: {error}",
                message.message_id
            ))
        })?;
        for (index, spec) in specs.into_iter().enumerate() {
            attachments.push(LocalConversationAttachmentRecord {
                attachment_id: spec.attachment_id,
                conversation_id: conversation_id.to_string(),
                turn_id: message.turn_id.clone(),
                message_id: message.message_id.clone(),
                ordinal: u64::try_from(index).unwrap_or(u64::MAX).saturating_add(1),
                display_name: spec.display_name,
                media_type: spec.media_type,
                byte_size: spec.byte_size,
                sha256: spec.sha256,
                authorized_local_ref: spec.authorized_local_ref,
                metadata: spec.metadata,
                created_at_unix_ms: message.created_at_unix_ms,
            });
        }
    }
    Ok(attachments)
}

fn merge_current_optimistic_input(
    messages: &mut Vec<LocalConversationMessageRecord>,
    attachments: &mut Vec<LocalConversationAttachmentRecord>,
    local_page: LocalConversationHistoryPage,
    local_turns: &[LocalConversationTurnRecord],
) {
    let running_turn_ids = local_turns
        .iter()
        .filter(|turn| turn.status == LocalConversationTurnStatus::Running)
        .map(|turn| turn.turn_id.clone())
        .collect::<HashSet<_>>();
    let mut known_message_ids = messages
        .iter()
        .map(|message| message.message_id.clone())
        .collect::<HashSet<_>>();
    let optimistic = local_page
        .messages
        .into_iter()
        .filter(|message| {
            message.role == LocalConversationMessageRole::User
                && running_turn_ids.contains(&message.turn_id)
                && known_message_ids.insert(message.message_id.clone())
        })
        .collect::<Vec<_>>();
    let optimistic_ids = optimistic
        .iter()
        .map(|message| message.message_id.clone())
        .collect::<HashSet<_>>();
    attachments.extend(
        local_page
            .attachments
            .into_iter()
            .filter(|attachment| optimistic_ids.contains(&attachment.message_id)),
    );
    messages.extend(optimistic);
    messages.sort_by(|left, right| {
        left.created_at_unix_ms
            .cmp(&right.created_at_unix_ms)
            .then_with(|| left.message_id.cmp(&right.message_id))
    });
}

fn authoritative_messages(
    conversation_id: &str,
    records: &[AuthoritativeConversationRecord],
) -> Result<Vec<LocalConversationMessageRecord>, LocalAgentRuntimeError> {
    let mut snapshots =
        HashMap::<String, ((i64, bool, String), LocalConversationMessageRecord)>::new();
    for record in records {
        let mut metadata = record.metadata.clone().unwrap_or_else(|| json!({}));
        if let Some(ui_metadata) = metadata
            .get("ui_message_metadata")
            .and_then(serde_json::Value::as_object)
            .cloned()
        {
            let metadata_object = metadata.as_object_mut().ok_or_else(|| {
                LocalAgentRuntimeError::InvalidRunState(format!(
                    "Memory transcript metadata is invalid for {}",
                    record.record_id
                ))
            })?;
            metadata_object.extend(ui_metadata);
        }
        if metadata
            .get("response_status")
            .and_then(serde_json::Value::as_str)
            == Some("tool_calls")
            || !matches!(record.role.as_str(), "user" | "assistant")
        {
            continue;
        }
        let Some(turn_id) = metadata
            .get("conversation_turn_id")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let message_id = metadata
            .get("ui_message_id")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(record.record_id.as_str())
            .to_string();
        let created_at = DateTime::parse_from_rfc3339(record.created_at.trim())
            .map_err(|_| {
                LocalAgentRuntimeError::InvalidRunState(format!(
                    "Memory transcript record has invalid created_at: {}",
                    record.record_id
                ))
            })?
            .with_timezone(&Utc);
        let created_at_unix_ms = created_at.timestamp_millis();
        let ordinal = authoritative_ordinal(created_at_unix_ms, message_id.as_str());
        // Several immutable Memory snapshots can represent one mutable UI card.
        // Callback time orders task states; survey snapshots carry their update
        // time in created_at. Never let hash/HTTP record order pick the UI state.
        let callback = metadata.get("task_runner_async");
        let snapshot_at = callback
            .and_then(|value| value.get("callback_at_unix_ms"))
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(created_at_unix_ms);
        let terminal = callback
            .and_then(|value| value.get("event"))
            .and_then(serde_json::Value::as_str)
            .is_some_and(|event| {
                matches!(
                    event,
                    "task.completed" | "task.failed" | "task.blocked" | "task.cancelled"
                )
            });
        let revision = (snapshot_at, terminal, record.record_id.clone());
        let content = match record.role.as_str() {
            "user" => json!({"text": record.content}),
            _ => record
                .structured_payload
                .clone()
                .filter(|_| {
                    metadata
                        .get("ui_structured_content")
                        .and_then(serde_json::Value::as_bool)
                        == Some(true)
                })
                .unwrap_or_else(|| serde_json::Value::String(record.content.clone())),
        };
        let message = LocalConversationMessageRecord {
            message_id: message_id.clone(),
            conversation_id: conversation_id.to_string(),
            turn_id: turn_id.to_string(),
            ordinal,
            role: if record.role == "assistant" {
                LocalConversationMessageRole::Assistant
            } else {
                LocalConversationMessageRole::User
            },
            content,
            metadata,
            created_at_unix_ms,
        };
        match snapshots.entry(message_id) {
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert((revision, message));
            }
            std::collections::hash_map::Entry::Occupied(mut entry) => {
                let (previous_revision, previous) = entry.get_mut();
                let first_created_at = previous.created_at_unix_ms.min(created_at_unix_ms);
                if revision > *previous_revision {
                    *previous_revision = revision;
                    *previous = message;
                }
                previous.created_at_unix_ms = first_created_at;
                previous.ordinal = authoritative_ordinal(first_created_at, &previous.message_id);
            }
        }
    }
    let mut messages = snapshots
        .into_values()
        .map(|(_, message)| message)
        .collect::<Vec<_>>();
    messages.sort_by(|left, right| {
        left.created_at_unix_ms
            .cmp(&right.created_at_unix_ms)
            .then_with(|| left.message_id.cmp(&right.message_id))
    });
    Ok(messages)
}

fn authoritative_ordinal(created_at_unix_ms: i64, record_id: &str) -> u64 {
    let millis = u64::try_from(created_at_unix_ms.max(1)).unwrap_or(1);
    let suffix = record_id.bytes().fold(0_u64, |value, byte| {
        value.wrapping_mul(31).wrapping_add(u64::from(byte))
    }) % 1_000;
    millis.saturating_mul(1_000).saturating_add(suffix)
}

fn authoritative_turns(
    conversation_id: &str,
    local_turns: &[LocalConversationTurnRecord],
    messages: &[LocalConversationMessageRecord],
) -> Vec<LocalConversationTurnRecord> {
    let local_by_id = local_turns
        .iter()
        .map(|turn| (turn.turn_id.as_str(), turn))
        .collect::<HashMap<_, _>>();
    let mut messages_by_turn: HashMap<&str, Vec<&LocalConversationMessageRecord>> = HashMap::new();
    for message in messages {
        messages_by_turn
            .entry(message.turn_id.as_str())
            .or_default()
            .push(message);
    }
    let mut turns = messages_by_turn
        .into_iter()
        .filter_map(|(turn_id, turn_messages)| {
            if let Some(local) = local_by_id.get(turn_id) {
                return Some((*local).clone());
            }
            let user = turn_messages
                .iter()
                .find(|message| message.role == LocalConversationMessageRole::User)?;
            let created_at_unix_ms = turn_messages
                .iter()
                .map(|message| message.created_at_unix_ms)
                .min()
                .unwrap_or(user.created_at_unix_ms);
            let updated_at_unix_ms = turn_messages
                .iter()
                .map(|message| message.created_at_unix_ms)
                .max()
                .unwrap_or(created_at_unix_ms);
            Some(LocalConversationTurnRecord {
                turn_id: turn_id.to_string(),
                conversation_id: conversation_id.to_string(),
                user_message_id: user.message_id.clone(),
                run_id: format!("memory:{turn_id}"),
                status: LocalConversationTurnStatus::Succeeded,
                created_at_unix_ms,
                updated_at_unix_ms,
            })
        })
        .collect::<Vec<_>>();
    turns.sort_by(|left, right| {
        left.created_at_unix_ms
            .cmp(&right.created_at_unix_ms)
            .then_with(|| left.turn_id.cmp(&right.turn_id))
    });
    turns
}
