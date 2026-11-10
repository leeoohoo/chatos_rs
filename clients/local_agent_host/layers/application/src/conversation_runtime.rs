// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{new_event_id, LocalAgentRuntime, LocalAgentRuntimeError};
use chatos_local_agent_ports::{
    AuthoritativeConversationRecord, ClientStorageError, IdempotentCommand,
};
use chatos_local_agent_protocol::{
    CancelConversationTurnCommand, CreateConversationCommand, HostCommand, HostResult,
    LocalAgentRunRecord, LocalAgentRunStatus, LocalConversationAttachmentRecord,
    LocalConversationAttachmentSpec, LocalConversationHistoryPage, LocalConversationMessageRecord,
    LocalConversationMessageRole, LocalConversationResourceBinding, LocalConversationResourceKind,
    LocalConversationTurnRecord, LocalConversationTurnStatus, ResumeConversationTurnCommand,
    StartConversationTurnCommand,
};
use chrono::{DateTime, Utc};
use serde_json::json;
use std::collections::{HashMap, HashSet};
const JIGULI_AGENT_ID: &str = "jiguli";

impl LocalAgentRuntime {
    /// Returns only recent attachment metadata and opaque authorization references.
    /// Main Chat uses this catalog to select the minimum inputs for a Task; attachment
    /// bytes and base64 are never copied into the model-owned Task arguments.
    pub async fn recent_conversation_attachments(
        &self,
        owner_user_id: &str,
        conversation_id: &str,
    ) -> Result<Vec<LocalConversationAttachmentRecord>, LocalAgentRuntimeError> {
        Ok(self
            .authoritative_conversation_history(
                chatos_local_agent_protocol::GetConversationHistoryCommand {
                    owner_user_id: owner_user_id.to_string(),
                    conversation_id: conversation_id.to_string(),
                    before_cursor: None,
                    limit: 100,
                },
            )
            .await?
            .attachments)
    }

    /// Resolves one opaque reference inside the source conversation boundary.
    pub async fn conversation_attachment_by_ref(
        &self,
        owner_user_id: &str,
        conversation_id: &str,
        authorized_local_ref: &str,
    ) -> Result<Option<LocalConversationAttachmentRecord>, LocalAgentRuntimeError> {
        Ok(self
            .store
            .get_conversation_attachment(owner_user_id, conversation_id, authorized_local_ref)
            .await?)
    }

    pub(super) async fn handle_conversation_command(
        &self,
        idempotency: &IdempotentCommand,
        command: HostCommand,
    ) -> Result<HostResult, LocalAgentRuntimeError> {
        match command {
            HostCommand::CreateConversation(command) => Ok(HostResult::Conversation {
                conversation: self.create_conversation(idempotency, command).await?,
            }),
            HostCommand::GetConversation(command) => Ok(HostResult::Conversation {
                conversation: self
                    .store
                    .get_conversation(&command.owner_user_id, &command.conversation_id)
                    .await?
                    .ok_or(ClientStorageError::NotFound(command.conversation_id))?,
            }),
            HostCommand::GetConversationContext(command) => Ok(HostResult::Conversation {
                conversation: self
                    .store
                    .get_conversation_context(&command.owner_user_id, &command.conversation_id)
                    .await?
                    .ok_or(ClientStorageError::NotFound(command.conversation_id))?,
            }),
            HostCommand::GetConversationAttachment(command) => {
                Ok(HostResult::ConversationAttachment {
                    attachment: self
                        .store
                        .get_conversation_attachment(
                            &command.owner_user_id,
                            &command.conversation_id,
                            &command.authorized_local_ref,
                        )
                        .await?,
                })
            }
            HostCommand::GetConversationHistory(command) => Ok(HostResult::ConversationHistory {
                page: Box::new(self.authoritative_conversation_history(command).await?),
            }),
            HostCommand::ListConversations(command) => Ok(HostResult::Conversations {
                page: self
                    .store
                    .list_conversations(
                        &command.owner_user_id,
                        command.before_updated_at_unix_ms,
                        command.before_conversation_id.as_deref(),
                        command.limit,
                    )
                    .await?,
            }),
            HostCommand::StartConversationTurn(command) => {
                let result = self.start_conversation_turn(idempotency, command).await?;
                Ok(HostResult::ConversationTurnStarted {
                    result: Box::new(result),
                })
            }
            HostCommand::GuideConversationTurn(command) => {
                let result = self
                    .store
                    .guide_conversation_turn(idempotency, &command, &new_event_id(), self.now()?)
                    .await?;
                Ok(HostResult::ConversationTurnUpdated {
                    result: Box::new(result),
                })
            }
            HostCommand::ResumeConversationTurn(command) => {
                let result = self.resume_conversation_turn(idempotency, command).await?;
                Ok(HostResult::ConversationTurnUpdated {
                    result: Box::new(result),
                })
            }
            HostCommand::CancelConversationTurn(command) => {
                let result = self.cancel_conversation_turn(idempotency, command).await?;
                Ok(HostResult::ConversationTurnUpdated {
                    result: Box::new(result),
                })
            }
            _ => unreachable!("non-conversation command routed to conversation runtime"),
        }
    }

    async fn create_conversation(
        &self,
        idempotency: &IdempotentCommand,
        command: CreateConversationCommand,
    ) -> Result<chatos_local_agent_protocol::LocalConversationDetail, LocalAgentRuntimeError> {
        Ok(self
            .store
            .create_conversation(idempotency, &command, self.now()?)
            .await?)
    }

