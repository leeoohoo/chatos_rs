// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};

use crate::LocalState;

pub(crate) const DEFAULT_CLOUD_BASE_URL: &str = "http://127.0.0.1:39230";
pub(crate) const DEFAULT_USER_SERVICE_BASE_URL: &str = "http://127.0.0.1:39190";
pub(crate) const DEFAULT_LOCAL_API_PORT: u16 = 39232;

#[derive(Debug, Clone)]
pub(crate) struct ClientConfig {
    pub(crate) cloud_base_url: String,
    pub(crate) access_token: String,
    pub(crate) device_name: String,
    pub(crate) public_key: Option<String>,
    pub(crate) workspace_path: Option<PathBuf>,
    pub(crate) workspace_alias: Option<String>,
    pub(crate) state_path: PathBuf,
}

impl ClientConfig {
    pub(crate) fn from_env() -> Result<Self> {
        let access_token = required_env("LOCAL_CONNECTOR_ACCESS_TOKEN")?;
        let cloud_base_url = optional_env("LOCAL_CONNECTOR_CLOUD_BASE_URL")
            .unwrap_or_else(|| DEFAULT_CLOUD_BASE_URL.to_string());
        let device_name =
            optional_env("LOCAL_CONNECTOR_DEVICE_NAME").unwrap_or_else(default_device_name);
        let public_key = optional_env("LOCAL_CONNECTOR_PUBLIC_KEY");
        let workspace_path = optional_env("LOCAL_CONNECTOR_WORKSPACE_PATH").map(PathBuf::from);
        let workspace_alias = optional_env("LOCAL_CONNECTOR_WORKSPACE_ALIAS");
        let state_path = optional_env("LOCAL_CONNECTOR_STATE_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(default_state_path);
        Ok(Self {
            cloud_base_url,
            access_token,
            device_name,
            public_key,
            workspace_path,
            workspace_alias,
            state_path,
        })
    }

    pub(crate) fn from_state(state: &LocalState, state_path: PathBuf) -> Option<Self> {
        let auth = state.auth.as_ref()?;
        Some(Self {
            cloud_base_url: auth.cloud_base_url.clone(),
            access_token: auth.access_token.clone(),
            device_name: auth.device_name.clone(),
            public_key: state.device_public_key.clone(),
            workspace_path: None,
            workspace_alias: None,
            state_path,
        })
    }
}

pub(crate) fn api_url(base: &str, path: &str) -> String {
    format!("{}{}", base.trim_end_matches('/'), path)
}

pub(crate) fn normalize_optional(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn required_env(key: &str) -> Result<String> {
    optional_env(key).ok_or_else(|| anyhow!("{key} is required"))
}

pub(crate) fn optional_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub(crate) fn default_state_path() -> PathBuf {
    home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".chatos")
        .join("local_connector")
        .join("state.json")
}

pub(crate) fn home_dir() -> Option<PathBuf> {
    std::env::var("HOME")
        .ok()
        .map(PathBuf::from)
        .or_else(|| std::env::var("USERPROFILE").ok().map(PathBuf::from))
}

pub(crate) fn default_device_name() -> String {
    optional_env("HOSTNAME")
        .or_else(|| optional_env("COMPUTERNAME"))
        .unwrap_or_else(|| "Local Connector".to_string())
}

pub(crate) fn load_dotenv() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    for path in [
        Some(manifest_dir.join(".env")),
        manifest_dir.parent().map(|path| path.join(".env")),
        manifest_dir
            .parent()
            .and_then(|path| path.parent())
            .map(|path| path.join(".env")),
    ]
    .into_iter()
    .flatten()
    {
        let _ = dotenvy::from_path(path);
    }
}
