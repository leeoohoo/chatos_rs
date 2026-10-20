mod credentials;
mod models;

use crate::{authentication::AuthenticationManager, connector_state, host_lifecycle::HostManager};
use chatos_local_agent_protocol::{
    CreateRemoteConnectionCommand, DeleteRemoteConnectionCommand, GetRemoteConnectionCommand,
    HostCommand, HostResult, ListRemoteConnectionsCommand, LocalRemoteConnection,
    LocalRemoteConnectionSpec, UpdateRemoteConnectionCommand,
};
use credentials::{PlatformRemoteCredentialVault, RemoteCredentialVault};
pub use models::{PresentedRemoteConnection, RemoteConnectionInput};
use models::{RemoteCredentialBundle, RemoteCredentialFlags};
use std::{collections::BTreeSet, future::Future, path::PathBuf, pin::Pin, sync::Arc};
use tauri::State;
use tokio::sync::Mutex;

type MetadataFuture<'a> = Pin<Box<dyn Future<Output = Result<HostResult, String>> + Send + 'a>>;

trait RemoteConnectionMetadataClient: Send + Sync {
    fn execute(&self, command: HostCommand) -> MetadataFuture<'_>;
}

impl RemoteConnectionMetadataClient for HostManager {
    fn execute(&self, command: HostCommand) -> MetadataFuture<'_> {
        Box::pin(HostManager::execute(self, command))
    }
}

#[derive(Clone)]
struct RemoteConnectorRoute {
    device_id: String,
    workspace_id: String,
}

trait RemoteConnectorRouteProvider: Send + Sync {
    fn route(&self, owner_user_id: &str) -> Result<RemoteConnectorRoute, String>;
}

struct ConnectorStateRouteProvider {
    data_directory: PathBuf,
}

impl RemoteConnectorRouteProvider for ConnectorStateRouteProvider {
    fn route(&self, owner_user_id: &str) -> Result<RemoteConnectorRoute, String> {
        let state = connector_state::read_owner_state(&self.data_directory, owner_user_id)?;
        let device_id = state
            .get("deviceID")
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "本机连接器设备标识不可用。".to_owned())?
            .to_owned();
        let workspace_id = state
            .get("workspaces")
            .and_then(serde_json::Value::as_array)
            .and_then(|workspaces| workspaces.first())
            .and_then(serde_json::Value::as_object)
            .and_then(|workspace| workspace.get("id"))
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.is_empty())
            .unwrap_or("windows-local-machine")
            .to_owned();
        Ok(RemoteConnectorRoute {
            device_id,
            workspace_id,
        })
    }
}

pub struct RemoteConnectionManager {
    mutation: Mutex<()>,
    credentials: Arc<dyn RemoteCredentialVault>,
    route_provider: Arc<dyn RemoteConnectorRouteProvider>,
}

impl RemoteConnectionManager {
    pub fn new(app_data_directory: PathBuf) -> Self {
        Self {
            mutation: Mutex::new(()),
            credentials: Arc::new(PlatformRemoteCredentialVault::new(
                app_data_directory.clone(),
            )),
            route_provider: Arc::new(ConnectorStateRouteProvider {
                data_directory: app_data_directory,
            }),
        }
    }

    #[cfg(test)]
    fn with_dependencies(
        credentials: Arc<dyn RemoteCredentialVault>,
        route_provider: Arc<dyn RemoteConnectorRouteProvider>,
    ) -> Self {
        Self {
            mutation: Mutex::new(()),
            credentials,
            route_provider,
        }
    }

    async fn list(
        &self,
        client: &dyn RemoteConnectionMetadataClient,
        owner_user_id: &str,
    ) -> Result<Vec<PresentedRemoteConnection>, String> {
        validate_owner(owner_user_id)?;
        let result = client
            .execute(HostCommand::ListRemoteConnections(
                ListRemoteConnectionsCommand {
                    owner_user_id: owner_user_id.to_owned(),
                },
            ))
            .await?;
        let HostResult::RemoteConnections { connections } = result else {
            return Err(metadata_response_error());
        };
        for connection in &connections {
            validate_record(connection, owner_user_id)?;
        }
        let active_ids = connections
            .iter()
            .map(|connection| connection.connection_id.clone())
            .collect::<BTreeSet<_>>();
        self.credentials.reconcile(owner_user_id, &active_ids)?;
        connections
            .into_iter()
            .map(|record| self.present(owner_user_id, record))
            .collect()
    }

