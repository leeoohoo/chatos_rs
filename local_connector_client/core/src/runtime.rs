// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use sha2::{Digest, Sha256};
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;

use crate::config::ClientConfig;
use crate::connector::connect_loop;
use crate::local_runtime::{sync_local_plugin_control_plane, LocalDatabase};
use crate::model_configs::reconcile_local_model_configs;
use crate::plugins::{
    PluginCredentialVault, PluginInstaller, PluginMcpAdapter, PluginOAuthBroker, PluginRuntimeHost,
    PluginSkillLoader,
};
use crate::registration::{
    ensure_default_filesystem_workspace_registered, ensure_device_registered,
    ensure_workspace_registered, is_cloud_authentication_expired,
};
use crate::remote_connection::RemoteSftpManager;
use crate::sandbox::managed_requirements::{
    load_system_client_config, resolve_startup_managed_requirements,
};
use crate::sandbox::types::LocalSandboxRuntime;
use crate::{tracing_stdout, LocalState};

#[derive(Debug, Clone, PartialEq, Eq)]
struct ConnectorIdentity {
    cloud_base_url: String,
    access_token_sha256: [u8; 32],
    device_id: String,
}

impl ConnectorIdentity {
    fn new(config: &ClientConfig, device_id: &str) -> Self {
        Self {
            cloud_base_url: config.cloud_base_url.clone(),
            access_token_sha256: Sha256::digest(config.access_token.as_bytes()).into(),
            device_id: device_id.to_string(),
        }
    }
}

#[derive(Debug)]
pub(crate) struct ConnectorTask {
    identity: ConnectorIdentity,
    handle: JoinHandle<()>,
}

impl ConnectorTask {
    pub(crate) fn is_running(&self) -> bool {
        !self.handle.is_finished()
    }

    fn matches_running(&self, identity: &ConnectorIdentity) -> bool {
        self.identity == *identity && self.is_running()
    }

    fn abort(self) {
        self.handle.abort();
    }
}

#[derive(Debug, Clone)]
pub(crate) struct LocalRuntime {
    pub(crate) state_path: PathBuf,
    pub(crate) state: Arc<RwLock<LocalState>>,
    pub(crate) http_client: reqwest::Client,
    pub(crate) database: Option<LocalDatabase>,
    pub(crate) connector_task: Arc<Mutex<Option<ConnectorTask>>>,
    pub(crate) plugin_auto_update_lock: Arc<Mutex<()>>,
    pub(crate) sandbox_runtime: LocalSandboxRuntime,
    pub(crate) plugin_installer: PluginInstaller,
    pub(crate) plugin_credentials: PluginCredentialVault,
    pub(crate) plugin_oauth: PluginOAuthBroker,
    pub(crate) plugin_runtime: PluginRuntimeHost,
    pub(crate) remote_sftp_manager: RemoteSftpManager,
}

impl LocalRuntime {
    pub(crate) fn new(
        state_path: PathBuf,
        state: Arc<RwLock<LocalState>>,
        http_client: reqwest::Client,
        database: LocalDatabase,
    ) -> Self {
        let plugin_credentials = PluginCredentialVault::for_state_path(state_path.as_path());
        let plugin_installer = PluginInstaller::for_state_path(state_path.as_path())
            .with_credential_vault(plugin_credentials.clone());
        let plugin_oauth =
            PluginOAuthBroker::new(plugin_installer.clone(), plugin_credentials.clone());
        let plugin_runtime = PluginRuntimeHost::new(
            PluginSkillLoader::new(plugin_installer.clone()),
            PluginMcpAdapter::new(plugin_installer.clone()).with_oauth_broker(plugin_oauth.clone()),
        )
        .with_local_state(state.clone())
        .with_approval_state_path(state_path.clone());
        Self {
            state_path,
            state,
            http_client,
            database: Some(database),
            connector_task: Arc::new(Mutex::new(None)),
            plugin_auto_update_lock: Arc::new(Mutex::new(())),
            sandbox_runtime: LocalSandboxRuntime::default(),
            plugin_installer,
            plugin_credentials,
            plugin_oauth,
            plugin_runtime,
            remote_sftp_manager: RemoteSftpManager::default(),
        }
    }

