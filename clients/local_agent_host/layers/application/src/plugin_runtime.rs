// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{LocalAgentRuntime, LocalAgentRuntimeError};
use chatos_local_agent_ports::{ClientStorageError, IdempotentCommand};
use chatos_local_agent_protocol::{HostCommand, HostResult};

impl LocalAgentRuntime {
    pub(super) async fn handle_plugin_command(
        &self,
        idempotency: &IdempotentCommand,
        command: HostCommand,
    ) -> Result<HostResult, LocalAgentRuntimeError> {
        match command {
            HostCommand::PutPluginInstallation(command) => {
                let installation = self
                    .store
                    .put_plugin_installation(
                        idempotency,
                        &command.installation,
                        command.expected_version,
                        self.now()?,
                    )
                    .await?;
                Ok(HostResult::PluginInstallation { installation })
            }
            HostCommand::GetPluginInstallation(command) => {
                let installation = self
                    .store
                    .get_plugin_installation(&command.owner_user_id, &command.installation_id)
                    .await?
                    .ok_or(ClientStorageError::NotFound(command.installation_id))?;
                Ok(HostResult::PluginInstallation { installation })
            }
            HostCommand::ListPluginInstallations(command) => {
                let page = self
                    .store
                    .list_plugin_installations(
                        &command.owner_user_id,
                        command.before_updated_at_unix_ms,
                        command.before_installation_id.as_deref(),
                        command.limit,
                    )
                    .await?;
                Ok(HostResult::PluginInstallations { page })
            }
            HostCommand::RemovePluginInstallation(command) => {
                let installation = self
                    .store
                    .remove_plugin_installation(
                        idempotency,
                        &command.owner_user_id,
                        &command.installation_id,
                        command.expected_version,
                        self.now()?,
                    )
                    .await?;
                Ok(HostResult::PluginInstallation { installation })
            }
            _ => unreachable!("non-plugin command routed to plugin runtime"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatos_client_storage::SqliteClientStorage;
    use chatos_local_agent_protocol::{
        GetPluginInstallationCommand, HostRequestEnvelope, ListPluginInstallationsCommand,
        LocalPluginInstallationSpec, PutPluginInstallationCommand, RemovePluginInstallationCommand,
        LOCAL_AGENT_PROTOCOL_VERSION,
    };
    use std::{collections::BTreeMap, sync::Arc};

    fn request(command_id: &str, command: HostCommand) -> HostRequestEnvelope {
        HostRequestEnvelope {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            command_id: command_id.to_string(),
            command,
        }
    }

    fn installation() -> LocalPluginInstallationSpec {
        LocalPluginInstallationSpec {
            installation_id: "install-1".to_string(),
            owner_user_id: "user-1".to_string(),
            plugin_id: "plugin-1".to_string(),
            release_id: "release-1".to_string(),
            release_digest: "sha256:abc".to_string(),
            component_id: "mcp-1".to_string(),
            component_revision: "revision-1".to_string(),
            server_id: "files".to_string(),
            executable_path: "/plugins/files/server".to_string(),
            args: vec!["--stdio".to_string()],
            working_directory: None,
            environment_secret_refs: BTreeMap::new(),
            tool_prefix: None,
            allowed_tools: Some(vec!["read_file".to_string()]),
            enabled: true,
        }
    }

    #[tokio::test]
    async fn host_routes_plugin_installation_lifecycle() {
        let storage = Arc::new(
            SqliteClientStorage::connect_memory()
                .await
                .expect("storage"),
        );
        let runtime = LocalAgentRuntime::with_clock(storage, Arc::new(|| Ok(10_000)));
        runtime.initialize().await.expect("initialize");
        let created = runtime
            .try_handle(request(
                "put-plugin-1",
                HostCommand::PutPluginInstallation(PutPluginInstallationCommand {
                    installation: installation(),
                    expected_version: None,
                }),
            ))
            .await
            .expect("put");
        let HostResult::PluginInstallation { installation } = created else {
            panic!("unexpected result")
        };
        assert_eq!(installation.version, 1);

        let listed = runtime
            .try_handle(request(
                "list-plugin-1",
                HostCommand::ListPluginInstallations(ListPluginInstallationsCommand {
                    owner_user_id: "user-1".to_string(),
                    before_updated_at_unix_ms: None,
                    before_installation_id: None,
                    limit: 10,
                }),
            ))
            .await
            .expect("list");
        assert!(matches!(
            listed,
            HostResult::PluginInstallations { page } if page.installations.len() == 1
        ));

        runtime
            .try_handle(request(
                "remove-plugin-1",
                HostCommand::RemovePluginInstallation(RemovePluginInstallationCommand {
                    owner_user_id: "user-1".to_string(),
                    installation_id: "install-1".to_string(),
                    expected_version: 1,
                }),
            ))
            .await
            .expect("remove");
        let missing = runtime
            .handle(request(
                "get-plugin-missing",
                HostCommand::GetPluginInstallation(GetPluginInstallationCommand {
                    owner_user_id: "user-1".to_string(),
                    installation_id: "install-1".to_string(),
                }),
            ))
            .await;
        assert_eq!(missing.error.expect("not found").code, "not_found");
    }
}