    async fn get(
        &self,
        client: &dyn RemoteConnectionMetadataClient,
        owner_user_id: &str,
        connection_id: &str,
    ) -> Result<Option<PresentedRemoteConnection>, String> {
        validate_identity(owner_user_id, connection_id)?;
        let connection = get_record(client, owner_user_id, connection_id).await?;
        let Some(connection) = connection else {
            self.credentials.delete(owner_user_id, connection_id)?;
            return Ok(None);
        };
        Ok(Some(self.present(owner_user_id, connection)?))
    }

    async fn create(
        &self,
        client: &dyn RemoteConnectionMetadataClient,
        owner_user_id: &str,
        input: RemoteConnectionInput,
    ) -> Result<PresentedRemoteConnection, String> {
        let _mutation = self.mutation.lock().await;
        validate_owner(owner_user_id)?;
        let (spec, retain_manual_jump_credentials) =
            self.spec(client, owner_user_id, None, &input).await?;
        let credentials = RemoteCredentialBundle::default().applying(
            &input.credentials,
            input.authentication_type,
            retain_manual_jump_credentials,
        )?;
        let result = client
            .execute(HostCommand::CreateRemoteConnection(
                CreateRemoteConnectionCommand {
                    owner_user_id: owner_user_id.to_owned(),
                    spec,
                },
            ))
            .await?;
        let HostResult::RemoteConnection {
            connection: Some(connection),
        } = result
        else {
            return Err(metadata_response_error());
        };
        validate_record(&connection, owner_user_id)?;
        if let Err(error) =
            self.credentials
                .save(owner_user_id, &connection.connection_id, &credentials)
        {
            let rollback = client
                .execute(HostCommand::DeleteRemoteConnection(
                    DeleteRemoteConnectionCommand {
                        owner_user_id: owner_user_id.to_owned(),
                        connection_id: connection.connection_id.clone(),
                        expected_version: connection.version,
                    },
                ))
                .await;
            let credential_cleanup = self
                .credentials
                .delete(owner_user_id, &connection.connection_id);
            return Err(rollback_failure_message(
                error,
                rollback,
                credential_cleanup,
            ));
        }
        Ok(PresentedRemoteConnection::from_record(
            connection,
            credentials.flags(),
        ))
    }

    async fn update(
        &self,
        client: &dyn RemoteConnectionMetadataClient,
        owner_user_id: &str,
        connection_id: &str,
        expected_version: u64,
        input: RemoteConnectionInput,
    ) -> Result<PresentedRemoteConnection, String> {
        let _mutation = self.mutation.lock().await;
        validate_identity(owner_user_id, connection_id)?;
        if expected_version == 0 {
            return Err("远端连接版本无效。".into());
        }
        let current = get_record(client, owner_user_id, connection_id)
            .await?
            .ok_or_else(|| "远端连接不存在。".to_owned())?;
        if current.version != expected_version {
            return Err("远端连接已被其他操作更新，请刷新后重试。".into());
        }
        let (spec, retain_manual_jump_credentials) = self
            .spec(client, owner_user_id, Some(connection_id), &input)
            .await?;
        let previous = self
            .credentials
            .load(owner_user_id, connection_id)?
            .unwrap_or_default();
        let next = previous.clone().applying(
            &input.credentials,
            input.authentication_type,
            retain_manual_jump_credentials,
        )?;
        self.credentials.save(owner_user_id, connection_id, &next)?;
        let result = client
            .execute(HostCommand::UpdateRemoteConnection(
                UpdateRemoteConnectionCommand {
                    owner_user_id: owner_user_id.to_owned(),
                    connection_id: connection_id.to_owned(),
                    expected_version,
                    spec,
                },
            ))
            .await;
        let result = match result {
            Ok(result) => result,
            Err(error) => {
                let rollback = restore_credentials(
                    self.credentials.as_ref(),
                    owner_user_id,
                    connection_id,
                    &previous,
                );
                return Err(credential_rollback_message(error, rollback));
            }
        };
        let HostResult::RemoteConnection {
            connection: Some(connection),
        } = result
        else {
            return Err(metadata_response_error());
        };
        validate_record(&connection, owner_user_id)?;
        if connection.connection_id != connection_id
            || connection.version != expected_version.saturating_add(1)
        {
            return Err(metadata_response_error());
        }
        Ok(PresentedRemoteConnection::from_record(
            connection,
            next.flags(),
        ))
    }

