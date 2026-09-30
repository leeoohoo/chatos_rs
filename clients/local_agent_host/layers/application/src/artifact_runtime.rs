// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{LocalAgentRuntime, LocalAgentRuntimeError};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use chatos_local_agent_ports::{ClientStorageError, IdempotentCommand, LocalAgentArtifactWrite};
use chatos_local_agent_protocol::{
    CreateArtifactCommand, HostCommand, HostResult, LocalAgentArtifact,
    LOCAL_AGENT_ARTIFACT_MAX_BYTES,
};
use sha2::{Digest, Sha256};
use uuid::Uuid;

impl LocalAgentRuntime {
    pub(super) async fn handle_artifact_command(
        &self,
        idempotency: &IdempotentCommand,
        command: HostCommand,
    ) -> Result<HostResult, LocalAgentRuntimeError> {
        match command {
            HostCommand::CreateArtifact(command) => {
                let write = artifact_write(command, self.now()?)?;
                let artifact = self.store.create_artifact(idempotency, &write).await?;
                Ok(HostResult::Artifact { artifact })
            }
            HostCommand::ListArtifacts(command) => {
                let (before_updated_at_unix_ms, before_artifact_id) = command
                    .cursor
                    .as_deref()
                    .map(parse_cursor)
                    .transpose()?
                    .map(|(timestamp, artifact_id)| (Some(timestamp), Some(artifact_id)))
                    .unwrap_or((None, None));
                let page = self
                    .store
                    .list_artifacts(
                        &command.owner_user_id,
                        before_updated_at_unix_ms,
                        before_artifact_id.as_deref(),
                        command.limit,
                    )
                    .await?;
                Ok(HostResult::Artifacts { page })
            }
            HostCommand::GetArtifactData(command) => {
                let data = self
                    .store
                    .read_artifact_data(&command.owner_user_id, &command.artifact_id)
                    .await?
                    .ok_or_else(|| ClientStorageError::NotFound(command.artifact_id.clone()))?;
                Ok(HostResult::ArtifactData {
                    artifact_id: command.artifact_id,
                    data_base64: STANDARD.encode(data),
                })
            }
            HostCommand::DeleteArtifact(command) => {
                self.store
                    .delete_artifact(
                        idempotency,
                        &command.owner_user_id,
                        &command.artifact_id,
                        self.now()?,
                    )
                    .await?;
                Ok(HostResult::ArtifactDeleted {
                    artifact_id: command.artifact_id,
                })
            }
            _ => unreachable!("non-artifact command routed to artifact runtime"),
        }
    }
}

fn artifact_write(
    command: CreateArtifactCommand,
    now_unix_ms: i64,
) -> Result<LocalAgentArtifactWrite, LocalAgentRuntimeError> {
    let data = STANDARD.decode(&command.data_base64).map_err(|_| {
        LocalAgentRuntimeError::InvalidRequest("artifact data is not valid base64".to_string())
    })?;
    if data.is_empty() || data.len() > LOCAL_AGENT_ARTIFACT_MAX_BYTES {
        return Err(LocalAgentRuntimeError::InvalidRequest(format!(
            "artifact data must contain 1..={LOCAL_AGENT_ARTIFACT_MAX_BYTES} decoded bytes"
        )));
    }
    let mime_type = command.mime_type.trim().to_ascii_lowercase();
    if mime_type != "text/markdown" && !mime_type.starts_with("text/markdown;") {
        return Err(LocalAgentRuntimeError::InvalidRequest(
            "agent artifacts must use the text/markdown MIME type".to_string(),
        ));
    }
    std::str::from_utf8(&data).map_err(|_| {
        LocalAgentRuntimeError::InvalidRequest(
            "agent artifact Markdown must contain valid UTF-8".to_string(),
        )
    })?;
    let digest = format!("{:x}", Sha256::digest(&data));
    if digest != command.sha256.to_ascii_lowercase() {
        return Err(LocalAgentRuntimeError::InvalidRequest(
            "artifact SHA-256 does not match its content".to_string(),
        ));
    }
    let size = u64::try_from(data.len()).map_err(|_| {
        LocalAgentRuntimeError::InvalidRequest("artifact size exceeds supported range".to_string())
    })?;
    Ok(LocalAgentArtifactWrite {
        artifact: LocalAgentArtifact {
            artifact_id: Uuid::new_v4().to_string(),
            owner_user_id: command.owner_user_id,
            name: command.name.trim().to_string(),
            mime_type,
            size,
            sha256: digest,
            created_at_unix_ms: now_unix_ms,
            updated_at_unix_ms: now_unix_ms,
        },
        idempotency_key: command.idempotency_key,
        data,
    })
}

