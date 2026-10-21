// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{new_event_id, LocalAgentRuntime, LocalAgentRuntimeError};
use chatos_local_agent_ports::{ClientStorageError, IdempotentCommand};
use chatos_local_agent_protocol::{
    CancelConversationTurnCommand, CreateConversationCommand, HostCommand, HostResult,
    LocalAgentRunRecord, LocalAgentRunStatus, ResumeConversationTurnCommand,
    StartConversationTurnCommand,
};
use serde_json::json;

impl LocalAgentRuntime {
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
                page: Box::new(
                    self.store
                        .get_conversation_history(
                            &command.owner_user_id,
                            &command.conversation_id,
                            command.before_ordinal,
                            command.limit,
                        )
                        .await?,
                ),
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
        let agent_execution_context = normalized_agent_execution_context(
            &command.message_metadata,
            &command.conversation_id,
            conversation.conversation.resource.is_none(),
        )?;
        let mut input = json!({
            "conversation_id": &command.conversation_id,
            "turn_id": &command.turn_id,
            "message_id": &command.message_id,
            "message": &command.message,
            "attachments": &command.attachments,
            "runtime_settings": runtime_settings.as_ref().map(|settings| json!({
                "version": settings.version,
                "selected_thinking_level": settings.selected_thinking_level,
                "remote_connection_id": settings.remote_connection_id,
                "reasoning_enabled": settings.reasoning_enabled,
            })),
        });
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
        let run = LocalAgentRunRecord {
            run_id: command.run_id.clone(),
            owner_user_id: conversation.conversation.owner_user_id,
            owner_entity_type: "conversation_turn".to_string(),
            owner_entity_id: command.turn_id.clone(),
            profile_key: "main_chat".to_string(),
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

fn normalized_agent_execution_context(
    metadata: &serde_json::Value,
    conversation_id: &str,
    conversation_is_unbound: bool,
) -> Result<Option<serde_json::Value>, LocalAgentRuntimeError> {
    let Some(context) = metadata.get("agent_execution_context") else {
        return Ok(None);
    };
    if metadata.get("source").and_then(serde_json::Value::as_str) != Some("native_agent_direct") {
        return Err(invalid_agent_context("source must be native_agent_direct"));
    }
    if !conversation_is_unbound {
        return Err(invalid_agent_context(
            "Agent direct conversations must not use Contact or Project resource bindings",
        ));
    }
    let values = context
        .as_object()
        .ok_or_else(|| invalid_agent_context("context must be an object"))?;
    const KEYS: [&str; 12] = [
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
    if values.len() != KEYS.len() || values.keys().any(|key| !KEYS.contains(&key.as_str())) {
        return Err(invalid_agent_context(
            "context fields do not match schema version 1",
        ));
    }
    if values
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        != Some(1)
    {
        return Err(invalid_agent_context("schema_version must be 1"));
    }
    let room_id = agent_context_string(values, "room_id", 512, true)?;
    let expected_conversation_id = format!("agent-direct-{room_id}");
    if conversation_id != expected_conversation_id {
        return Err(invalid_agent_context(
            "room_id does not match the Host conversation identity",
        ));
    }
    let agent_id = agent_context_string(values, "agent_id", 512, true)?;
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
    Ok(Some(json!({
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
    })))
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
    use chatos_client_storage::SqliteClientStorage;
    use chatos_local_agent_protocol::{
        CancelConversationTurnCommand, ClaimNextRunCommand, CommitStepCommand,
        GetConversationCommand, GetConversationHistoryCommand, GuideConversationTurnCommand,
        HostRequestEnvelope, ListConversationsCommand, LocalAgentStepOutcome,
        LocalConversationAttachmentSpec, LocalConversationMessageRole, LocalConversationTurnStatus,
        ResumeConversationTurnCommand, LOCAL_AGENT_PROTOCOL_VERSION,
    };
    use std::sync::Arc;

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
            resource: None,
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

    #[test]
    fn agent_context_is_strictly_bound_to_an_unbound_direct_conversation() {
        let normalized =
            normalized_agent_execution_context(&agent_context(), "agent-direct-room-1", true)
                .unwrap()
                .unwrap();
        assert_eq!(normalized["agent_id"], "agent-1");
        assert_eq!(normalized["profession_key"], "software_engineer");
        assert!(
            normalized_agent_execution_context(&agent_context(), "conversation-1", true,).is_err()
        );
        assert!(
            normalized_agent_execution_context(&agent_context(), "agent-direct-room-1", false,)
                .is_err()
        );
    }

    #[test]
    fn agent_context_rejects_unknown_fields_and_bad_skill_hashes() {
        let mut unknown = agent_context();
        unknown["agent_execution_context"]["secret_path"] = json!("/tmp/private");
        assert!(
            normalized_agent_execution_context(&unknown, "agent-direct-room-1", true,).is_err()
        );

        let mut bad_hash = agent_context();
        bad_hash["agent_execution_context"]["profession_skill_sha256"] = json!("ABC");
        assert!(
            normalized_agent_execution_context(&bad_hash, "agent-direct-room-1", true,).is_err()
        );
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

        let history = runtime
            .try_handle(request(
                "get-conversation-history-1",
                HostCommand::GetConversationHistory(GetConversationHistoryCommand {
                    owner_user_id: "user-1".to_string(),
                    conversation_id: "conversation-1".to_string(),
                    before_ordinal: None,
                    limit: 10,
                }),
            ))
            .await
            .expect("get conversation history");
        assert!(matches!(
            history,
            HostResult::ConversationHistory { page }
                if page.messages.len() == 1 && page.attachments.len() == 1
        ));
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
        let runtime = LocalAgentRuntime::with_clock(storage, Arc::new(|| Ok(10_000)));
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
                    attachments: Vec::new(),
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
    }
}