    async fn delete(
        &self,
        client: &dyn RemoteConnectionMetadataClient,
        owner_user_id: &str,
        connection_id: &str,
        expected_version: u64,
    ) -> Result<String, String> {
        let _mutation = self.mutation.lock().await;
        validate_identity(owner_user_id, connection_id)?;
        if expected_version == 0 {
            return Err("远端连接版本无效。".into());
        }
        let current = get_record(client, owner_user_id, connection_id)
            .await?
            .ok_or_else(|| "远端连接不存在。".to_owned())?;
        if current.version != expected_version {
            return Err("远端连接已被其他操作更新，请刷新后重试。".into());
        }
        let previous = self.credentials.load(owner_user_id, connection_id)?;
        self.credentials.delete(owner_user_id, connection_id)?;
        let result = client
            .execute(HostCommand::DeleteRemoteConnection(
                DeleteRemoteConnectionCommand {
                    owner_user_id: owner_user_id.to_owned(),
                    connection_id: connection_id.to_owned(),
                    expected_version,
                },
            ))
            .await;
        let result = match result {
            Ok(result) => result,
            Err(error) => {
                let rollback = match previous.as_ref() {
                    Some(previous) => self
                        .credentials
                        .save(owner_user_id, connection_id, previous),
                    None => Ok(()),
                };
                return Err(credential_rollback_message(error, rollback));
            }
        };
        let HostResult::RemoteConnectionDeleted {
            connection_id: deleted_id,
        } = result
        else {
            return Err(metadata_response_error());
        };
        if deleted_id != connection_id {
            return Err(metadata_response_error());
        }
        Ok(deleted_id)
    }

    async fn spec(
        &self,
        client: &dyn RemoteConnectionMetadataClient,
        owner_user_id: &str,
        current_connection_id: Option<&str>,
        input: &RemoteConnectionInput,
    ) -> Result<(LocalRemoteConnectionSpec, bool), String> {
        let route = self.route_provider.route(owner_user_id)?;
        let mut jump_connection_id = clean(input.jump_connection_id.clone());
        let mut jump_host = clean(input.jump_host.clone());
        let mut jump_port = input.jump_port;
        let mut jump_username = clean(input.jump_username.clone());
        let retain_manual_jump_credentials;
        if !input.jump_enabled {
            jump_connection_id = None;
            jump_host = None;
            jump_port = None;
            jump_username = None;
            retain_manual_jump_credentials = false;
        } else if let Some(jump_id) = jump_connection_id.as_deref() {
            if current_connection_id == Some(jump_id) {
                return Err("远端连接不能把自身设为跳板机。".into());
            }
            let jump = get_record(client, owner_user_id, jump_id)
                .await?
                .ok_or_else(|| "所选跳板机连接已不存在。".to_owned())?;
            jump_host = Some(jump.host);
            jump_port = Some(jump.port);
            jump_username = Some(jump.username);
            retain_manual_jump_credentials = false;
        } else {
            retain_manual_jump_credentials = true;
        }
        let spec = LocalRemoteConnectionSpec {
            name: clean(input.name.clone()),
            host: input.host.trim().to_owned(),
            port: input.port,
            username: input.username.trim().to_owned(),
            authentication_type: input.authentication_type,
            default_remote_path: clean(input.default_remote_path.clone()),
            host_key_policy: input.host_key_policy,
            local_connector_device_id: route.device_id,
            local_connector_workspace_id: route.workspace_id,
            jump_enabled: input.jump_enabled,
            jump_connection_id,
            jump_host,
            jump_port,
            jump_username,
        };
        spec.validate()
            .map_err(|_| "远端连接信息无效。".to_owned())?;
        Ok((spec, retain_manual_jump_credentials))
    }

    fn present(
        &self,
        owner_user_id: &str,
        record: LocalRemoteConnection,
    ) -> Result<PresentedRemoteConnection, String> {
        let flags = self
            .credentials
            .load(owner_user_id, &record.connection_id)?
            .map(|credentials| credentials.flags())
            .unwrap_or_else(RemoteCredentialFlags::default);
        Ok(PresentedRemoteConnection::from_record(record, flags))
    }
}

#[tauri::command]
pub async fn list_remote_connections(
    host: State<'_, HostManager>,
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, RemoteConnectionManager>,
    owner_user_id: String,
) -> Result<Vec<PresentedRemoteConnection>, String> {
    authentication
        .access_token_for_owner(&owner_user_id)
        .await?;
    manager.list(&*host, &owner_user_id).await
}

#[tauri::command]
pub async fn get_remote_connection(
    host: State<'_, HostManager>,
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, RemoteConnectionManager>,
    owner_user_id: String,
    connection_id: String,
) -> Result<Option<PresentedRemoteConnection>, String> {
    authentication
        .access_token_for_owner(&owner_user_id)
        .await?;
    manager.get(&*host, &owner_user_id, &connection_id).await
}

