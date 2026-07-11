// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;

use crate::config::ClientConfig;
use crate::connector::connect_loop;
use crate::registration::{ensure_device_registered, ensure_workspace_registered};
use crate::sandbox::types::LocalSandboxRuntime;
use crate::{tracing_stdout, LocalState};

#[derive(Debug, Clone)]
pub(crate) struct LocalRuntime {
    pub(crate) state_path: PathBuf,
    pub(crate) state: Arc<RwLock<LocalState>>,
    pub(crate) http_client: reqwest::Client,
    pub(crate) connector_task: Arc<Mutex<Option<JoinHandle<()>>>>,
    pub(crate) sandbox_runtime: LocalSandboxRuntime,
}

impl LocalRuntime {
    pub(crate) fn new(
        state_path: PathBuf,
        state: Arc<RwLock<LocalState>>,
        http_client: reqwest::Client,
    ) -> Self {
        Self {
            state_path,
            state,
            http_client,
            connector_task: Arc::new(Mutex::new(None)),
            sandbox_runtime: LocalSandboxRuntime::default(),
        }
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
        self.sync_saved_workspaces_if_needed().await?;
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
                    runtime.sandbox_runtime.clone(),
                    device_id,
                )
                .await
                {
                    tracing_stdout(format!("connector loop stopped: {err}").as_str());
                    tokio::time::sleep(Duration::from_secs(3)).await;
                    continue;
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }));
        Ok(())
    }
}