    pub(crate) fn local_database(&self) -> Result<&LocalDatabase> {
        self.database
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("local connector state database is unavailable"))
    }

    pub(crate) async fn reload_managed_requirements_for_current_identity(&self) -> Result<()> {
        let result = async {
            let client_config = load_system_client_config()?;
            let state_snapshot = self.state.read().await.clone();
            let connector_config =
                ClientConfig::from_state(&state_snapshot, self.state_path.clone());
            let resolved = resolve_startup_managed_requirements(
                &self.http_client,
                self.state_path.as_path(),
                &state_snapshot,
                connector_config.as_ref(),
                client_config,
            )
            .await?;
            {
                let mut state = self.state.write().await;
                state
                    .sandbox
                    .load_runtime_permission_profile_layers(resolved.document)?;
            }
            if let Some(refresh) = resolved.background_refresh {
                refresh.spawn(self.http_client.clone());
            }
            Ok(())
        }
        .await;
        if let Err(err) = result {
            let mut state = self.state.write().await;
            state
                .sandbox
                .block_runtime_permission_profile_layers(format!("{err:#}"));
            return Err(err);
        }
        Ok(())
    }

    pub(crate) async fn sync_saved_workspaces_if_needed(&self) -> Result<()> {
        let config = {
            let state = self.state.read().await;
            ClientConfig::from_state(&state, self.state_path.clone())
        };
        let Some(config) = config else {
            return Ok(());
        };
        config.ensure_remote_urls_allowed()?;
        let mut state = self.state.write().await;
        let previous_device_id = state.device_id.clone();
        let saved_workspaces = state.workspaces.clone();
        let device_id = ensure_device_registered(&self.http_client, &config, &mut state).await?;
        let device_changed = previous_device_id.as_deref() != Some(device_id.as_str());
        for workspace in saved_workspaces {
            let workspace_config = ClientConfig {
                workspace_alias: Some(workspace.alias.clone()),
                ..config.clone()
            };
            if let Err(err) = ensure_workspace_registered(
                &self.http_client,
                &workspace_config,
                &mut state,
                device_id.as_str(),
                workspace.absolute_root.clone(),
                device_changed,
                Some(workspace.fingerprint.as_str()),
            )
            .await
            {
                tracing_stdout(
                    format!(
                        "sync saved workspace {} failed: {err}",
                        workspace.absolute_root.display()
                    )
                    .as_str(),
                );
            }
        }
        ensure_default_filesystem_workspace_registered(
            &self.http_client,
            &config,
            &mut state,
            device_id.as_str(),
        )
        .await?;
        state.save(self.state_path.as_path())?;
        Ok(())
    }

    pub(crate) async fn start_connector_if_configured(&self) -> Result<()> {
        let result = self.start_connector_if_configured_inner().await;
        if let Err(error) = result {
            if is_cloud_authentication_expired(&error) {
                self.clear_expired_cloud_auth().await?;
                return Err(anyhow!(
                    "Local Connector saved login expired; sign in again to reconnect"
                ));
            }
            return Err(error);
        }
        Ok(())
    }

    pub(crate) async fn stop_connector(&self) {
        if let Some(task) = self.connector_task.lock().await.take() {
            task.abort();
        }
    }

    async fn clear_expired_cloud_auth(&self) -> Result<()> {
        let mut state = self.state.write().await;
        state.auth = None;
        state.save(self.state_path.as_path())?;
        Ok(())
    }

    async fn start_connector_if_configured_inner(&self) -> Result<()> {
        self.sync_saved_workspaces_if_needed().await?;
        {
            let mut state = self.state.write().await;
            match reconcile_local_model_configs(&self.http_client, &mut state).await {
                Ok(synced) => {
                    if synced > 0 {
                        tracing_stdout(
                            format!(
                                "synchronized {synced} server-authoritative model config change(s)"
                            )
                            .as_str(),
                        );
                    }
                    state.save(self.state_path.as_path())?;
                }
                Err(err) => {
                    tracing_stdout(format!("reconcile saved model configs failed: {err}").as_str())
                }
            }
        }
        match sync_local_plugin_control_plane(self).await {
            Ok(synced) if synced > 0 => tracing_stdout(
                format!("synced {synced} local approval Agent configuration").as_str(),
            ),
            Ok(_) => {}
            Err(err) => tracing_stdout(
                format!("keep cached local approval Agent configuration: {err}").as_str(),
            ),
        }
        let config = {
            let state = self.state.read().await;
            ClientConfig::from_state(&state, self.state_path.clone())
        };
        let Some(config) = config else {
            return Ok(());
        };
        config.ensure_remote_urls_allowed()?;
        let device_id = {
            let mut state = self.state.write().await;
            let device_id =
                ensure_device_registered(&self.http_client, &config, &mut state).await?;
            state.save(self.state_path.as_path())?;
            device_id
        };
        let database = self.local_database()?.clone();
        let identity = ConnectorIdentity::new(&config, device_id.as_str());

        let mut current = self.connector_task.lock().await;
        if current
            .as_ref()
            .is_some_and(|task| task.matches_running(&identity))
        {
            return Ok(());
        }
        if let Some(task) = current.take() {
            task.abort();
        }
        let runtime = self.clone();
        let handle = tokio::spawn(async move {
            loop {
                let maybe_config = {
                    let state = runtime.state.read().await;
                    ClientConfig::from_state(&state, runtime.state_path.clone())
                };
                let Some(config) = maybe_config else {
                    break;
                };
                let device_id = {
                    let state = runtime.state.read().await;
                    state.device_id.clone().unwrap_or_else(|| device_id.clone())
                };
                if let Err(err) = connect_loop(
                    config,
                    runtime.state.clone(),
                    database.clone(),
                    runtime.sandbox_runtime.clone(),
                    runtime.plugin_runtime.clone(),
                    runtime.plugin_oauth.clone(),
                    runtime.remote_sftp_manager.clone(),
                    device_id,
                )
                .await
                {
                    if is_cloud_authentication_expired(&err) {
                        if let Err(clear_error) = runtime.clear_expired_cloud_auth().await {
                            tracing_stdout(
                                format!(
                                    "clear expired Local Connector login failed: {clear_error}"
                                )
                                .as_str(),
                            );
                        } else {
                            tracing_stdout(
                                "Local Connector saved login expired; sign in again to reconnect",
                            );
                        }
                        break;
                    }
                    tracing_stdout(format!("connector loop stopped: {err}").as_str());
                    tokio::time::sleep(Duration::from_secs(3)).await;
                    continue;
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        });
        *current = Some(ConnectorTask { identity, handle });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{ConnectorIdentity, ConnectorTask};
    use sha2::{Digest, Sha256};

    #[tokio::test]
    async fn connector_task_only_matches_same_running_identity() {
        let identity = ConnectorIdentity {
            cloud_base_url: "http://127.0.0.1:39230".to_string(),
            access_token_sha256: Sha256::digest(b"token-a").into(),
            device_id: "device-a".to_string(),
        };
        let task = ConnectorTask {
            identity: identity.clone(),
            handle: tokio::spawn(std::future::pending()),
        };

        assert!(task.matches_running(&identity));
        assert!(!task.matches_running(&ConnectorIdentity {
            access_token_sha256: Sha256::digest(b"token-b").into(),
            ..identity.clone()
        }));
        task.abort();
    }
}