#[tauri::command]
pub async fn create_remote_connection(
    host: State<'_, HostManager>,
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, RemoteConnectionManager>,
    owner_user_id: String,
    input: RemoteConnectionInput,
) -> Result<PresentedRemoteConnection, String> {
    authentication
        .access_token_for_owner(&owner_user_id)
        .await?;
    manager.create(&*host, &owner_user_id, input).await
}

#[tauri::command]
pub async fn update_remote_connection(
    host: State<'_, HostManager>,
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, RemoteConnectionManager>,
    owner_user_id: String,
    connection_id: String,
    expected_version: u64,
    input: RemoteConnectionInput,
) -> Result<PresentedRemoteConnection, String> {
    authentication
        .access_token_for_owner(&owner_user_id)
        .await?;
    manager
        .update(
            &*host,
            &owner_user_id,
            &connection_id,
            expected_version,
            input,
        )
        .await
}

#[tauri::command]
pub async fn delete_remote_connection(
    host: State<'_, HostManager>,
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, RemoteConnectionManager>,
    owner_user_id: String,
    connection_id: String,
    expected_version: u64,
) -> Result<String, String> {
    authentication
        .access_token_for_owner(&owner_user_id)
        .await?;
    manager
        .delete(&*host, &owner_user_id, &connection_id, expected_version)
        .await
}

async fn get_record(
    client: &dyn RemoteConnectionMetadataClient,
    owner_user_id: &str,
    connection_id: &str,
) -> Result<Option<LocalRemoteConnection>, String> {
    validate_identity(owner_user_id, connection_id)?;
    let result = client
        .execute(HostCommand::GetRemoteConnection(
            GetRemoteConnectionCommand {
                owner_user_id: owner_user_id.to_owned(),
                connection_id: connection_id.to_owned(),
            },
        ))
        .await?;
    let HostResult::RemoteConnection { connection } = result else {
        return Err(metadata_response_error());
    };
    if let Some(record) = connection.as_ref() {
        validate_record(record, owner_user_id)?;
        if record.connection_id != connection_id {
            return Err(metadata_response_error());
        }
    }
    Ok(connection)
}

fn validate_owner(owner_user_id: &str) -> Result<(), String> {
    ListRemoteConnectionsCommand {
        owner_user_id: owner_user_id.to_owned(),
    }
    .validate()
    .map_err(|_| "当前账号标识无效。".to_owned())
}

fn validate_identity(owner_user_id: &str, connection_id: &str) -> Result<(), String> {
    GetRemoteConnectionCommand {
        owner_user_id: owner_user_id.to_owned(),
        connection_id: connection_id.to_owned(),
    }
    .validate()
    .map_err(|_| "远端连接标识无效。".to_owned())
}

fn validate_record(record: &LocalRemoteConnection, owner_user_id: &str) -> Result<(), String> {
    if record.owner_user_id != owner_user_id || record.version == 0 {
        return Err(metadata_response_error());
    }
    validate_identity(owner_user_id, &record.connection_id)
}

fn restore_credentials(
    vault: &dyn RemoteCredentialVault,
    owner_user_id: &str,
    connection_id: &str,
    previous: &RemoteCredentialBundle,
) -> Result<(), String> {
    if previous.is_empty() {
        vault.delete(owner_user_id, connection_id)
    } else {
        vault.save(owner_user_id, connection_id, previous)
    }
}

fn clean(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim().to_owned();
        (!value.is_empty()).then_some(value)
    })
}

fn metadata_response_error() -> String {
    "Local Agent Host 返回了无效的远端连接响应。".into()
}

fn credential_rollback_message(error: String, rollback: Result<(), String>) -> String {
    if rollback.is_ok() {
        error
    } else {
        "远端连接操作失败，且本机凭据回滚未完成；请刷新后重试。".into()
    }
}

fn rollback_failure_message(
    error: String,
    metadata_rollback: Result<HostResult, String>,
    credential_cleanup: Result<(), String>,
) -> String {
    let metadata_rollback_succeeded = matches!(
        metadata_rollback,
        Ok(HostResult::RemoteConnectionDeleted { .. })
    );
    if metadata_rollback_succeeded && credential_cleanup.is_ok() {
        error
    } else {
        "远端连接凭据保存失败，且创建回滚未完整完成；请刷新连接列表后重试。".into()
    }
}

#[cfg(test)]
mod tests;