    async fn start_conversation_turn(
        &self,
        idempotency: &IdempotentCommand,
        command: StartConversationTurnCommand,
    ) -> Result<chatos_local_agent_protocol::LocalConversationTurnStart, LocalAgentRuntimeError>
    {
        let conversation = self
            .store
            .get_conversation(&command.owner_user_id, &command.conversation_id)
            .await?
            .ok_or_else(|| ClientStorageError::NotFound(command.conversation_id.clone()))?;
        let runtime_settings = self
            .store
            .get_conversation_runtime_settings(&command.owner_user_id, &command.conversation_id)
            .await?;
        if let Some(settings) = &runtime_settings {
            if settings.selected_model_config_ref != command.model_config_ref
                || settings.selected_model_config_revision != command.model_config_revision
            {
                return Err(LocalAgentRuntimeError::InvalidRequest(
                    "Turn model does not match the current conversation runtime settings"
                        .to_string(),
                ));
            }
        }
        let now = self.now()?;
        let explicit_capability_selection =
            normalized_explicit_capability_selection(&command.message_metadata)?;
        let team_executor_context = normalized_team_executor_context(
            &command.message_metadata,
            &command.conversation_id,
            conversation.conversation.resource.as_ref(),
        )?;
        let agent_execution_context = normalized_agent_execution_context(
            &command.message_metadata,
            &command.conversation_id,
            conversation.conversation.resource.as_ref(),
            team_executor_context.as_ref(),
        )?;
        if team_executor_context.is_some() && explicit_capability_selection.is_some() {
            return Err(invalid_agent_context(
                "executor turns must not contain a Main Chat capability selection",
            ));
        }
        let conversation_agent_identity =
            if agent_execution_context.is_none() && team_executor_context.is_none() {
                Some(normalized_conversation_agent_identity(
                    &command.message_metadata,
                    conversation.conversation.resource.as_ref(),
                )?)
            } else {
                None
            };
        let mut input = json!({
            "conversation_id": &command.conversation_id,
            "conversation_title": &conversation.conversation.title,
            "conversation_resource": &conversation.conversation.resource,
            "turn_id": &command.turn_id,
            "message_id": &command.message_id,
            "message": &command.message,
            "ui_message_metadata": &command.message_metadata,
            "attachments": &command.attachments,
            "runtime_settings": runtime_settings.as_ref().map(|settings| json!({
                "version": settings.version,
                "selected_thinking_level": settings.selected_thinking_level,
                "remote_connection_id": settings.remote_connection_id,
                "reasoning_enabled": settings.reasoning_enabled,
            })),
        });
        if let Some((agent_id, agent_name)) = conversation_agent_identity {
            let object = input
                .as_object_mut()
                .expect("conversation Run input is an object");
            object.insert("agent_id".to_string(), json!(agent_id));
            if let Some(agent_name) = agent_name {
                object.insert("agent_name".to_string(), json!(agent_name));
            }
        }
        if let Some(LocalConversationResourceBinding {
            kind: LocalConversationResourceKind::Project,
            resource_id,
        }) = conversation.conversation.resource.as_ref()
        {
            let object = input
                .as_object_mut()
                .expect("conversation Run input is an object");
            object.insert("project_id".to_string(), json!(resource_id));
            let metadata = command
                .message_metadata
                .as_object()
                .expect("validated message metadata is an object");
            if metadata.get("project_name").is_some() {
                object.insert(
                    "project_name".to_string(),
                    json!(agent_context_string(metadata, "project_name", 240, false)?),
                );
            }
        }
        if let Some(selection) = explicit_capability_selection {
            input
                .as_object_mut()
                .expect("conversation Run input is an object")
                .insert("explicit_capability_selection".to_string(), selection);
        }
        if let Some(context) = agent_execution_context {
            input
                .as_object_mut()
                .expect("conversation Run input is an object")
                .insert("agent_execution_context".to_string(), context);
        }
        let profile_key = if let Some(context) = team_executor_context {
            let object = input
                .as_object_mut()
                .expect("conversation Run input is an object");
            let todo_id = context
                .get("todo_id")
                .and_then(serde_json::Value::as_str)
                .expect("normalized executor context has todo_id");
            object.insert("todo_id".to_string(), json!(todo_id));
            object.insert(
                "memory_thread_id".to_string(),
                json!(format!("client-agent:todo:{todo_id}")),
            );
            object.insert("prompt".to_string(), json!(&command.message));
            object.insert("owned_paths".to_string(), json!([]));
            object.insert(
                "tool_options".to_string(),
                context
                    .get("tool_options")
                    .cloned()
                    .expect("normalized executor context has tool_options"),
            );
            object.insert("team_executor_context".to_string(), context);
            "task_execution"
        } else {
            "main_chat"
        };
        let run = LocalAgentRunRecord {
            run_id: command.run_id.clone(),
            owner_user_id: conversation.conversation.owner_user_id,
            owner_entity_type: "conversation_turn".to_string(),
            owner_entity_id: command.turn_id.clone(),
            profile_key: profile_key.to_string(),
            model_config_ref: command.model_config_ref.clone(),
            model_config_revision: command.model_config_revision.clone(),
            capability_policy_revision: command.capability_policy_revision.clone(),
            input,
            status: LocalAgentRunStatus::Queued,
            iteration: 0,
            model_attempt: 1,
            max_iterations: command.max_iterations,
            version: 1,
            claim_token: None,
            claim_until_unix_ms: None,
            next_attempt_at_unix_ms: None,
            pending_tool_batch: None,
            checkpoint: serde_json::Value::Null,
            continuation_input: None,
            terminal_outcome: None,
            created_at_unix_ms: now,
            updated_at_unix_ms: now,
        };
        Ok(self
            .store
            .start_conversation_turn(idempotency, &command, &run, &new_event_id(), now)
            .await?)
    }

    async fn resume_conversation_turn(
        &self,
        idempotency: &IdempotentCommand,
        command: ResumeConversationTurnCommand,
    ) -> Result<chatos_local_agent_protocol::LocalConversationTurnUpdate, LocalAgentRuntimeError>
    {
        let continuation_input = json!({
            "type": "resume",
            "reason": &command.reason,
            "input": {
                "message": &command.message,
                "attachments": &command.attachments,
            }
        });
        Ok(self
            .store
            .resume_conversation_turn(
                idempotency,
                &command,
                &continuation_input,
                &new_event_id(),
                self.now()?,
            )
            .await?)
    }

    async fn cancel_conversation_turn(
        &self,
        idempotency: &IdempotentCommand,
        command: CancelConversationTurnCommand,
    ) -> Result<chatos_local_agent_protocol::LocalConversationTurnUpdate, LocalAgentRuntimeError>
    {
        Ok(self
            .store
            .cancel_conversation_turn(idempotency, &command, &new_event_id(), self.now()?)
            .await?)
    }
}

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
        let mut messages =
            authoritative_messages(&command.conversation_id, remote.records.as_slice())?;
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

fn normalized_conversation_agent_identity(
    metadata: &serde_json::Value,
    conversation_resource: Option<&LocalConversationResourceBinding>,
) -> Result<(String, Option<String>), LocalAgentRuntimeError> {
    let values = metadata
        .as_object()
        .ok_or_else(|| invalid_agent_context("message metadata must be an object"))?;
    let agent_id = agent_context_string(values, "agent_id", 512, true)?;
    let conversation_resource = conversation_resource.ok_or_else(|| {
        invalid_agent_context("Ordinary Agent conversations require an Agent-bound resource")
    })?;
    match conversation_resource {
        LocalConversationResourceBinding {
            kind: LocalConversationResourceKind::Project,
            ..
        } if agent_id != JIGULI_AGENT_ID => {
            return Err(invalid_agent_context(
                "Project conversations must use the jiguli Agent identity",
            ));
        }
        LocalConversationResourceBinding {
            kind: LocalConversationResourceKind::Contact,
            resource_id,
        } if agent_id != resource_id => {
            return Err(invalid_agent_context(
                "Contact resource does not match the Agent identity",
            ));
        }
        _ => {}
    }
    let agent_name = values
        .get("agent_name")
        .map(|_| agent_context_string(values, "agent_name", 120, false))
        .transpose()?
        .map(ToOwned::to_owned);
    Ok((agent_id.to_string(), agent_name))
}

