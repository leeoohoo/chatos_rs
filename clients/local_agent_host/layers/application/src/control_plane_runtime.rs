// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{LocalAgentRuntime, LocalAgentRuntimeError};
use chatos_local_agent_ports::{ClientStorageError, IdempotentCommand};
use chatos_local_agent_protocol::{HostCommand, HostResult};

impl LocalAgentRuntime {
    pub(super) async fn handle_control_plane_command(
        &self,
        idempotency: &IdempotentCommand,
        command: HostCommand,
    ) -> Result<HostResult, LocalAgentRuntimeError> {
        match command {
            HostCommand::PutModelConfigSnapshot(command) => {
                let snapshot = self
                    .store
                    .put_model_config_snapshot(idempotency, &command.snapshot, self.now()?)
                    .await?;
                Ok(HostResult::ModelConfigSnapshot { snapshot })
            }
            HostCommand::GetModelConfigSnapshot(command) => {
                let snapshot = self
                    .store
                    .get_model_config_snapshot(
                        &command.model_config_ref,
                        &command.model_config_revision,
                    )
                    .await?
                    .ok_or_else(|| {
                        ClientStorageError::NotFound(format!(
                            "{}@{}",
                            command.model_config_ref, command.model_config_revision
                        ))
                    })?;
                Ok(HostResult::ModelConfigSnapshot { snapshot })
            }
            HostCommand::PutCapabilityPolicySnapshot(command) => {
                let snapshot = self
                    .store
                    .put_capability_snapshot(idempotency, &command.snapshot, self.now()?)
                    .await?;
                Ok(HostResult::CapabilityPolicySnapshot { snapshot })
            }
            HostCommand::GetCapabilityPolicySnapshot(command) => {
                let snapshot = self
                    .store
                    .get_capability_snapshot(
                        &command.profile_key,
                        &command.capability_policy_revision,
                    )
                    .await?
                    .ok_or_else(|| {
                        ClientStorageError::NotFound(format!(
                            "{}@{}",
                            command.profile_key, command.capability_policy_revision
                        ))
                    })?;
                Ok(HostResult::CapabilityPolicySnapshot { snapshot })
            }
            _ => unreachable!("non-control-plane command routed to control-plane runtime"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatos_client_storage::SqliteClientStorage;
    use chatos_local_agent_protocol::{
        GetCapabilityPolicySnapshotCommand, GetModelConfigSnapshotCommand, HostRequestEnvelope,
        LocalCapabilityPolicySnapshot, LocalModelConfigSnapshot,
        PutCapabilityPolicySnapshotCommand, PutModelConfigSnapshotCommand,
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

    fn snapshot(model: &str) -> LocalModelConfigSnapshot {
        LocalModelConfigSnapshot {
            model_config_ref: "default".to_string(),
            model_config_revision: "revision-1".to_string(),
            credential_ref: "keychain:model/default".to_string(),
            base_url: "https://api.example.test/v1".to_string(),
            model: model.to_string(),
            provider: "openai".to_string(),
            supports_responses: true,
            supports_images: Some(true),
            instructions: None,
            temperature: None,
            max_output_tokens: Some(4096),
            thinking_level: Some("high".to_string()),
            include_prompt_cache_retention: true,
            request_body_limit_bytes: Some(1_048_576),
            max_transient_retries: Some(5),
            output_format: None,
        }
    }

    #[tokio::test]
    async fn host_routes_idempotent_model_snapshot_publication_and_read() {
        let storage = Arc::new(
            SqliteClientStorage::connect_memory()
                .await
                .expect("storage"),
        );
        let runtime = LocalAgentRuntime::with_clock(storage, Arc::new(|| Ok(10_000)));
        runtime.initialize().await.expect("initialize");
        let put = HostCommand::PutModelConfigSnapshot(PutModelConfigSnapshotCommand {
            snapshot: snapshot("model-a"),
        });
        let first = runtime
            .try_handle(request("put-model-1", put.clone()))
            .await
            .expect("put");
        let replay = runtime
            .try_handle(request("put-model-1", put))
            .await
            .expect("replay");
        assert_eq!(first, replay);

        let read = runtime
            .try_handle(request(
                "get-model-1",
                HostCommand::GetModelConfigSnapshot(GetModelConfigSnapshotCommand {
                    model_config_ref: "default".to_string(),
                    model_config_revision: "revision-1".to_string(),
                }),
            ))
            .await
            .expect("get");
        assert!(matches!(
            read,
            HostResult::ModelConfigSnapshot { snapshot } if snapshot.model == "model-a"
        ));

        let mismatch = runtime
            .handle(request(
                "put-model-1",
                HostCommand::PutModelConfigSnapshot(PutModelConfigSnapshotCommand {
                    snapshot: snapshot("model-b"),
                }),
            ))
            .await;
        assert_eq!(mismatch.error.expect("mismatch").code, "command_mismatch");

        runtime
            .try_handle(request(
                "put-policy-1",
                HostCommand::PutCapabilityPolicySnapshot(PutCapabilityPolicySnapshotCommand {
                    snapshot: LocalCapabilityPolicySnapshot {
                        profile_key: "main_chat".to_string(),
                        capability_policy_revision: "policy-1".to_string(),
                        instructions: Some("local policy".to_string()),
                        prefixed_input_items: Vec::new(),
                        tools: vec![serde_json::json!({"name": "read_file"})],
                    },
                }),
            ))
            .await
            .expect("put policy");
        let policy = runtime
            .try_handle(request(
                "get-policy-1",
                HostCommand::GetCapabilityPolicySnapshot(GetCapabilityPolicySnapshotCommand {
                    profile_key: "main_chat".to_string(),
                    capability_policy_revision: "policy-1".to_string(),
                }),
            ))
            .await
            .expect("get policy");
        assert!(matches!(
            policy,
            HostResult::CapabilityPolicySnapshot { snapshot }
                if snapshot.instructions.as_deref() == Some("local policy")
        ));
    }
}
