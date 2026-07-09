// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::Utc;
use tokio::sync::RwLock;

mod api;
mod approval;
mod config;
mod connector;
mod history;
mod mcp;
mod model_configs;
mod registration;
mod relay;
mod runtime;
mod sandbox;
mod state;
mod terminal;
#[cfg(test)]
mod tests;
mod workspace;

use crate::api::serve_local_api;
use crate::config::{default_state_path, load_dotenv, optional_env, ClientConfig};
use crate::registration::bootstrap_env_config;
pub(crate) use chatos_mcp_service::LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER;
pub(crate) use runtime::LocalRuntime;
pub(crate) use state::{AuthState, AuthUserState, LocalState, WorkspaceState};

pub(crate) const DEFAULT_LOCAL_SANDBOX_IMAGE: &str = "chatos-sandbox-agent:latest";
pub(crate) const DEFAULT_LOCAL_SANDBOX_IMAGE_TAG_PREFIX: &str = "chatos-sandbox-agent";
pub(crate) const LOCAL_SANDBOX_BACKEND: &str = "docker";
pub(crate) const LOCAL_SANDBOX_STATUS_READY: &str = "ready";
pub(crate) const LOCAL_SANDBOX_STATUS_DESTROYED: &str = "destroyed";
const DEFAULT_TERMINAL_EXEC_TIMEOUT_MS: u64 = 30_000;
const MAX_TERMINAL_EXEC_TIMEOUT_MS: u64 = 10 * 60 * 1000;
pub(crate) const MAX_TERMINAL_OUTPUT_BYTES: usize = 512 * 1024;
const MAX_LOCAL_MCP_READ_BYTES: u64 = 256 * 1024;
const MAX_LOCAL_MCP_WRITE_BYTES: usize = 1024 * 1024;
const MAX_LOCAL_MCP_SEARCH_RESULTS: usize = 500;
const MAX_COMMAND_HISTORY_ENTRIES: usize = 1_000;
const LOCAL_CONNECTOR_ROOT_PREFIX: &str = "local://connector/";

#[tokio::main]
async fn main() -> Result<()> {
    load_dotenv();
    let state_path = optional_env("LOCAL_CONNECTOR_STATE_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(default_state_path);
    let state = Arc::new(RwLock::new(LocalState::load(state_path.as_path())?));
    let http_client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .context("build HTTP client")?;

    if let Ok(config) = ClientConfig::from_env() {
        bootstrap_env_config(&http_client, &config, &state).await?;
    }

    let runtime = LocalRuntime::new(state_path, state, http_client);
    if let Err(err) = runtime.start_connector_if_configured().await {
        tracing_stdout(format!("start connector from saved config failed: {err}").as_str());
    }

    serve_local_api(runtime).await
}

pub(crate) fn local_now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

pub(crate) fn select_local_shell() -> String {
    if cfg!(windows) {
        return std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string());
    }
    std::env::var("SHELL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| {
            if Path::new("/bin/zsh").exists() {
                "/bin/zsh".to_string()
            } else {
                "/bin/sh".to_string()
            }
        })
}

pub(crate) fn tracing_stdout(message: &str) {
    println!("[local-connector] {message}");
}
