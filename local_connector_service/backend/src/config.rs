// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub host: IpAddr,
    pub port: u16,
    pub database_url: String,
    pub user_service_base_url: String,
    pub user_service_request_timeout: Duration,
    pub relay_request_timeout: Duration,
    pub public_base_url: Option<String>,
    pub internal_api_secret: Option<String>,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, String> {
        let host = std::env::var("LOCAL_CONNECTOR_SERVICE_HOST")
            .ok()
            .and_then(|value| value.parse::<IpAddr>().ok())
            .unwrap_or(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
        let port = std::env::var("LOCAL_CONNECTOR_SERVICE_PORT")
            .ok()
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(39230);
        let timeout_ms = std::env::var("LOCAL_CONNECTOR_USER_SERVICE_REQUEST_TIMEOUT_MS")
            .ok()
            .or_else(|| std::env::var("CHATOS_USER_SERVICE_REQUEST_TIMEOUT_MS").ok())
            .or_else(|| std::env::var("USER_SERVICE_DOWNSTREAM_REQUEST_TIMEOUT_MS").ok())
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(5_000)
            .max(300);
        let relay_timeout_ms = std::env::var("LOCAL_CONNECTOR_RELAY_REQUEST_TIMEOUT_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(30_000)
            .max(1_000);

        Ok(Self {
            host,
            port,
            database_url: normalized_env("LOCAL_CONNECTOR_DATABASE_URL")
                .unwrap_or_else(default_database_url),
            user_service_base_url: normalized_env("LOCAL_CONNECTOR_USER_SERVICE_BASE_URL")
                .or_else(|| normalized_env("CHATOS_USER_SERVICE_BASE_URL"))
                .or_else(|| normalized_env("USER_SERVICE_BASE_URL"))
                .unwrap_or_else(default_user_service_base_url),
            user_service_request_timeout: Duration::from_millis(timeout_ms),
            relay_request_timeout: Duration::from_millis(relay_timeout_ms),
            public_base_url: normalized_env("LOCAL_CONNECTOR_PUBLIC_BASE_URL"),
            internal_api_secret: normalized_env("LOCAL_CONNECTOR_INTERNAL_API_SECRET")
                .or_else(|| normalized_env("CHATOS_LOCAL_CONNECTOR_INTERNAL_API_SECRET"))
                .or_else(|| normalized_env("TASK_RUNNER_LOCAL_CONNECTOR_INTERNAL_API_SECRET"))
                .or_else(|| normalized_env("TASK_RUNNER_INTERNAL_API_SECRET")),
        })
    }

    pub fn bind_addr(&self) -> SocketAddr {
        SocketAddr::new(self.host, self.port)
    }

    pub fn sandbox_facade_base_url(&self, pairing_id: &str) -> String {
        let path = format!("/api/local-connectors/sandbox-facade/{pairing_id}");
        match self.public_base_url.as_deref() {
            Some(base) => format!("{}{}", base.trim_end_matches('/'), path),
            None => path,
        }
    }
}

pub fn load_local_connector_dotenv() {
    for path in local_connector_dotenv_files() {
        let _ = dotenvy::from_path(path);
    }
}

fn local_connector_dotenv_files() -> Vec<PathBuf> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
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
        if !files.iter().any(|existing| existing == &path) {
            files.push(path);
        }
    }
    files
}

pub(crate) fn normalized_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn default_database_url() -> String {
    "sqlite://local_connector_service/data/local_connector.db".to_string()
}

fn default_user_service_base_url() -> String {
    let host = normalized_env("USER_SERVICE_HOST")
        .map(|value| match value.as_str() {
            "0.0.0.0" | "::" | "[::]" => "127.0.0.1".to_string(),
            _ => value,
        })
        .unwrap_or_else(|| "127.0.0.1".to_string());
    let port = normalized_env("USER_SERVICE_PORT")
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(39190);
    format!("http://{host}:{port}")
}
