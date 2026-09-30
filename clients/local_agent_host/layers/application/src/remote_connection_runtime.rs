// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{LocalAgentRuntime, LocalAgentRuntimeError};
use chatos_local_agent_ports::{ClientStorageError, IdempotentCommand};
use chatos_local_agent_protocol::{
    HostCommand, HostResult, LocalRemoteConnection, LocalRemoteConnectionSpec,
};
use uuid::Uuid;

impl LocalAgentRuntime {
    pub(super) async fn handle_remote_connection_command(
        &self,
        idempotency: &IdempotentCommand,
        command: HostCommand,
    ) -> Result<HostResult, LocalAgentRuntimeError> {
        match command {
            HostCommand::ListRemoteConnections(command) => {
                let connections = self
                    .store
                    .list_remote_connections(&command.owner_user_id)
                    .await?;
                Ok(HostResult::RemoteConnections { connections })
            }
            HostCommand::GetRemoteConnection(command) => {
                let connection = self
                    .store
                    .get_remote_connection(&command.owner_user_id, &command.connection_id)
                    .await?;
                Ok(HostResult::RemoteConnection { connection })
            }
            HostCommand::CreateRemoteConnection(command) => {
                let now = self.now()?;
                let connection = new_connection(
                    Uuid::new_v4().to_string(),
                    command.owner_user_id,
                    normalize_spec(command.spec),
                    now,
                );
                let connection = self
                    .store
                    .create_remote_connection(idempotency, &connection)
                    .await?;
                Ok(HostResult::RemoteConnection {
                    connection: Some(connection),
                })
            }
            HostCommand::UpdateRemoteConnection(command) => {
                let current = self
                    .store
                    .get_remote_connection(&command.owner_user_id, &command.connection_id)
                    .await?
                    .ok_or_else(|| ClientStorageError::NotFound(command.connection_id.clone()))?;
                let connection = updated_connection(
                    current,
                    normalize_spec(command.spec),
                    command.expected_version,
                    self.now()?,
                )?;
                let connection = self
                    .store
                    .update_remote_connection(idempotency, &connection, command.expected_version)
                    .await?;
                Ok(HostResult::RemoteConnection {
                    connection: Some(connection),
                })
            }
            HostCommand::DeleteRemoteConnection(command) => {
                self.store
                    .delete_remote_connection(
                        idempotency,
                        &command.owner_user_id,
                        &command.connection_id,
                        command.expected_version,
                        self.now()?,
                    )
                    .await?;
                Ok(HostResult::RemoteConnectionDeleted {
                    connection_id: command.connection_id,
                })
            }
            _ => unreachable!("non-remote-connection command routed to remote runtime"),
        }
    }
}

fn new_connection(
    connection_id: String,
    owner_user_id: String,
    spec: LocalRemoteConnectionSpec,
    now: i64,
) -> LocalRemoteConnection {
    LocalRemoteConnection {
        connection_id,
        owner_user_id,
        name: spec
            .name
            .clone()
            .unwrap_or_else(|| format!("{}@{}", spec.username, spec.host)),
        host: spec.host,
        port: spec.port,
        username: spec.username,
        authentication_type: spec.authentication_type,
        has_password: false,
        has_private_key_path: false,
        has_certificate_path: false,
        default_remote_path: spec.default_remote_path,
        host_key_policy: spec.host_key_policy,
        local_connector_device_id: spec.local_connector_device_id,
        local_connector_workspace_id: spec.local_connector_workspace_id,
        jump_enabled: spec.jump_enabled,
        jump_connection_id: spec.jump_connection_id,
        jump_host: spec.jump_host,
        jump_port: spec.jump_port,
        jump_username: spec.jump_username,
        has_jump_private_key_path: false,
        has_jump_certificate_path: false,
        has_jump_password: false,
        last_active_at_unix_ms: None,
        version: 1,
        created_at_unix_ms: now,
        updated_at_unix_ms: now,
    }
}

fn updated_connection(
    current: LocalRemoteConnection,
    spec: LocalRemoteConnectionSpec,
    expected_version: u64,
    now: i64,
) -> Result<LocalRemoteConnection, LocalAgentRuntimeError> {
    let next_version = expected_version.checked_add(1).ok_or_else(|| {
        LocalAgentRuntimeError::InvalidRequest("remote connection version overflow".to_string())
    })?;
    Ok(LocalRemoteConnection {
        connection_id: current.connection_id,
        owner_user_id: current.owner_user_id,
        name: spec
            .name
            .clone()
            .unwrap_or_else(|| format!("{}@{}", spec.username, spec.host)),
        host: spec.host,
        port: spec.port,
        username: spec.username,
        authentication_type: spec.authentication_type,
        has_password: false,
        has_private_key_path: false,
        has_certificate_path: false,
        default_remote_path: spec.default_remote_path,
        host_key_policy: spec.host_key_policy,
        local_connector_device_id: spec.local_connector_device_id,
        local_connector_workspace_id: spec.local_connector_workspace_id,
        jump_enabled: spec.jump_enabled,
        jump_connection_id: spec.jump_connection_id,
        jump_host: spec.jump_host,
        jump_port: spec.jump_port,
        jump_username: spec.jump_username,
        has_jump_private_key_path: false,
        has_jump_certificate_path: false,
        has_jump_password: false,
        last_active_at_unix_ms: current.last_active_at_unix_ms,
        version: next_version,
        created_at_unix_ms: current.created_at_unix_ms,
        updated_at_unix_ms: now,
    })
}