fn normalized_agent_execution_context(
    metadata: &serde_json::Value,
    conversation_id: &str,
    conversation_resource: Option<&LocalConversationResourceBinding>,
    team_executor_context: Option<&serde_json::Value>,
) -> Result<Option<serde_json::Value>, LocalAgentRuntimeError> {
    let Some(context) = metadata.get("agent_execution_context") else {
        return Ok(None);
    };
    let source = metadata
        .get("source")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| invalid_agent_context("source is required"))?;
    if !matches!(source, "native_agent_direct" | "native_agent_team") {
        return Err(invalid_agent_context(
            "source is not an Agent execution surface",
        ));
    }
    if conversation_resource.is_some() && team_executor_context.is_none() {
        return Err(invalid_agent_context(
            "Agent direct conversations must not use Contact or Project resource bindings",
        ));
    }
    let values = context
        .as_object()
        .ok_or_else(|| invalid_agent_context("context must be an object"))?;
    const DIRECT_KEYS: [&str; 12] = [
        "schema_version",
        "room_id",
        "agent_id",
        "agent_name",
        "role_prompt",
        "profession_key",
        "profession_label",
        "profession_skill_ref",
        "profession_skill_sha256",
        "profession_skill_markdown",
        "default_plugin_ids",
        "default_skill_ids",
    ];
    const TEAM_KEYS: [&str; 18] = [
        "schema_version",
        "room_id",
        "agent_id",
        "agent_name",
        "role_prompt",
        "profession_key",
        "profession_label",
        "profession_skill_ref",
        "profession_skill_sha256",
        "profession_skill_markdown",
        "default_plugin_ids",
        "default_skill_ids",
        "project_id",
        "team_name",
        "team_goal",
        "team_role",
        "team_responsibility",
        "is_project_manager",
    ];
    let schema_version = values
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| invalid_agent_context("schema_version is required"))?;
    let expected_keys: &[&str] = match (source, schema_version) {
        ("native_agent_direct", 1) => &DIRECT_KEYS,
        ("native_agent_team", 2) => &TEAM_KEYS,
        _ => {
            return Err(invalid_agent_context(
                "source and schema_version do not match",
            ))
        }
    };
    if values.len() != expected_keys.len()
        || values
            .keys()
            .any(|key| !expected_keys.contains(&key.as_str()))
    {
        return Err(invalid_agent_context(
            "context fields do not match the schema",
        ));
    }
    let room_id = agent_context_string(values, "room_id", 512, true)?;
    let agent_id = agent_context_string(values, "agent_id", 512, true)?;
    let expected_conversation_id = if let Some(executor) = team_executor_context {
        for (field, expected) in [("team_id", room_id), ("agent_id", agent_id)] {
            if executor.get(field).and_then(serde_json::Value::as_str) != Some(expected) {
                return Err(invalid_agent_context(&format!(
                    "executor {field} does not match the Agent identity"
                )));
            }
        }
        conversation_id.to_string()
    } else if source == "native_agent_team" {
        format!("agent-team-{room_id}-{agent_id}")
    } else {
        format!("agent-direct-{room_id}")
    };
    if conversation_id != expected_conversation_id {
        return Err(invalid_agent_context(
            "room_id does not match the Host conversation identity",
        ));
    }
    let agent_name = agent_context_string(values, "agent_name", 120, false)?;
    let role_prompt = agent_context_string(values, "role_prompt", 32_000, false)?;
    let profession_key = agent_context_string(values, "profession_key", 512, true)?;
    let profession_label = agent_context_string(values, "profession_label", 200, false)?;
    let profession_skill_ref = agent_context_string(values, "profession_skill_ref", 700, true)?;
    let profession_skill_sha256 =
        agent_context_string(values, "profession_skill_sha256", 64, true)?;
    if profession_skill_sha256.len() != 64
        || !profession_skill_sha256
            .bytes()
            .all(|value| value.is_ascii_digit() || (b'a'..=b'f').contains(&value))
    {
        return Err(invalid_agent_context(
            "profession_skill_sha256 must be lowercase SHA-256",
        ));
    }
    let profession_skill_markdown =
        agent_context_string(values, "profession_skill_markdown", 500_000, false)?;
    let default_plugin_ids = agent_context_identifiers(values, "default_plugin_ids")?;
    let default_skill_ids = agent_context_identifiers(values, "default_skill_ids")?;
    let mut normalized = json!({
        "schema_version": 1,
        "room_id": room_id,
        "agent_id": agent_id,
        "agent_name": agent_name,
        "role_prompt": role_prompt,
        "profession_key": profession_key,
        "profession_label": profession_label,
        "profession_skill_ref": profession_skill_ref,
        "profession_skill_sha256": profession_skill_sha256,
        "profession_skill_markdown": profession_skill_markdown,
        "default_plugin_ids": default_plugin_ids,
        "default_skill_ids": default_skill_ids,
    });
    if source == "native_agent_team" {
        let project_id = agent_context_string(values, "project_id", 512, true)?;
        if let Some(executor) = team_executor_context {
            if executor
                .get("project_id")
                .and_then(serde_json::Value::as_str)
                != Some(project_id)
            {
                return Err(invalid_agent_context(
                    "executor project_id does not match the Agent identity",
                ));
            }
        }
        let team_name = agent_context_string(values, "team_name", 160, false)?;
        let team_goal = agent_context_optional_text(values, "team_goal", 8_000)?;
        let team_role = agent_context_string(values, "team_role", 160, false)?;
        let team_responsibility =
            agent_context_optional_text(values, "team_responsibility", 8_000)?;
        let is_project_manager = values
            .get("is_project_manager")
            .and_then(serde_json::Value::as_bool)
            .ok_or_else(|| invalid_agent_context("is_project_manager must be a boolean"))?;
        let object = normalized
            .as_object_mut()
            .expect("normalized Agent context is an object");
        object.insert("schema_version".into(), json!(2));
        object.insert("project_id".into(), json!(project_id));
        object.insert("team_name".into(), json!(team_name));
        object.insert("team_goal".into(), json!(team_goal));
        object.insert("team_role".into(), json!(team_role));
        object.insert("team_responsibility".into(), json!(team_responsibility));
        object.insert("is_project_manager".into(), json!(is_project_manager));
    }
    Ok(Some(normalized))
}

