mod credential_files;
mod credentials;
mod files;
mod models;
mod ssh;
mod terminal;

use crate::{authentication::AuthenticationManager, connector_state, host_lifecycle::HostManager};
use chatos_local_agent_protocol::{
    CreateRemoteConnectionCommand, DeleteRemoteConnectionCommand, GetRemoteConnectionCommand,
    HostCommand, HostResult, ListRemoteConnectionsCommand, LocalRemoteConnection,
    LocalRemoteConnectionSpec, UpdateRemoteConnectionCommand,
};
use credential_files::RemoteCredentialFileSelectionStore;
pub use credential_files::{
    discard_remote_credential_file_selections, pick_remote_credential_file,
};
use credentials::{PlatformRemoteCredentialVault, RemoteCredentialVault};
#[cfg(test)]
use files::DisabledRemoteFileExecutor;
pub use files::{
    create_remote_directory, delete_remote_entry, discard_remote_file_selections,
    download_remote_file, list_remote_directory, pick_remote_download_destination,
    pick_remote_upload_file, rename_remote_entry, upload_remote_file,
};
use files::{RemoteFileExecutor, RemoteFileSelectionStore};
pub use models::{
    PresentedRemoteConnection, PresentedRemoteConnectionTestResult, RemoteConnectionInput,
};
use models::{RemoteCredentialBundle, RemoteCredentialFlags};
use ssh::{NativeOpenSshTester, RemoteConnectionTester, SshConnectionMaterial};
use std::{collections::BTreeSet, future::Future, path::PathBuf, pin::Pin, sync::Arc};
use tauri::State;
#[cfg(test)]
use terminal::DisabledRemoteTerminalLauncher;
pub use terminal::{
    close_remote_terminal, discard_remote_terminal_presentation, interrupt_remote_terminal,
    poll_remote_terminal, resize_remote_terminal, send_remote_terminal_input,
    start_remote_terminal,
};
use terminal::{RemoteTerminalLauncher, RemoteTerminalRegistry};
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
    tester: Arc<dyn RemoteConnectionTester>,
    credential_file_selections: RemoteCredentialFileSelectionStore,
    file_executor: Arc<dyn RemoteFileExecutor>,
    file_selections: RemoteFileSelectionStore,
    terminal_tester: Arc<dyn RemoteConnectionTester>,
    terminal_launcher: Arc<dyn RemoteTerminalLauncher>,
    terminal_sessions: RemoteTerminalRegistry,
}

impl RemoteConnectionManager {
    pub fn new(app_data_directory: PathBuf) -> Self {
        let runtime = Arc::new(NativeOpenSshTester::new(app_data_directory.clone()));
        let terminal_runtime = Arc::new(NativeOpenSshTester::new(app_data_directory.clone()));
        Self {
            mutation: Mutex::new(()),
            credentials: Arc::new(PlatformRemoteCredentialVault::new(
                app_data_directory.clone(),
            )),
            route_provider: Arc::new(ConnectorStateRouteProvider {
                data_directory: app_data_directory,
            }),
            tester: runtime.clone(),
            credential_file_selections: RemoteCredentialFileSelectionStore::default(),
            file_executor: runtime.clone(),
            file_selections: RemoteFileSelectionStore::default(),
            terminal_tester: terminal_runtime.clone(),
            terminal_launcher: terminal_runtime,
            terminal_sessions: RemoteTerminalRegistry::default(),
        }
    }

