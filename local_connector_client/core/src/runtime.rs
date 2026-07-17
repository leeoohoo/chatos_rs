// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;

use crate::config::ClientConfig;
use crate::connector::connect_loop;
use crate::local_runtime::{
    check_agent_prompt_updates, run_local_task_worker_loop, spawn_agent_prompt_update_checker,
    sync_local_plugin_control_plane, sync_managed_memory_policy, LocalAskUserPromptRegistry,
    LocalDatabase, LocalEnvironmentJobRegistry, LocalMemoryJobRegistry, LocalTurnControlRegistry,
};
use crate::model_configs::reconcile_local_model_configs;
use crate::registration::{
    ensure_device_registered, ensure_workspace_registered, is_cloud_authentication_expired,
};
use crate::sandbox::managed_requirements::{
    load_system_client_config, resolve_startup_managed_requirements,
};
use crate::sandbox::types::LocalSandboxRuntime;
use crate::{tracing_stdout, LocalState};

#[derive(Debug, Clone)]
pub(crate) struct LocalRuntime {
    pub(crate) state_path: PathBuf,
    pub(crate) state: Arc<RwLock<LocalState>>,
    pub(crate) http_client: reqwest::Client,
    pub(crate) database: Option<LocalDatabase>,
    pub(crate) turn_control: LocalTurnControlRegistry,
    pub(crate) memory_jobs: LocalMemoryJobRegistry,
    pub(crate) ask_user_prompts: LocalAskUserPromptRegistry,
    pub(crate) environment_jobs: LocalEnvironmentJobRegistry,
    pub(crate) connector_task: Arc<Mutex<Option<JoinHandle<()>>>>,
    pub(crate) task_worker_task: Arc<Mutex<Option<JoinHandle<()>>>>,
    pub(crate) agent_prompt_check_task: Arc<Mutex<Option<JoinHandle<()>>>>,
    pub(crate) sandbox_runtime: LocalSandboxRuntime,
}

impl LocalRuntime {
    pub(crate) fn new(
        state_path: PathBuf,
        state: Arc<RwLock<LocalState>>,
        http_client: reqwest::Client,
        database: LocalDatabase,
    ) -> Self {
        Self {
            state_path,
            state,
            http_client,
            database: Some(database),
            turn_control: LocalTurnControlRegistry::default(),
            memory_jobs: LocalMemoryJobRegistry::default(),
            ask_user_prompts: LocalAskUserPromptRegistry::default(),
            environment_jobs: LocalEnvironmentJobRegistry::default(),
            connector_task: Arc::new(Mutex::new(None)),
            task_worker_task: Arc::new(Mutex::new(None)),
            agent_prompt_check_task: Arc::new(Mutex::new(None)),
            sandbox_runtime: LocalSandboxRuntime::default(),
        }
    }

    pub(crate) fn local_database(&self) -> Result<&LocalDatabase> {
        self.database
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("local runtime database is unavailable"))
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
        {
            let mut current = self.agent_prompt_check_task.lock().await;
            if let Some(handle) = current.take() {
                handle.abort();
            }
            *current = Some(spawn_agent_prompt_update_checker(self.clone()));
        }

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
                format!("synced {synced} local Plugin capability snapshots").as_str(),
            ),
            Ok(_) => {}
            Err(err) => {
                tracing_stdout(format!("keep cached Plugin capability snapshots: {err}").as_str())
            }
        }
        match sync_managed_memory_policy(self).await {
            Ok(bundle) => tracing_stdout(
                format!(
                    "synced managed Memory Policy revision {} ({})",
                    bundle.revision, bundle.checksum
                )
                .as_str(),
            ),
            Err(err) => {
                tracing_stdout(format!("keep cached managed Memory Policy: {err}").as_str())
            }
        }
        if let Err(err) = check_agent_prompt_updates(self).await {
            tracing_stdout(format!("check Agent Prompt updates failed: {err}").as_str());
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

        let mut current = self.connector_task.lock().await;
        if let Some(handle) = current.take() {
            handle.abort();
        }
        let runtime = self.clone();
        *current = Some(tokio::spawn(async move {
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
        }));
        Ok(())
    }

    pub(crate) async fn start_local_task_worker(&self) {
        let mut current = self.task_worker_task.lock().await;
        if current.as_ref().is_some_and(|handle| !handle.is_finished()) {
            return;
        }
        if let Some(handle) = current.take() {
            handle.abort();
        }
        let runtime = self.clone();
        *current = Some(tokio::spawn(async move {
            run_local_task_worker_loop(runtime).await;
        }));
    }
}