fn normalized_team_executor_context(
    metadata: &serde_json::Value,
    conversation_id: &str,
    conversation_resource: Option<&LocalConversationResourceBinding>,
) -> Result<Option<serde_json::Value>, LocalAgentRuntimeError> {
    let Some(context) = metadata.get("agent_team_execution") else {
        return Ok(None);
    };
    if metadata.get("source").and_then(serde_json::Value::as_str) != Some("native_agent_team") {
        return Err(invalid_agent_context(
            "executor context requires the native_agent_team source",
        ));
    }
    let values = context
        .as_object()
        .ok_or_else(|| invalid_agent_context("agent_team_execution must be an object"))?;
    const KEYS: [&str; 9] = [
        "schema_version",
        "delivery_lane",
        "trigger_kind",
        "delivery_id",
        "todo_id",
        "team_id",
        "agent_id",
        "project_id",
        "execution_plan",
    ];
    if values.len() != KEYS.len()
        || values.keys().any(|key| !KEYS.contains(&key.as_str()))
        || values
            .get("schema_version")
            .and_then(serde_json::Value::as_u64)
            != Some(1)
        || values
            .get("delivery_lane")
            .and_then(serde_json::Value::as_str)
            != Some("executor")
        || values
            .get("trigger_kind")
            .and_then(serde_json::Value::as_str)
            != Some("todo")
    {
        return Err(invalid_agent_context(
            "executor context fields do not match the schema",
        ));
    }
    let delivery_id = agent_context_string(values, "delivery_id", 512, true)?;
    let todo_id = agent_context_string(values, "todo_id", 512, true)?;
    let team_id = agent_context_string(values, "team_id", 512, true)?;
    let agent_id = agent_context_string(values, "agent_id", 512, true)?;
    let project_id = agent_context_string(values, "project_id", 512, true)?;
    if conversation_id != format!("agent-team-executor-{delivery_id}") {
        return Err(invalid_agent_context(
            "delivery_id does not match the executor conversation identity",
        ));
    }
    if !matches!(
        conversation_resource,
        Some(LocalConversationResourceBinding {
            kind: LocalConversationResourceKind::Project,
            resource_id,
        }) if resource_id == project_id
    ) {
        return Err(invalid_agent_context(
            "executor conversation must be bound to its exact Project",
        ));
    }
    let plan = values
        .get("execution_plan")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| invalid_agent_context("execution_plan must be an object"))?;
    const PLAN_KEYS: [&str; 3] = ["requires_execution", "builtin_capabilities", "plugins"];
    if plan.len() != PLAN_KEYS.len() || plan.keys().any(|key| !PLAN_KEYS.contains(&key.as_str())) {
        return Err(invalid_agent_context(
            "execution_plan fields do not match the schema",
        ));
    }
    let requires_execution = plan
        .get("requires_execution")
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| invalid_agent_context("requires_execution must be a boolean"))?;
    let capabilities = plan
        .get("builtin_capabilities")
        .and_then(serde_json::Value::as_array)
        .filter(|values| values.len() <= 3)
        .ok_or_else(|| invalid_agent_context("builtin_capabilities must be an array"))?;
    let mut unique = std::collections::HashSet::new();
    let mut enabled_builtin_kinds = Vec::with_capacity(capabilities.len());
    for capability in capabilities {
        let capability = capability
            .as_str()
            .ok_or_else(|| invalid_agent_context("builtin_capabilities must contain strings"))?;
        let builtin_kind = match capability {
            "project_read" => "CodeMaintainerRead",
            "project_write" if requires_execution => "CodeMaintainerWrite",
            "terminal" if requires_execution => "TerminalController",
            _ => {
                return Err(invalid_agent_context(
                    "execution_plan contains an unauthorized capability",
                ))
            }
        };
        if !unique.insert(capability) {
            return Err(invalid_agent_context("builtin_capabilities must be unique"));
        }
        enabled_builtin_kinds.push(builtin_kind);
    }
    let plugins = plan
        .get("plugins")
        .and_then(serde_json::Value::as_array)
        .filter(|values| values.len() <= 32)
        .ok_or_else(|| invalid_agent_context("plugins must be an array"))?;
    let mut plugin_selection_ids = std::collections::HashSet::new();
    let mut normalized_plugins = Vec::with_capacity(plugins.len());
    let mut plugin_hints = Vec::with_capacity(plugins.len());
    let mut plugin_release_snapshots = Vec::with_capacity(plugins.len());
    for plugin in plugins {
        let plugin = plugin
            .as_object()
            .ok_or_else(|| invalid_agent_context("plugins must contain objects"))?;
        const PLUGIN_KEYS: [&str; 9] = [
            "installationId",
            "pluginId",
            "componentKey",
            "componentRevision",
            "releaseId",
            "releaseDigest",
            "manifestVersion",
            "displayName",
            "reason",
        ];
        if plugin.len() != PLUGIN_KEYS.len()
            || plugin
                .keys()
                .any(|key| !PLUGIN_KEYS.contains(&key.as_str()))
        {
            return Err(invalid_agent_context(
                "Plugin selection fields do not match the schema",
            ));
        }
        let installation_id = agent_context_string(plugin, "installationId", 1_024, true)?;
        let plugin_id = agent_context_string(plugin, "pluginId", 1_024, true)?;
        let component_key = agent_context_string(plugin, "componentKey", 1_024, true)?;
        let component_revision = agent_context_string(plugin, "componentRevision", 1_024, true)?;
        let release_id = agent_context_string(plugin, "releaseId", 1_024, true)?;
        let release_digest = agent_context_string(plugin, "releaseDigest", 512, true)?;
        let digest = release_digest
            .strip_prefix("sha256:")
            .unwrap_or(release_digest);
        if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(invalid_agent_context("releaseDigest is invalid"));
        }
        let manifest_version = agent_context_string(plugin, "manifestVersion", 128, false)?;
        let display_name = agent_context_string(plugin, "displayName", 240, false)?;
        let reason = agent_context_optional_text(plugin, "reason", 1_000)?;
        if !plugin_selection_ids.insert((plugin_id, component_key)) {
            return Err(invalid_agent_context("Plugin selections must be unique"));
        }
        normalized_plugins.push(json!({
            "installation_id": installation_id,
            "plugin_id": plugin_id,
            "component_key": component_key,
            "component_revision": component_revision,
            "release_id": release_id,
            "release_digest": release_digest,
            "manifest_version": manifest_version,
            "display_name": display_name,
            "reason": reason,
        }));
        plugin_hints.push(json!({
            "plugin_key": plugin_id,
            "component_key": component_key,
            "reason": reason,
        }));
        plugin_release_snapshots.push(json!({
            "plugin_key": plugin_id,
            "component_key": component_key,
            "plugin_id": plugin_id,
            "release_id": release_id,
            "version": manifest_version,
            "artifact_sha256": release_digest,
        }));
    }
    Ok(Some(json!({
        "schema_version": 1,
        "delivery_lane": "executor",
        "delivery_id": delivery_id,
        "todo_id": todo_id,
        "team_id": team_id,
        "agent_id": agent_id,
        "project_id": project_id,
        "execution_plan": {
            "requires_execution": requires_execution,
            "builtin_capabilities": capabilities,
            "plugins": normalized_plugins,
        },
        "tool_options": {
            "requires_execution": requires_execution,
            "enabled_builtin_kinds": enabled_builtin_kinds,
            "plugin_hints": plugin_hints,
            "plugin_release_snapshots": plugin_release_snapshots,
            "external_mcp_config_ids": [],
        },
    })))
}

fn agent_context_optional_text<'a>(
    values: &'a serde_json::Map<String, serde_json::Value>,
    field: &str,
    maximum: usize,
) -> Result<&'a str, LocalAgentRuntimeError> {
    let value = values
        .get(field)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| invalid_agent_context(&format!("{field} must be a string")))?;
    if value.trim() != value || value.chars().count() > maximum || value.contains('\0') {
        return Err(invalid_agent_context(&format!("{field} is invalid")));
    }
    Ok(value)
}

fn agent_context_string<'a>(
    values: &'a serde_json::Map<String, serde_json::Value>,
    field: &str,
    maximum: usize,
    identifier: bool,
) -> Result<&'a str, LocalAgentRuntimeError> {
    let value = values
        .get(field)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| invalid_agent_context(&format!("{field} must be a string")))?;
    let valid = !value.is_empty()
        && value.trim() == value
        && value.chars().count() <= maximum
        && !value.contains('\0')
        && (!identifier
            || value
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || "-_.:|".contains(character)));
    if !valid {
        return Err(invalid_agent_context(&format!("{field} is invalid")));
    }
    Ok(value)
}

fn agent_context_identifiers<'a>(
    values: &'a serde_json::Map<String, serde_json::Value>,
    field: &str,
) -> Result<Vec<&'a str>, LocalAgentRuntimeError> {
    let items = values
        .get(field)
        .and_then(serde_json::Value::as_array)
        .filter(|items| items.len() <= 100)
        .ok_or_else(|| invalid_agent_context(&format!("{field} must be an array")))?;
    let mut unique = std::collections::HashSet::new();
    let mut result = Vec::with_capacity(items.len());
    for item in items {
        let value = item
            .as_str()
            .ok_or_else(|| invalid_agent_context(&format!("{field} must contain strings")))?;
        let valid = !value.is_empty()
            && value.trim() == value
            && value.chars().count() <= 512
            && !value.chars().any(char::is_control);
        if !valid || !unique.insert(value) {
            return Err(invalid_agent_context(&format!(
                "{field} contains an invalid value"
            )));
        }
        result.push(value);
    }
    Ok(result)
}

fn invalid_agent_context(detail: &str) -> LocalAgentRuntimeError {
    LocalAgentRuntimeError::InvalidRequest(format!("invalid Agent execution context: {detail}"))
}