    #[cfg(test)]
    fn with_dependencies(
        credentials: Arc<dyn RemoteCredentialVault>,
        route_provider: Arc<dyn RemoteConnectorRouteProvider>,
        tester: Arc<dyn RemoteConnectionTester>,
    ) -> Self {
        Self {
            mutation: Mutex::new(()),
            credentials,
            route_provider,
            tester: tester.clone(),
            credential_file_selections: RemoteCredentialFileSelectionStore::default(),
            file_executor: Arc::new(DisabledRemoteFileExecutor),
            file_selections: RemoteFileSelectionStore::default(),
            terminal_tester: tester.clone(),
            terminal_launcher: Arc::new(DisabledRemoteTerminalLauncher),
            terminal_sessions: RemoteTerminalRegistry::default(),
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
        presentation_id: Option<&str>,
        input: RemoteConnectionInput,
    ) -> Result<PresentedRemoteConnection, String> {
        let _mutation = self.mutation.lock().await;
        self.reset_runtime().await;
        validate_owner(owner_user_id)?;
        let credential_mutation = self.credential_file_selections.resolve_mutation(
            owner_user_id,
            presentation_id,
            &input.credentials,
        )?;
        let prepared = self.spec(client, owner_user_id, None, &input).await?;
        let credentials = RemoteCredentialBundle::default().applying(
            &credential_mutation,
            input.authentication_type,
            prepared.retain_manual_jump_credentials,
        )?;
        let result = client
            .execute(HostCommand::CreateRemoteConnection(
                CreateRemoteConnectionCommand {
                    owner_user_id: owner_user_id.to_owned(),
                    spec: prepared.spec,
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
        let presented = PresentedRemoteConnection::from_record(connection, credentials.flags());
        if let Some(presentation_id) = presentation_id {
            self.credential_file_selections
                .discard(owner_user_id, presentation_id);
        }
        Ok(presented)
    }

    async fn update(
        &self,
        client: &dyn RemoteConnectionMetadataClient,
        owner_user_id: &str,
        connection_id: &str,
        expected_version: u64,
        presentation_id: Option<&str>,
        input: RemoteConnectionInput,
    ) -> Result<PresentedRemoteConnection, String> {
        let _mutation = self.mutation.lock().await;
        self.reset_runtime().await;
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
        let prepared = self
            .spec(client, owner_user_id, Some(connection_id), &input)
            .await?;
        let previous = self
            .credentials
            .load(owner_user_id, connection_id)?
            .unwrap_or_default();
        let credential_mutation = self.credential_file_selections.resolve_mutation(
            owner_user_id,
            presentation_id,
            &input.credentials,
        )?;
        let next = previous.clone().applying(
            &credential_mutation,
            input.authentication_type,
            prepared.retain_manual_jump_credentials,
        )?;
        self.credentials.save(owner_user_id, connection_id, &next)?;
        let result = client
            .execute(HostCommand::UpdateRemoteConnection(
                UpdateRemoteConnectionCommand {
                    owner_user_id: owner_user_id.to_owned(),
                    connection_id: connection_id.to_owned(),
                    expected_version,
                    spec: prepared.spec,
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
        let presented = PresentedRemoteConnection::from_record(connection, next.flags());
        if let Some(presentation_id) = presentation_id {
            self.credential_file_selections
                .discard(owner_user_id, presentation_id);
        }
        Ok(presented)
    }

    async fn delete(
        &self,
        client: &dyn RemoteConnectionMetadataClient,
        owner_user_id: &str,
        connection_id: &str,
        expected_version: u64,
    ) -> Result<String, String> {
        let _mutation = self.mutation.lock().await;
        self.reset_runtime().await;
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
    ) -> Result<PreparedRemoteSpec, String> {
        let route = self.route_provider.route(owner_user_id)?;
        let mut jump_connection_id = clean(input.jump_connection_id.clone());
        let mut jump_host = clean(input.jump_host.clone());
        let mut jump_port = input.jump_port;
        let mut jump_username = clean(input.jump_username.clone());
        let retain_manual_jump_credentials;
        let mut referenced_jump = None;
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
            jump_host = Some(jump.host.clone());
            jump_port = Some(jump.port);
            jump_username = Some(jump.username.clone());
            referenced_jump = Some(jump);
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
        Ok(PreparedRemoteSpec {
            spec,
            retain_manual_jump_credentials,
            referenced_jump,
        })
    }

    async fn test_draft(
        &self,
        client: &dyn RemoteConnectionMetadataClient,
        owner_user_id: &str,
        input: RemoteConnectionInput,
        presentation_id: Option<&str>,
        credential_reference_id: Option<String>,
        verification_code: Option<String>,
    ) -> Result<PresentedRemoteConnectionTestResult, String> {
        let _mutation = self.mutation.lock().await;
        validate_owner(owner_user_id)?;
        let base_credentials = match credential_reference_id.as_deref() {
            Some(connection_id) => {
                get_record(client, owner_user_id, connection_id)
                    .await?
                    .ok_or_else(|| "远端连接不存在。".to_owned())?;
                self.credentials
                    .load(owner_user_id, connection_id)?
                    .unwrap_or_default()
            }
            None => RemoteCredentialBundle::default(),
        };
        let prepared = self
            .spec(
                client,
                owner_user_id,
                credential_reference_id.as_deref(),
                &input,
            )
            .await?;
        let credential_mutation = self.credential_file_selections.resolve_mutation(
            owner_user_id,
            presentation_id,
            &input.credentials,
        )?;
        let credentials = base_credentials.applying(
            &credential_mutation,
            input.authentication_type,
            prepared.retain_manual_jump_credentials,
        )?;
        let (jump_authentication_type, jump_credentials) =
            self.resolved_jump_credentials(owner_user_id, prepared.referenced_jump.as_ref())?;
        self.tester
            .test(
                SshConnectionMaterial {
                    spec: prepared.spec,
                    credentials,
                    jump_authentication_type,
                    jump_credentials,
                },
                verification_code,
            )
            .await
    }

    async fn test_saved(
        &self,
        client: &dyn RemoteConnectionMetadataClient,
        owner_user_id: &str,
        connection_id: &str,
        verification_code: Option<String>,
    ) -> Result<PresentedRemoteConnectionTestResult, String> {
        let _mutation = self.mutation.lock().await;
        let material = self
            .saved_material(client, owner_user_id, connection_id)
            .await?;
        self.tester.test(material, verification_code).await
    }

    async fn saved_material(
        &self,
        client: &dyn RemoteConnectionMetadataClient,
        owner_user_id: &str,
        connection_id: &str,
    ) -> Result<SshConnectionMaterial, String> {
        validate_identity(owner_user_id, connection_id)?;
        let current = get_record(client, owner_user_id, connection_id)
            .await?
            .ok_or_else(|| "远端连接不存在。".to_owned())?;
        let referenced_jump = match current.jump_connection_id.as_deref() {
            Some(jump_id) => Some(
                get_record(client, owner_user_id, jump_id)
                    .await?
                    .ok_or_else(|| "所选跳板机连接已不存在。".to_owned())?,
            ),
            None => None,
        };
        let spec = spec_from_record(current, referenced_jump.as_ref())?;
        let credentials = self
            .credentials
            .load(owner_user_id, connection_id)?
            .unwrap_or_default();
        let (jump_authentication_type, jump_credentials) =
            self.resolved_jump_credentials(owner_user_id, referenced_jump.as_ref())?;
        Ok(SshConnectionMaterial {
            spec,
            credentials,
            jump_authentication_type,
            jump_credentials,
        })
    }

    fn resolved_jump_credentials(
        &self,
        owner_user_id: &str,
        referenced_jump: Option<&LocalRemoteConnection>,
    ) -> Result<
        (
            Option<chatos_local_agent_protocol::LocalRemoteAuthenticationType>,
            Option<RemoteCredentialBundle>,
        ),
        String,
    > {
        let Some(jump) = referenced_jump else {
            return Ok((None, None));
        };
        Ok((
            Some(jump.authentication_type),
            self.credentials.load(owner_user_id, &jump.connection_id)?,
        ))
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

    pub(crate) async fn reset(&self) {
        self.credential_file_selections.reset();
        self.reset_runtime().await;
    }

    async fn reset_runtime(&self) {
        self.file_selections.reset();
        self.terminal_sessions.reset().await;
        self.terminal_tester.reset().await;
        self.tester.reset().await;
    }
}

struct PreparedRemoteSpec {
    spec: LocalRemoteConnectionSpec,
    retain_manual_jump_credentials: bool,
    referenced_jump: Option<LocalRemoteConnection>,
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
    presentation_id: String,
    input: RemoteConnectionInput,
) -> Result<PresentedRemoteConnection, String> {
    authentication
        .access_token_for_owner(&owner_user_id)
        .await?;
    credential_files::validate_presentation_id(&presentation_id)?;
    manager
        .create(&*host, &owner_user_id, Some(&presentation_id), input)
        .await
}

#[tauri::command]
pub async fn update_remote_connection(
    host: State<'_, HostManager>,
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, RemoteConnectionManager>,
    owner_user_id: String,
    connection_id: String,
    expected_version: u64,
    presentation_id: String,
    input: RemoteConnectionInput,
) -> Result<PresentedRemoteConnection, String> {
    authentication
        .access_token_for_owner(&owner_user_id)
        .await?;
    credential_files::validate_presentation_id(&presentation_id)?;
    manager
        .update(
            &*host,
            &owner_user_id,
            &connection_id,
            expected_version,
            Some(&presentation_id),
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

#[tauri::command]
pub async fn test_remote_connection_draft(
    host: State<'_, HostManager>,
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, RemoteConnectionManager>,
    owner_user_id: String,
    input: RemoteConnectionInput,
    presentation_id: String,
    credential_reference_id: Option<String>,
    verification_code: Option<String>,
) -> Result<PresentedRemoteConnectionTestResult, String> {
    authentication
        .access_token_for_owner(&owner_user_id)
        .await?;
    credential_files::validate_presentation_id(&presentation_id)?;
    manager
        .test_draft(
            &*host,
            &owner_user_id,
            input,
            Some(&presentation_id),
            credential_reference_id,
            verification_code,
        )
        .await
}

#[tauri::command]
pub async fn test_saved_remote_connection(
    host: State<'_, HostManager>,
    authentication: State<'_, AuthenticationManager>,
    manager: State<'_, RemoteConnectionManager>,
    owner_user_id: String,
    connection_id: String,
    verification_code: Option<String>,
) -> Result<PresentedRemoteConnectionTestResult, String> {
    authentication
        .access_token_for_owner(&owner_user_id)
        .await?;
    manager
        .test_saved(&*host, &owner_user_id, &connection_id, verification_code)
        .await
}

pub fn run_ssh_askpass_helper_if_requested() -> bool {
    ssh::run_askpass_helper_if_requested()
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

fn spec_from_record(
    record: LocalRemoteConnection,
    referenced_jump: Option<&LocalRemoteConnection>,
) -> Result<LocalRemoteConnectionSpec, String> {
    let (jump_host, jump_port, jump_username) = referenced_jump
        .map(|jump| {
            (
                Some(jump.host.clone()),
                Some(jump.port),
                Some(jump.username.clone()),
            )
        })
        .unwrap_or((record.jump_host, record.jump_port, record.jump_username));
    let spec = LocalRemoteConnectionSpec {
        name: Some(record.name),
        host: record.host,
        port: record.port,
        username: record.username,
        authentication_type: record.authentication_type,
        default_remote_path: record.default_remote_path,
        host_key_policy: record.host_key_policy,
        local_connector_device_id: record.local_connector_device_id,
        local_connector_workspace_id: record.local_connector_workspace_id,
        jump_enabled: record.jump_enabled,
        jump_connection_id: record.jump_connection_id,
        jump_host,
        jump_port,
        jump_username,
    };
    spec.validate()
        .map_err(|_| "远端连接信息无效。".to_owned())?;
    Ok(spec)
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
