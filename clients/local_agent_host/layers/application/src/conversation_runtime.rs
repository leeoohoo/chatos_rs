// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{new_event_id, LocalAgentRuntime, LocalAgentRuntimeError};
use chatos_local_agent_ports::{ClientStorageError, IdempotentCommand};
use chatos_local_agent_protocol::{
    CreateConversationCommand, HostCommand, HostResult, LocalAgentRunRecord, LocalAgentRunStatus,
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
                    .get_conversation(&command.conversation_id)
                    .await?
                    .ok_or(ClientStorageError::NotFound(command.conversation_id))?,
            }),
            HostCommand::ListConversations(command) => Ok(HostResult::Conversations {
                conversations: self
                    .store
                    .list_conversations(&command.owner_user_id, command.limit)
                    .await?,
            }),
            HostCommand::StartConversationTurn(command) => {
                let result = self.start_conversation_turn(idempotency, command).await?;
                Ok(HostResult::ConversationTurnStarted {
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
            .get_conversation(&command.conversation_id)
            .await?
            .ok_or_else(|| ClientStorageError::NotFound(command.conversation_id.clone()))?;
        let now = self.now()?;
        let input = json!({
            "message": &command.message,
            "attachments": &command.attachments,
        });
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatos_client_storage::SqliteClientStorage;
    use chatos_local_agent_protocol::{
        GetConversationCommand, HostRequestEnvelope, ListConversationsCommand,
        LocalConversationAttachmentSpec, LocalConversationMessageRole, LocalConversationTurnStatus,
        LOCAL_AGENT_PROTOCOL_VERSION,
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
        }
    }

    fn start_turn(expected_conversation_version: u64) -> StartConversationTurnCommand {
        StartConversationTurnCommand {
            conversation_id: "conversation-1".to_string(),
            expected_conversation_version,
            turn_id: "turn-1".to_string(),
            message_id: "message-1".to_string(),
            run_id: "run-1".to_string(),
            message: "hello".to_string(),
            message_metadata: json!({"source": "composer"}),
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

    #[tokio::test]
    async fn host_routes_local_conversation_and_atomic_turn_start() {
        let storage = Arc::new(
            SqliteClientStorage::connect_memory()
                .await
                .expect("storage"),
        );
        let runtime = LocalAgentRuntime::with_clock(storage, Arc::new(|| Ok(10_000)));
        runtime.initialize().await.expect("initialize");

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
        assert_eq!(
            result.run.input["attachments"][0]["attachment_id"],
            "attachment-1"
        );

        let loaded = runtime
            .try_handle(request(
                "get-conversation-1",
                HostCommand::GetConversation(GetConversationCommand {
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
                    limit: 10,
                }),
            ))
            .await
            .expect("list conversations");
        assert!(matches!(
            listed,
            HostResult::Conversations { conversations } if conversations.len() == 1
        ));
    }
}