fn normalized_explicit_capability_selection(
    metadata: &serde_json::Value,
) -> Result<Option<serde_json::Value>, LocalAgentRuntimeError> {
    let Some(selection) = metadata.get("explicit_capability_selection") else {
        return Ok(None);
    };
    let values = selection.as_object().ok_or_else(|| {
        LocalAgentRuntimeError::InvalidRequest(
            "explicit_capability_selection must be an object".to_string(),
        )
    })?;
    let kind = values
        .get("kind")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            LocalAgentRuntimeError::InvalidRequest(
                "explicit_capability_selection.kind is required".to_string(),
            )
        })?;
    let display_name = values
        .get("display_name")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.chars().count() <= 200)
        .ok_or_else(|| {
            LocalAgentRuntimeError::InvalidRequest(
                "explicit_capability_selection.display_name must be 1..=200 characters".to_string(),
            )
        })?;
    match kind {
        "plugin" => {
            let plugin_key = values
                .get("plugin_key")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty() && value.chars().count() <= 256)
                .ok_or_else(|| {
                    LocalAgentRuntimeError::InvalidRequest(
                        "plugin selection requires a 1..=256 character plugin_key".to_string(),
                    )
                })?;
            let component_key = values
                .get("component_key")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty() && value.chars().count() <= 256)
                .ok_or_else(|| {
                    LocalAgentRuntimeError::InvalidRequest(
                        "plugin selection requires a 1..=256 character component_key".to_string(),
                    )
                })?;
            Ok(Some(json!({
                "kind": "plugin",
                "plugin_key": plugin_key,
                "component_key": component_key,
                "display_name": display_name,
            })))
        }
        "requirement_survey" => {
            if values
                .get("plugin_key")
                .is_some_and(|value| !value.is_null())
            {
                return Err(LocalAgentRuntimeError::InvalidRequest(
                    "requirement_survey selection must not contain plugin_key".to_string(),
                ));
            }
            Ok(Some(json!({
                "kind": "requirement_survey",
                "display_name": display_name,
            })))
        }
        _ => Err(LocalAgentRuntimeError::InvalidRequest(format!(
            "unsupported explicit capability selection kind: {kind}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chatos_client_storage::SqliteClientStorage;
    use chatos_local_agent_ports::{
        AuthoritativeConversationPage, LocalConversationTranscriptSource, LocalMemoryOutboxStore,
    };
    use chatos_local_agent_protocol::{
        CancelConversationTurnCommand, ClaimNextRunCommand, CommitStepCommand,
        GetConversationCommand, GetConversationHistoryCommand, GuideConversationTurnCommand,
        HostRequestEnvelope, ListConversationsCommand, LocalAgentStepOutcome,
        LocalConversationAttachmentSpec, LocalConversationMessageRole, LocalConversationTurnStatus,
        ResumeConversationTurnCommand, LOCAL_AGENT_PROTOCOL_VERSION,
    };
    use std::sync::Arc;

    struct TranscriptFixture;

    #[test]
    fn projection_history_keeps_latest_snapshot_regardless_of_record_order() {
        let snapshot =
            |id: &str, event: &str, at: i64, content: &str| AuthoritativeConversationRecord {
                record_id: id.to_string(),
                role: "assistant".to_string(),
                content: content.to_string(),
                structured_payload: None,
                metadata: Some(json!({
                    "conversation_turn_id":"turn-1",
                    "ui_message_id":"callback-1",
                    "memory_projection_event":true,
                    "task_runner_async":{
                        "event":event,
                        "callback_at_unix_ms":at
                    }
                })),
                created_at: "2026-11-10T10:00:00Z".to_string(),
            };
        let running = snapshot("projection-event:z", "task.run.started", 1_000, "running");
        for completed_at in [1_000, 2_000] {
            let completed = snapshot("projection-event:a", "task.completed", completed_at, "done");
            for records in [
                vec![running.clone(), completed.clone()],
                vec![completed.clone(), running.clone()],
            ] {
                let messages = super::authoritative_messages("conversation-1", &records).unwrap();
                assert_eq!(messages.len(), 1);
                assert_eq!(messages[0].message_id, "callback-1");
                assert_eq!(messages[0].content, json!("done"));
                assert_eq!(
                    messages[0].metadata["task_runner_async"]["event"],
                    "task.completed"
                );
            }
        }

        let mut open = running;
        open.metadata = Some(json!({
            "conversation_turn_id":"turn-1", "ui_message_id":"survey-1",
            "ui_structured_content":true, "memory_projection_event":true
        }));
        open.structured_payload = Some(json!({"type":"requirement_survey_status","status":"open"}));
        let mut resolved = open.clone();
        resolved.record_id = "projection-event:b".to_string();
        resolved.created_at = "2026-11-10T10:00:01Z".to_string();
        resolved.structured_payload =
            Some(json!({"type":"requirement_survey_status","status":"resolved"}));
        for records in [vec![open.clone(), resolved.clone()], vec![resolved, open]] {
            let messages = super::authoritative_messages("conversation-1", &records).unwrap();
            assert_eq!(messages.len(), 1);
            assert_eq!(messages[0].content["status"], "resolved");
            assert_eq!(messages[0].created_at_unix_ms, 1_794_304_800_000);
        }
    }

    #[async_trait]
    impl LocalConversationTranscriptSource for TranscriptFixture {
        async fn list_conversation_records(
            &self,
            tenant_id: &str,
            thread_id: &str,
            conversation_id: &str,
            before_cursor: Option<&str>,
            limit: u32,
        ) -> Result<AuthoritativeConversationPage, String> {
            assert_eq!(tenant_id, "user-1");
            assert_eq!(thread_id, "client-agent:agent:jiguli");
            assert_eq!(conversation_id, "conversation-1");
            assert_eq!(before_cursor, None);
            assert_eq!(limit, 10);
            Ok(AuthoritativeConversationPage {
                records: vec![
                    AuthoritativeConversationRecord {
                        record_id: "remote-assistant".to_string(),
                        role: "assistant".to_string(),
                        content: "remote answer".to_string(),
                        structured_payload: None,
                        metadata: Some(json!({
                            "conversation_id": "conversation-1",
                            "conversation_turn_id": "turn-1"
                        })),
                        created_at: "2026-11-06T10:00:01Z".to_string(),
                    },
                    AuthoritativeConversationRecord {
                        record_id: "message-1".to_string(),
                        role: "user".to_string(),
                        content: "remote hello".to_string(),
                        structured_payload: None,
                        metadata: Some(json!({
                            "conversation_id": "conversation-1",
                            "conversation_turn_id": "turn-1",
                            "ui_message_metadata": {
                                "composer": "main_chat",
                                "client_marker": "authoritative"
                            },
                            "ui_attachments": [{
                                "attachment_id": "attachment-1",
                                "display_name": "brief.pdf",
                                "media_type": "application/pdf",
                                "byte_size": 128,
                                "sha256": "a".repeat(64),
                                "authorized_local_ref": "local-attachment:brief.pdf",
                                "metadata": {"origin": "memory"}
                            }]
                        })),
                        created_at: "2026-11-06T10:00:00Z".to_string(),
                    },
                ],
                next_before_cursor: Some("cursor-2".to_string()),
            })
        }
    }

    fn request(command_id: &str, command: HostCommand) -> HostRequestEnvelope {
        HostRequestEnvelope {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            command_id: command_id.to_string(),
            command,
        }
    }

    fn create_conversation() -> CreateConversationCommand {
        CreateConversationCommand {
            conversation_id: "conversation-1".to_string(),
            owner_user_id: "user-1".to_string(),
            title: "Local conversation".to_string(),
            resource: Some(LocalConversationResourceBinding {
                kind: LocalConversationResourceKind::Contact,
                resource_id: JIGULI_AGENT_ID.to_string(),
            }),
        }
    }

    fn start_turn(expected_conversation_version: u64) -> StartConversationTurnCommand {
        StartConversationTurnCommand {
            owner_user_id: "user-1".to_string(),
            conversation_id: "conversation-1".to_string(),
            expected_conversation_version,
            turn_id: "turn-1".to_string(),
            message_id: "message-1".to_string(),
            run_id: "run-1".to_string(),
            message: "hello".to_string(),
            message_metadata: json!({
                "source": "composer",
                "agent_id": "jiguli",
                "agent_name": "叽咕狸",
                "explicit_capability_selection": {
                    "kind": "plugin",
                    "plugin_key": "document-plugin",
                    "component_key": "document-mcp",
                    "display_name": "Document"
                }
            }),
            attachments: vec![LocalConversationAttachmentSpec {
                attachment_id: "attachment-1".to_string(),
                display_name: "brief.pdf".to_string(),
                media_type: "application/pdf".to_string(),
                byte_size: 42,
                sha256: "a".repeat(64),
                authorized_local_ref: "local-attachment:authority-1".to_string(),
                metadata: json!({"page_count": 1}),
            }],
            model_config_ref: "model-1".to_string(),
            model_config_revision: "revision-1".to_string(),
            capability_policy_revision: "policy-1".to_string(),
            max_iterations: 8,
        }
    }

    fn agent_context() -> serde_json::Value {
        json!({
            "source": "native_agent_direct",
            "agent_execution_context": {
                "schema_version": 1,
                "room_id": "room-1",
                "agent_id": "agent-1",
                "agent_name": "工程 Agent",
                "role_prompt": "负责工程实现\n保持证据可追踪。",
                "profession_key": "software_engineer",
                "profession_label": "软件工程师",
                "profession_skill_ref": "AS-profession-software_engineer-123456789abc",
                "profession_skill_sha256": "a".repeat(64),
                "profession_skill_markdown": "# 工作流\n先核实，再实现。",
                "default_plugin_ids": [],
                "default_skill_ids": ["local.project.list"],
            }
        })
    }

    fn team_agent_context() -> serde_json::Value {
        json!({
            "source": "native_agent_team",
            "agent_execution_context": {
                "schema_version": 2,
                "room_id": "team-1",
                "agent_id": "agent-1",
                "agent_name": "工程 Agent",
                "role_prompt": "负责工程实现。",
                "profession_key": "software_engineer",
                "profession_label": "软件工程师",
                "profession_skill_ref": "AS-profession-software_engineer-123456789abc",
                "profession_skill_sha256": "a".repeat(64),
                "profession_skill_markdown": "# 工作流\n先核实，再实现。",
                "default_plugin_ids": [],
                "default_skill_ids": ["local.project.list"],
                "project_id": "project-1",
                "team_name": "Alpha 团队",
                "team_goal": "交付 Alpha",
                "team_role": "前端工程师",
                "team_responsibility": "负责 Windows UI",
                "is_project_manager": false,
            }
        })
    }

    fn team_executor_context() -> serde_json::Value {
        let mut metadata = team_agent_context();
        metadata["agent_team_execution"] = json!({
            "schema_version": 1,
            "delivery_lane": "executor",
            "trigger_kind": "todo",
            "delivery_id": "delivery-1",
            "todo_id": "todo-1",
            "team_id": "team-1",
            "agent_id": "agent-1",
            "project_id": "project-1",
            "execution_plan": {
                "requires_execution": true,
                "builtin_capabilities": ["project_read", "terminal"],
                "plugins": [],
            }
        });
        metadata
    }

    fn project_resource() -> LocalConversationResourceBinding {
        LocalConversationResourceBinding {
            kind: LocalConversationResourceKind::Project,
            resource_id: "project-1".into(),
        }
    }

    #[test]
    fn agent_context_is_strictly_bound_to_an_unbound_direct_conversation() {
        let normalized =
            normalized_agent_execution_context(&agent_context(), "agent-direct-room-1", None, None)
                .unwrap()
                .unwrap();
        assert_eq!(normalized["agent_id"], "agent-1");
        assert_eq!(normalized["profession_key"], "software_engineer");
        assert!(
            normalized_agent_execution_context(&agent_context(), "conversation-1", None, None,)
                .is_err()
        );
        assert!(normalized_agent_execution_context(
            &agent_context(),
            "agent-direct-room-1",
            Some(&project_resource()),
            None,
        )
        .is_err());
    }

    #[test]
    fn ordinary_conversation_identity_comes_from_the_client_and_matches_its_resource() {
        let jiguli = normalized_conversation_agent_identity(
            &json!({"agent_id": "jiguli", "agent_name": "叽咕狸"}),
            Some(&project_resource()),
        )
        .expect("Jiguli project identity");
        assert_eq!(jiguli, ("jiguli".to_string(), Some("叽咕狸".to_string())));

        let plugin_resource = LocalConversationResourceBinding {
            kind: LocalConversationResourceKind::Contact,
            resource_id: "plugin-installer".into(),
        };
        let plugin = normalized_conversation_agent_identity(
            &json!({"agent_id": "plugin-installer", "agent_name": "插件安装助手"}),
            Some(&plugin_resource),
        )
        .expect("plugin Agent identity");
        assert_eq!(plugin.0, "plugin-installer");
        assert!(normalized_conversation_agent_identity(
            &json!({"agent_id": "jiguli"}),
            Some(&plugin_resource),
        )
        .is_err());
        assert!(normalized_conversation_agent_identity(
            &json!({"agent_id": "another-agent"}),
            Some(&project_resource()),
        )
        .is_err());
        assert!(
            normalized_conversation_agent_identity(&json!({"agent_id": "jiguli"}), None,).is_err()
        );
    }

    #[test]
    fn agent_context_rejects_unknown_fields_and_bad_skill_hashes() {
        let mut unknown = agent_context();
        unknown["agent_execution_context"]["secret_path"] = json!("/tmp/private");
        assert!(
            normalized_agent_execution_context(&unknown, "agent-direct-room-1", None, None,)
                .is_err()
        );

        let mut bad_hash = agent_context();
        bad_hash["agent_execution_context"]["profession_skill_sha256"] = json!("ABC");
        assert!(
            normalized_agent_execution_context(&bad_hash, "agent-direct-room-1", None, None,)
                .is_err()
        );
    }

    #[test]
    fn team_agent_context_is_strictly_bound_to_its_agent_lane() {
        let normalized = normalized_agent_execution_context(
            &team_agent_context(),
            "agent-team-team-1-agent-1",
            None,
            None,
        )
        .unwrap()
        .unwrap();
        assert_eq!(normalized["schema_version"], 2);
        assert_eq!(normalized["team_role"], "前端工程师");
        assert_eq!(normalized["is_project_manager"], false);
        assert!(normalized_agent_execution_context(
            &team_agent_context(),
            "agent-team-team-1-agent-2",
            None,
            None,
        )
        .is_err());
        let mut wrong_source = team_agent_context();
        wrong_source["source"] = json!("native_agent_direct");
        assert!(normalized_agent_execution_context(
            &wrong_source,
            "agent-team-team-1-agent-1",
            None,
            None,
        )
        .is_err());
    }

    #[test]
    fn executor_context_is_project_bound_and_capability_scoped() {
        let metadata = team_executor_context();
        let executor = normalized_team_executor_context(
            &metadata,
            "agent-team-executor-delivery-1",
            Some(&project_resource()),
        )
        .unwrap()
        .unwrap();
        let normalized = normalized_agent_execution_context(
            &metadata,
            "agent-team-executor-delivery-1",
            Some(&project_resource()),
            Some(&executor),
        )
        .unwrap()
        .unwrap();
        assert_eq!(normalized["project_id"], "project-1");
        assert_eq!(
            executor["tool_options"]["enabled_builtin_kinds"],
            json!(["CodeMaintainerRead", "TerminalController"])
        );
        assert!(normalized_team_executor_context(
            &metadata,
            "agent-team-executor-delivery-1",
            None,
        )
        .is_err());
        let mut escalated = metadata;
        escalated["agent_team_execution"]["execution_plan"]["builtin_capabilities"] =
            json!(["project_read", "admin"]);
        assert!(normalized_team_executor_context(
            &escalated,
            "agent-team-executor-delivery-1",
            Some(&project_resource()),
        )
        .is_err());
    }

    #[test]
    fn executor_context_projects_only_frozen_plugin_selections() {
        let mut metadata = team_executor_context();
        metadata["agent_team_execution"]["execution_plan"]["plugins"] = json!([{
            "installationId": "installation-1",
            "pluginId": "document-plugin",
            "componentKey": "document-mcp",
            "componentRevision": "revision-1",
            "releaseId": "release-1",
            "releaseDigest": format!("sha256:{}", "a".repeat(64)),
            "manifestVersion": "1.2.3",
            "displayName": "Document Tools",
            "reason": "生成验收文档",
        }]);

        let executor = normalized_team_executor_context(
            &metadata,
            "agent-team-executor-delivery-1",
            Some(&project_resource()),
        )
        .unwrap()
        .unwrap();

        assert_eq!(
            executor["tool_options"]["plugin_hints"],
            json!([{
                "plugin_key": "document-plugin",
                "component_key": "document-mcp",
                "reason": "生成验收文档",
            }])
        );
        assert_eq!(
            executor["tool_options"]["plugin_release_snapshots"][0]["release_id"],
            "release-1"
        );
        assert_eq!(
            executor["execution_plan"]["plugins"][0]["installation_id"],
            "installation-1"
        );

        let mut forged = metadata;
        forged["agent_team_execution"]["execution_plan"]["plugins"][0]["path"] =
            json!("C:\\private\\plugin.exe");
        assert!(normalized_team_executor_context(
            &forged,
            "agent-team-executor-delivery-1",
            Some(&project_resource()),
        )
        .is_err());
    }

    #[tokio::test]
    async fn only_a_bound_team_executor_turn_uses_task_execution() {
        let storage = Arc::new(
            SqliteClientStorage::connect_memory()
                .await
                .expect("storage"),
        );
        let runtime = LocalAgentRuntime::with_clock(storage, Arc::new(|| Ok(10_000)));
        runtime.initialize("user-1").await.expect("initialize");
        let conversation_id = "agent-team-executor-delivery-1";
        runtime
            .try_handle(request(
                "create-executor-conversation",
                HostCommand::CreateConversation(CreateConversationCommand {
                    conversation_id: conversation_id.into(),
                    owner_user_id: "user-1".into(),
                    title: "Executor".into(),
                    resource: Some(project_resource()),
                }),
            ))
            .await
            .expect("create executor conversation");
        let mut turn = start_turn(1);
        turn.conversation_id = conversation_id.into();
        turn.message_metadata = team_executor_context();
        let started = runtime
            .try_handle(request(
                "start-executor-turn",
                HostCommand::StartConversationTurn(turn),
            ))
            .await
            .expect("start executor Turn");
        let HostResult::ConversationTurnStarted { result } = started else {
            panic!("unexpected start result")
        };
        assert_eq!(result.run.profile_key, "task_execution");
        assert_eq!(result.run.input["project_id"], "project-1");
        assert_eq!(result.run.input["prompt"], "hello");
        assert_eq!(
            result.run.input["tool_options"]["enabled_builtin_kinds"],
            json!(["CodeMaintainerRead", "TerminalController"])
        );
        assert_eq!(
            result.run.input["team_executor_context"]["todo_id"],
            "todo-1"
        );
        assert_eq!(result.run.input["todo_id"], "todo-1");
        assert_eq!(
            result.run.input["memory_thread_id"],
            "client-agent:todo:todo-1"
        );
    }

    #[tokio::test]
    async fn project_conversation_carries_its_display_name_into_memory_input() {
        let storage = Arc::new(
            SqliteClientStorage::connect_memory()
                .await
                .expect("storage"),
        );
        let runtime = LocalAgentRuntime::with_clock(storage, Arc::new(|| Ok(10_000)));
        runtime.initialize("user-1").await.expect("initialize");
        runtime
            .try_handle(request(
                "create-project-conversation",
                HostCommand::CreateConversation(CreateConversationCommand {
                    conversation_id: "project-conversation-1".into(),
                    owner_user_id: "user-1".into(),
                    title: "红果逆向".into(),
                    resource: Some(project_resource()),
                }),
            ))
            .await
            .expect("create project conversation");
        let mut turn = start_turn(1);
        turn.conversation_id = "project-conversation-1".into();
        turn.message_metadata = json!({
            "source": "native_main_chat",
            "agent_id": "jiguli",
            "agent_name": "叽咕狸",
            "project_name": "红果逆向",
        });

        let started = runtime
            .try_handle(request(
                "start-project-turn",
                HostCommand::StartConversationTurn(turn),
            ))
            .await
            .expect("start project Turn");
        let HostResult::ConversationTurnStarted { result } = started else {
            panic!("unexpected start result")
        };
        assert_eq!(result.run.input["project_id"], "project-1");
        assert_eq!(result.run.input["project_name"], "红果逆向");
    }

    #[tokio::test]
    async fn host_routes_local_conversation_and_atomic_turn_start() {
        let storage = Arc::new(
            SqliteClientStorage::connect_memory()
                .await
                .expect("storage"),
        );
        let runtime = LocalAgentRuntime::with_clock(storage, Arc::new(|| Ok(10_000)));
        runtime.initialize("user-1").await.expect("initialize");

        let created = runtime
            .try_handle(request(
                "create-conversation-1",
                HostCommand::CreateConversation(create_conversation()),
            ))
            .await
            .expect("create conversation");
        let HostResult::Conversation { conversation } = created else {
            panic!("unexpected create result")
        };
        assert_eq!(conversation.conversation.version, 1);

        let started = runtime
            .try_handle(request(
                "start-turn-1",
                HostCommand::StartConversationTurn(start_turn(1)),
            ))
            .await
            .expect("start Turn");
        let HostResult::ConversationTurnStarted { result } = started else {
            panic!("unexpected start result")
        };
        assert_eq!(result.conversation.version, 2);
        assert_eq!(result.turn.status, LocalConversationTurnStatus::Running);
        assert_eq!(result.message.role, LocalConversationMessageRole::User);
        assert_eq!(result.attachments.len(), 1);
        assert_eq!(result.attachments[0].attachment_id, "attachment-1");
        assert_eq!(result.run.owner_entity_type, "conversation_turn");
        assert_eq!(result.run.owner_entity_id, "turn-1");
        assert_eq!(result.run.profile_key, "main_chat");
        assert_eq!(result.run.input["agent_id"], "jiguli");
        assert_eq!(result.run.input["agent_name"], "叽咕狸");
        assert_eq!(result.run.input["conversation_id"], "conversation-1");
        assert_eq!(result.run.input["turn_id"], "turn-1");
        assert_eq!(result.run.input["message_id"], "message-1");
        assert_eq!(
            result.run.input["explicit_capability_selection"],
            json!({
                "kind": "plugin",
                "plugin_key": "document-plugin",
                "component_key": "document-mcp",
                "display_name": "Document"
            })
        );
        assert_eq!(
            result.run.input["attachments"][0]["attachment_id"],
            "attachment-1"
        );

        let loaded = runtime
            .try_handle(request(
                "get-conversation-1",
                HostCommand::GetConversation(GetConversationCommand {
                    owner_user_id: "user-1".to_string(),
                    conversation_id: "conversation-1".to_string(),
                }),
            ))
            .await
            .expect("get conversation");
        assert!(matches!(
            loaded,
            HostResult::Conversation { conversation }
                if conversation.turns.len() == 1
                    && conversation.messages.len() == 1
                    && conversation.attachments.len() == 1
        ));

        let listed = runtime
            .try_handle(request(
                "list-conversation-1",
                HostCommand::ListConversations(ListConversationsCommand {
                    owner_user_id: "user-1".to_string(),
                    before_updated_at_unix_ms: None,
                    before_conversation_id: None,
                    limit: 10,
                }),
            ))
            .await
            .expect("list conversations");
        assert!(matches!(
            listed,
            HostResult::Conversations { page } if page.conversations.len() == 1
        ));

        let history_error = runtime
            .try_handle(request(
                "get-conversation-history-1",
                HostCommand::GetConversationHistory(GetConversationHistoryCommand {
                    owner_user_id: "user-1".to_string(),
                    conversation_id: "conversation-1".to_string(),
                    before_cursor: None,
                    limit: 10,
                }),
            ))
            .await
            .expect_err("conversation history must not fall back to local SQLite");
        assert!(history_error
            .to_string()
            .contains("Memory transcript source is required"));
    }

    #[tokio::test]
    async fn configured_history_uses_memory_transcript_instead_of_local_messages() {
        let storage = Arc::new(
            SqliteClientStorage::connect_memory()
                .await
                .expect("storage"),
        );
        let runtime = LocalAgentRuntime::with_clock(storage, Arc::new(|| Ok(10_000)))
            .with_conversation_transcript_source(Arc::new(TranscriptFixture));
        runtime.initialize("user-1").await.expect("initialize");
        runtime
            .try_handle(request(
                "create-memory-authoritative-conversation",
                HostCommand::CreateConversation(create_conversation()),
            ))
            .await
            .expect("create conversation");
        let history = runtime
            .try_handle(request(
                "read-memory-authoritative-history",
                HostCommand::GetConversationHistory(GetConversationHistoryCommand {
                    owner_user_id: "user-1".to_string(),
                    conversation_id: "conversation-1".to_string(),
                    before_cursor: None,
                    limit: 10,
                }),
            ))
            .await
            .expect("read Memory transcript");
        let HostResult::ConversationHistory { page } = history else {
            panic!("unexpected result")
        };
        assert_eq!(page.messages.len(), 2);
        assert_eq!(page.messages[0].content, json!({"text": "remote hello"}));
        assert_eq!(page.messages[0].metadata["composer"], "main_chat");
        assert_eq!(page.messages[0].metadata["client_marker"], "authoritative");
        assert_eq!(page.messages[1].content, json!("remote answer"));
        assert_eq!(page.turns.len(), 1);
        assert_eq!(page.turns[0].turn_id, "turn-1");
        assert_eq!(page.turns[0].user_message_id, "message-1");
        assert_eq!(page.next_before_cursor.as_deref(), Some("cursor-2"));
        assert_eq!(page.attachments.len(), 1);
        assert_eq!(page.attachments[0].metadata, json!({"origin": "memory"}));
    }

    #[tokio::test]
    async fn host_routes_conversation_resume_and_stop_with_version_checks() {
        let storage = Arc::new(
            SqliteClientStorage::connect_memory()
                .await
                .expect("storage"),
        );
        let runtime = LocalAgentRuntime::with_clock(storage, Arc::new(|| Ok(10_000)));
        runtime.initialize("user-1").await.expect("initialize");
        runtime
            .try_handle(request(
                "create-conversation-control",
                HostCommand::CreateConversation(create_conversation()),
            ))
            .await
            .expect("create conversation");
        runtime
            .try_handle(request(
                "start-turn-control",
                HostCommand::StartConversationTurn(start_turn(1)),
            ))
            .await
            .expect("start Turn");
        let claim = runtime
            .try_handle(request(
                "claim-turn-control",
                HostCommand::ClaimNextRun(ClaimNextRunCommand {
                    owner_user_id: "user-1".to_string(),
                    worker_id: "model-worker".to_string(),
                    lease_duration_ms: 10_000,
                }),
            ))
            .await
            .expect("claim Run");
        let HostResult::Claim { claim: Some(claim) } = claim else {
            panic!("unexpected claim result")
        };
        let waiting = runtime
            .try_handle(request(
                "wait-turn-control",
                HostCommand::CommitStep(CommitStepCommand {
                    owner_user_id: "user-1".to_string(),
                    run_id: claim.run.run_id,
                    claim_token: claim.claim_token,
                    expected_version: claim.run.version,
                    outcome: LocalAgentStepOutcome::WaitForUser {
                        prompt: json!({"question": "continue?"}),
                        checkpoint: json!({"response_id": "response-1"}),
                    },
                }),
            ))
            .await
            .expect("wait for user");
        let HostResult::Run { run: waiting } = waiting else {
            panic!("unexpected wait result")
        };

        let resumed = runtime
            .try_handle(request(
                "resume-turn-control",
                HostCommand::ResumeConversationTurn(ResumeConversationTurnCommand {
                    owner_user_id: "user-1".to_string(),
                    conversation_id: "conversation-1".to_string(),
                    expected_conversation_version: 2,
                    turn_id: "turn-1".to_string(),
                    expected_run_version: waiting.version,
                    expected_run_status: LocalAgentRunStatus::WaitingUser,
                    message_id: "message-2".to_string(),
                    message: "continue".to_string(),
                    message_metadata: json!({"source": "ask_user"}),
                    attachments: Vec::new(),
                    reason: "user replied".to_string(),
                }),
            ))
            .await
            .expect("resume Turn");
        let HostResult::ConversationTurnUpdated { result: resumed } = resumed else {
            panic!("unexpected resume result")
        };
        assert_eq!(resumed.conversation.version, 3);
        assert_eq!(resumed.run.status, LocalAgentRunStatus::ContinuationReady);
        assert_eq!(
            resumed
                .run
                .continuation_input
                .as_ref()
                .expect("continuation")["input"]["message"],
            "continue"
        );

        let cancelled = runtime
            .try_handle(request(
                "cancel-turn-control",
                HostCommand::CancelConversationTurn(CancelConversationTurnCommand {
                    owner_user_id: "user-1".to_string(),
                    conversation_id: "conversation-1".to_string(),
                    expected_conversation_version: resumed.conversation.version,
                    turn_id: "turn-1".to_string(),
                    expected_run_version: Some(resumed.run.version),
                    reason: "user stopped".to_string(),
                }),
            ))
            .await
            .expect("cancel Turn");
        let HostResult::ConversationTurnUpdated { result: cancelled } = cancelled else {
            panic!("unexpected cancel result")
        };
        assert_eq!(cancelled.conversation.version, 4);
        assert_eq!(cancelled.run.status, LocalAgentRunStatus::Cancelled);
        assert_eq!(
            cancelled.turn.status,
            LocalConversationTurnStatus::Cancelled
        );
        assert!(cancelled.message.is_none());
    }

    #[tokio::test]
    async fn host_routes_guidance_into_the_next_local_model_claim() {
        let storage = Arc::new(
            SqliteClientStorage::connect_memory()
                .await
                .expect("storage"),
        );
        storage
            .configure_memory_projection("user-1", "chatos", 9_000)
            .await
            .expect("configure Memory projection");
        let runtime = LocalAgentRuntime::with_clock(storage.clone(), Arc::new(|| Ok(10_000)));
        runtime.initialize("user-1").await.expect("initialize");
        runtime
            .try_handle(request(
                "create-conversation-guidance",
                HostCommand::CreateConversation(create_conversation()),
            ))
            .await
            .expect("create conversation");
        runtime
            .try_handle(request(
                "start-turn-guidance",
                HostCommand::StartConversationTurn(start_turn(1)),
            ))
            .await
            .expect("start Turn");
        let guided = runtime
            .try_handle(request(
                "guide-turn",
                HostCommand::GuideConversationTurn(GuideConversationTurnCommand {
                    owner_user_id: "user-1".to_string(),
                    conversation_id: "conversation-1".to_string(),
                    expected_conversation_version: 2,
                    turn_id: "turn-1".to_string(),
                    expected_run_version: Some(1),
                    message_id: "message-guidance".to_string(),
                    message: "include tests".to_string(),
                    message_metadata: json!({}),
                    attachments: vec![LocalConversationAttachmentSpec {
                        attachment_id: "attachment-guidance".to_string(),
                        display_name: "guidance.txt".to_string(),
                        media_type: "text/plain".to_string(),
                        byte_size: 14,
                        sha256: "b".repeat(64),
                        authorized_local_ref: "local-attachment:guidance.txt".to_string(),
                        metadata: json!({"purpose": "guidance"}),
                    }],
                }),
            ))
            .await
            .expect("guide Turn");
        assert!(matches!(
            guided,
            HostResult::ConversationTurnUpdated { result }
                if result.conversation.version == 3 && result.message.is_some()
        ));
        let claim = runtime
            .try_handle(request(
                "claim-guided-turn",
                HostCommand::ClaimNextRun(ClaimNextRunCommand {
                    owner_user_id: "user-1".to_string(),
                    worker_id: "model-worker".to_string(),
                    lease_duration_ms: 10_000,
                }),
            ))
            .await
            .expect("claim guided Run");
        assert!(matches!(
            claim,
            HostResult::Claim { claim: Some(claim) }
                if claim.run.continuation_input.as_ref()
                    .and_then(|input| input.get("guidance"))
                    .and_then(serde_json::Value::as_array)
                    .is_some_and(|guidance| guidance.len() == 1)
        ));
        let projected = storage
            .claim_next_memory_record("user-1", "memory-claim", 10_001, 20_000)
            .await
            .expect("claim projected guidance")
            .expect("guidance Memory record");
        assert_eq!(projected.record_id, "message-guidance");
        assert_eq!(projected.thread_id, "client-agent:agent:jiguli");
        assert_eq!(projected.payload["content"], "include tests");
        assert_eq!(
            projected.payload["metadata"]["ui_attachments"][0]["attachment_id"],
            "attachment-guidance"
        );
    }
}