fn normalize_spec(mut spec: LocalRemoteConnectionSpec) -> LocalRemoteConnectionSpec {
    spec.name = clean(spec.name);
    spec.host = spec.host.trim().to_string();
    spec.username = spec.username.trim().to_string();
    spec.default_remote_path = clean(spec.default_remote_path);
    spec.local_connector_device_id = spec.local_connector_device_id.trim().to_string();
    spec.local_connector_workspace_id = spec.local_connector_workspace_id.trim().to_string();
    spec.jump_connection_id = clean(spec.jump_connection_id);
    spec.jump_host = clean(spec.jump_host);
    spec.jump_username = clean(spec.jump_username);
    spec
}

fn clean(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim().to_string();
        (!value.is_empty()).then_some(value)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatos_client_storage::SqliteClientStorage;
    use chatos_local_agent_protocol::{
        CreateRemoteConnectionCommand, DeleteRemoteConnectionCommand, GetRemoteConnectionCommand,
        HostRequestEnvelope, ListRemoteConnectionsCommand, LocalRemoteAuthenticationType,
        LocalRemoteHostKeyPolicy, UpdateRemoteConnectionCommand, LOCAL_AGENT_PROTOCOL_VERSION,
    };
    use std::sync::Arc;

    fn request(command_id: &str, command: HostCommand) -> HostRequestEnvelope {
        HostRequestEnvelope {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            command_id: command_id.to_string(),
            command,
        }
    }

    fn spec(name: &str) -> LocalRemoteConnectionSpec {
        LocalRemoteConnectionSpec {
            name: Some(name.to_string()),
            host: format!("{}.example.com", name.to_lowercase()),
            port: 22,
            username: "deploy".to_string(),
            authentication_type: LocalRemoteAuthenticationType::PrivateKey,
            default_remote_path: Some("/srv/app".to_string()),
            host_key_policy: LocalRemoteHostKeyPolicy::Strict,
            local_connector_device_id: "device-local".to_string(),
            local_connector_workspace_id: "workspace-local".to_string(),
            jump_enabled: false,
            jump_connection_id: None,
            jump_host: None,
            jump_port: None,
            jump_username: None,
        }
    }

    async fn create(
        runtime: &LocalAgentRuntime,
        command_id: &str,
        spec: LocalRemoteConnectionSpec,
    ) -> LocalRemoteConnection {
        let result = runtime
            .try_handle(request(
                command_id,
                HostCommand::CreateRemoteConnection(CreateRemoteConnectionCommand {
                    owner_user_id: "user-1".to_string(),
                    spec,
                }),
            ))
            .await
            .expect("create connection");
        let HostResult::RemoteConnection {
            connection: Some(connection),
        } = result
        else {
            panic!("expected remote connection")
        };
        connection
    }

    #[tokio::test]
    async fn remote_connections_are_owner_scoped_versioned_and_jump_safe() {
        let storage = Arc::new(
            SqliteClientStorage::connect_memory()
                .await
                .expect("storage"),
        );
        let runtime = LocalAgentRuntime::with_clock(storage, Arc::new(|| Ok(10_000)));
        runtime.initialize("user-1").await.expect("initialize");

        let jump = create(&runtime, "create-jump", spec("Jump")).await;
        let mut target_spec = spec("Target");
        target_spec.jump_enabled = true;
        target_spec.jump_connection_id = Some(jump.connection_id.clone());
        let target = create(&runtime, "create-target", target_spec.clone()).await;

        let hidden = runtime
            .try_handle(request(
                "get-other-owner",
                HostCommand::GetRemoteConnection(GetRemoteConnectionCommand {
                    owner_user_id: "user-2".to_string(),
                    connection_id: target.connection_id.clone(),
                }),
            ))
            .await
            .expect("owner-scoped get");
        assert_eq!(hidden, HostResult::RemoteConnection { connection: None });

        target_spec.name = Some("Renamed".to_string());
        let updated = runtime
            .try_handle(request(
                "update-target",
                HostCommand::UpdateRemoteConnection(UpdateRemoteConnectionCommand {
                    owner_user_id: "user-1".to_string(),
                    connection_id: target.connection_id.clone(),
                    expected_version: target.version,
                    spec: target_spec,
                }),
            ))
            .await
            .expect("update connection");
        let HostResult::RemoteConnection {
            connection: Some(updated),
        } = updated
        else {
            panic!("expected updated connection")
        };
        assert_eq!(updated.name, "Renamed");
        assert_eq!(updated.version, 2);

        let stale = runtime
            .try_handle(request(
                "stale-update",
                HostCommand::UpdateRemoteConnection(UpdateRemoteConnectionCommand {
                    owner_user_id: "user-1".to_string(),
                    connection_id: updated.connection_id.clone(),
                    expected_version: 1,
                    spec: spec("Stale"),
                }),
            ))
            .await;
        assert!(matches!(
            stale,
            Err(LocalAgentRuntimeError::Storage(
                ClientStorageError::Conflict(_)
            ))
        ));

        runtime
            .try_handle(request(
                "delete-jump",
                HostCommand::DeleteRemoteConnection(DeleteRemoteConnectionCommand {
                    owner_user_id: "user-1".to_string(),
                    connection_id: jump.connection_id,
                    expected_version: jump.version,
                }),
            ))
            .await
            .expect("delete jump");
        let result = runtime
            .try_handle(request(
                "list-connections",
                HostCommand::ListRemoteConnections(ListRemoteConnectionsCommand {
                    owner_user_id: "user-1".to_string(),
                }),
            ))
            .await
            .expect("list connections");
        let HostResult::RemoteConnections { connections } = result else {
            panic!("expected connection list")
        };
        assert_eq!(connections.len(), 1);
        assert_eq!(connections[0].connection_id, target.connection_id);
        assert_eq!(connections[0].jump_connection_id, None);
        assert_eq!(connections[0].version, 3);
    }
}
