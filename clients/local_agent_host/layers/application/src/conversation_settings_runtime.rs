// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{LocalAgentRuntime, LocalAgentRuntimeError};
use chatos_local_agent_ports::{ClientStorageError, IdempotentCommand};
use chatos_local_agent_protocol::{HostCommand, HostResult};

impl LocalAgentRuntime {
    pub(super) async fn handle_conversation_settings_command(
        &self,
        idempotency: &IdempotentCommand,
        command: HostCommand,
    ) -> Result<HostResult, LocalAgentRuntimeError> {
        match command {
            HostCommand::GetConversationRuntimeSettings(command) => {
                let settings = self
                    .store
                    .get_conversation_runtime_settings(
                        &command.owner_user_id,
                        &command.conversation_id,
                    )
                    .await?
                    .ok_or_else(|| ClientStorageError::NotFound(command.conversation_id))?;
                Ok(HostResult::ConversationRuntimeSettings { settings })
            }
            HostCommand::PutConversationRuntimeSettings(command) => {
                let snapshot = self
                    .store
                    .get_model_config_snapshot(
                        &command.owner_user_id,
                        &command.selected_model_config_ref,
                        &command.selected_model_config_revision,
                    )
                    .await?
                    .ok_or_else(|| {
                        ClientStorageError::NotFound(format!(
                            "{}@{}",
                            command.selected_model_config_ref,
                            command.selected_model_config_revision
                        ))
                    })?;
                validate_thinking_level(
                    &snapshot.provider,
                    command.selected_thinking_level.as_deref(),
                    command.reasoning_enabled,
                )?;
                let settings = self
                    .store
                    .put_conversation_runtime_settings(idempotency, &command, self.now()?)
                    .await?;
                Ok(HostResult::ConversationRuntimeSettings { settings })
            }
            _ => unreachable!("non-settings command routed to conversation settings runtime"),
        }
    }
}

fn validate_thinking_level(
    provider: &str,
    level: Option<&str>,
    reasoning_enabled: bool,
) -> Result<(), LocalAgentRuntimeError> {
    let normalized = level.map(str::trim).filter(|value| !value.is_empty());
    if reasoning_enabled && matches!(normalized, None | Some("none")) {
        return Err(LocalAgentRuntimeError::InvalidRequest(
            "reasoning_enabled requires a non-none selected_thinking_level".to_string(),
        ));
    }
    let Some(level) = normalized else {
        return Ok(());
    };
    let normalized_provider = provider.trim().to_ascii_lowercase();
    let provider = match normalized_provider.as_str() {
        "openai" | "gpt" => "gpt",
        "kimik2" | "kimi" | "moonshot" => "kimi",
        "openai-compatible" | "openai_compatible" | "compatible" => "openai_compatible",
        value => value,
    };
    let accepted = match provider {
        "gpt" => &["none", "minimal", "low", "medium", "high", "xhigh"][..],
        "deepseek" => &["none", "low", "medium", "high", "max"][..],
        "kimi" => &["none", "auto", "low", "medium", "high", "xhigh"][..],
        _ => &["none", "low", "medium", "high", "xhigh"][..],
    };
    if !accepted.contains(&level) {
        return Err(LocalAgentRuntimeError::InvalidRequest(format!(
            "selected_thinking_level is not supported by provider {provider}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatos_client_storage::SqliteClientStorage;
    use chatos_local_agent_protocol::{
        GetConversationRuntimeSettingsCommand, HostRequestEnvelope, LocalModelConfigSnapshot,
        PutConversationRuntimeSettingsCommand, PutModelConfigSnapshotCommand,
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

    fn model(owner_user_id: &str) -> LocalModelConfigSnapshot {
        LocalModelConfigSnapshot {
            owner_user_id: owner_user_id.to_string(),
            model_config_ref: "model-1".to_string(),
            model_config_revision: "revision-1".to_string(),
            credential_ref: "env:CHATOS_LOCAL_AGENT_MODEL_1".to_string(),
            base_url: "https://example.test/v1".to_string(),
            model: "test-model".to_string(),
            provider: "openai".to_string(),
            supports_responses: true,
            supports_images: None,
            instructions: None,
            temperature: None,
            max_output_tokens: None,
            thinking_level: Some("medium".to_string()),
            include_prompt_cache_retention: false,
            request_body_limit_bytes: None,
            max_transient_retries: None,
            output_format: None,
        }
    }

    fn settings(expected_version: Option<u64>) -> PutConversationRuntimeSettingsCommand {
        PutConversationRuntimeSettingsCommand {
            owner_user_id: "user-1".to_string(),
            conversation_id: "conversation-1".to_string(),
            selected_model_config_ref: "model-1".to_string(),
            selected_model_config_revision: "revision-1".to_string(),
            selected_thinking_level: Some("high".to_string()),
            remote_connection_id: Some("connection-1".to_string()),
            reasoning_enabled: true,
            expected_version,
        }
    }

    #[tokio::test]
    async fn runtime_settings_require_an_owned_model_revision_and_version_updates() {
        let storage = Arc::new(
            SqliteClientStorage::connect_memory()
                .await
                .expect("storage"),
        );
        let runtime = LocalAgentRuntime::with_clock(storage, Arc::new(|| Ok(10_000)));
        runtime.initialize("user-1").await.expect("initialize");

        let missing = runtime
            .try_handle(request(
                "put-settings-missing-model",
                HostCommand::PutConversationRuntimeSettings(settings(None)),
            ))
            .await;
        assert!(matches!(
            missing,
            Err(LocalAgentRuntimeError::Storage(
                ClientStorageError::NotFound(_)
            ))
        ));

        runtime
            .try_handle(request(
                "put-model",
                HostCommand::PutModelConfigSnapshot(PutModelConfigSnapshotCommand {
                    snapshot: model("user-1"),
                }),
            ))
            .await
            .expect("put model");
        let created = runtime
            .try_handle(request(
                "put-settings",
                HostCommand::PutConversationRuntimeSettings(settings(None)),
            ))
            .await
            .expect("put settings");
        assert!(matches!(
            created,
            HostResult::ConversationRuntimeSettings { settings }
                if settings.version == 1 && settings.reasoning_enabled
        ));

        let mut update = settings(Some(1));
        update.reasoning_enabled = false;
        update.selected_thinking_level = Some("none".to_string());
        let updated = runtime
            .try_handle(request(
                "update-settings",
                HostCommand::PutConversationRuntimeSettings(update),
            ))
            .await
            .expect("update settings");
        assert!(matches!(
            updated,
            HostResult::ConversationRuntimeSettings { settings }
                if settings.version == 2 && !settings.reasoning_enabled
        ));

        let loaded = runtime
            .try_handle(request(
                "get-settings",
                HostCommand::GetConversationRuntimeSettings(
                    GetConversationRuntimeSettingsCommand {
                        owner_user_id: "user-1".to_string(),
                        conversation_id: "conversation-1".to_string(),
                    },
                ),
            ))
            .await
            .expect("get settings");
        assert!(matches!(
            loaded,
            HostResult::ConversationRuntimeSettings { settings }
                if settings.version == 2 && settings.selected_thinking_level.as_deref() == Some("none")
        ));
    }
}