fn parse_cursor(value: &str) -> Result<(i64, String), LocalAgentRuntimeError> {
    let (timestamp, artifact_id) = value.split_once(':').ok_or_else(|| {
        LocalAgentRuntimeError::InvalidRequest("artifact cursor is invalid".to_string())
    })?;
    let timestamp = timestamp.parse::<i64>().map_err(|_| {
        LocalAgentRuntimeError::InvalidRequest("artifact cursor is invalid".to_string())
    })?;
    chatos_local_agent_protocol::validate_identifier("artifact cursor id", artifact_id)
        .map_err(LocalAgentRuntimeError::InvalidRequest)?;
    if timestamp < 0 {
        return Err(LocalAgentRuntimeError::InvalidRequest(
            "artifact cursor is invalid".to_string(),
        ));
    }
    Ok((timestamp, artifact_id.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatos_client_storage::SqliteClientStorage;
    use chatos_local_agent_protocol::{
        DeleteArtifactCommand, GetArtifactDataCommand, HostRequestEnvelope, ListArtifactsCommand,
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

    fn create_command(owner: &str, key: &str, content: &str) -> CreateArtifactCommand {
        let data = content.as_bytes();
        CreateArtifactCommand {
            owner_user_id: owner.to_string(),
            name: "report.md".to_string(),
            mime_type: "text/markdown".to_string(),
            data_base64: STANDARD.encode(data),
            sha256: format!("{:x}", Sha256::digest(data)),
            idempotency_key: key.to_string(),
        }
    }

    #[tokio::test]
    async fn artifacts_are_file_backed_owner_scoped_and_idempotent() {
        let storage = Arc::new(
            SqliteClientStorage::connect_memory()
                .await
                .expect("storage"),
        );
        let runtime = LocalAgentRuntime::with_clock(storage, Arc::new(|| Ok(10_000)));
        runtime.initialize("owner-1").await.expect("initialize");

        let created = runtime
            .try_handle(request(
                "create-1",
                HostCommand::CreateArtifact(create_command("owner-1", "attachment-1", "# hi")),
            ))
            .await
            .expect("create artifact");
        let HostResult::Artifact { artifact } = created else {
            panic!("expected artifact")
        };
        let replay = runtime
            .try_handle(request(
                "create-2",
                HostCommand::CreateArtifact(create_command("owner-1", "attachment-1", "# hi")),
            ))
            .await
            .expect("logical replay");
        assert_eq!(
            replay,
            HostResult::Artifact {
                artifact: artifact.clone()
            }
        );

        let hidden = runtime
            .try_handle(request(
                "hidden",
                HostCommand::GetArtifactData(GetArtifactDataCommand {
                    owner_user_id: "owner-2".to_string(),
                    artifact_id: artifact.artifact_id.clone(),
                }),
            ))
            .await;
        assert!(hidden.is_err());

        let data = runtime
            .try_handle(request(
                "read",
                HostCommand::GetArtifactData(GetArtifactDataCommand {
                    owner_user_id: "owner-1".to_string(),
                    artifact_id: artifact.artifact_id.clone(),
                }),
            ))
            .await
            .expect("read artifact");
        assert_eq!(
            data,
            HostResult::ArtifactData {
                artifact_id: artifact.artifact_id.clone(),
                data_base64: STANDARD.encode("# hi"),
            }
        );

        let page = runtime
            .try_handle(request(
                "list",
                HostCommand::ListArtifacts(ListArtifactsCommand {
                    owner_user_id: "owner-1".to_string(),
                    limit: 10,
                    cursor: None,
                }),
            ))
            .await
            .expect("list artifacts");
        let HostResult::Artifacts { page } = page else {
            panic!("expected artifact page")
        };
        assert_eq!(page.artifacts, vec![artifact.clone()]);

        runtime
            .try_handle(request(
                "delete",
                HostCommand::DeleteArtifact(DeleteArtifactCommand {
                    owner_user_id: "owner-1".to_string(),
                    artifact_id: artifact.artifact_id.clone(),
                }),
            ))
            .await
            .expect("delete artifact");
        let deleted = runtime
            .try_handle(request(
                "read-deleted",
                HostCommand::GetArtifactData(GetArtifactDataCommand {
                    owner_user_id: "owner-1".to_string(),
                    artifact_id: artifact.artifact_id,
                }),
            ))
            .await;
        assert!(deleted.is_err());
    }
}
