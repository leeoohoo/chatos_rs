// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::time::Duration;

use chatos_service_runtime::{
    DEFAULT_MEMORY_ENGINE_OPERATOR_TOKEN, DEFAULT_SANDBOX_MANAGER_SYSTEM_CLIENT_ID,
    DEFAULT_SANDBOX_MANAGER_SYSTEM_CLIENT_KEY,
};

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub host: IpAddr,
    pub port: u16,
    pub database_url: String,
    pub user_service_base_url: String,
    pub user_service_request_timeout: Duration,
    pub user_service_internal_secret: Option<String>,
    pub local_connector_service_base_url: String,
    pub local_connector_service_request_timeout: Duration,
    pub memory_engine_base_url: String,
    pub memory_engine_source_id: String,
    pub memory_engine_operator_token: Option<String>,
    pub memory_engine_request_timeout: Duration,
    pub sandbox_manager_base_url: String,
    pub sandbox_manager_client_id: Option<String>,
    pub sandbox_manager_client_key: Option<String>,
    pub sandbox_image_mcp_request_timeout: Duration,
    pub cloud_project_import_enabled: bool,
    pub cloud_project_max_zip_bytes: usize,
    pub cloud_project_max_unpacked_bytes: u64,
    pub cloud_project_max_files: usize,
    pub cloud_project_git_timeout: Duration,
    pub task_runner_base_url: Option<String>,
    pub task_runner_request_timeout: Duration,
    pub task_runner_internal_secret: Option<String>,
    pub sync_secret: Option<String>,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, String> {
        let host = std::env::var("PROJECT_SERVICE_HOST")
            .ok()
            .and_then(|value| value.parse::<IpAddr>().ok())
            .unwrap_or(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
        let port = std::env::var("PROJECT_SERVICE_PORT")
            .ok()
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(39210);
        let user_service_request_timeout_ms =
            std::env::var("PROJECT_SERVICE_USER_SERVICE_REQUEST_TIMEOUT_MS")
                .ok()
                .or_else(|| std::env::var("CHATOS_USER_SERVICE_REQUEST_TIMEOUT_MS").ok())
                .or_else(|| std::env::var("USER_SERVICE_DOWNSTREAM_REQUEST_TIMEOUT_MS").ok())
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(5_000)
                .max(300);
        let task_runner_request_timeout_ms =
            std::env::var("PROJECT_SERVICE_TASK_RUNNER_REQUEST_TIMEOUT_MS")
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(10_000)
                .max(300);
        let local_connector_service_request_timeout_ms =
            std::env::var("PROJECT_SERVICE_LOCAL_CONNECTOR_SERVICE_REQUEST_TIMEOUT_MS")
                .ok()
                .or_else(|| std::env::var("CHATOS_LOCAL_CONNECTOR_SERVICE_REQUEST_TIMEOUT_MS").ok())
                .or_else(|| std::env::var("LOCAL_CONNECTOR_SERVICE_REQUEST_TIMEOUT_MS").ok())
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(5_000)
                .max(300);
        let memory_engine_request_timeout_ms =
            std::env::var("PROJECT_SERVICE_MEMORY_ENGINE_REQUEST_TIMEOUT_MS")
                .ok()
                .or_else(|| std::env::var("MEMORY_ENGINE_REQUEST_TIMEOUT_MS").ok())
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(30_000)
                .max(300);
        let sandbox_image_mcp_request_timeout_ms =
            std::env::var("PROJECT_SERVICE_SANDBOX_IMAGE_MCP_REQUEST_TIMEOUT_MS")
                .ok()
                .or_else(|| std::env::var("SANDBOX_IMAGE_MCP_REQUEST_TIMEOUT_MS").ok())
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(2 * 60 * 60 * 1_000)
                .max(10_000);
        let cloud_project_git_timeout_ms =
            std::env::var("PROJECT_SERVICE_CLOUD_PROJECT_GIT_TIMEOUT_MS")
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(120_000)
                .max(1_000);

        Ok(Self {
            host,
            port,
            database_url: normalized_env("PROJECT_SERVICE_DATABASE_URL")
                .unwrap_or_else(default_database_url),
            user_service_base_url: normalized_env("PROJECT_SERVICE_USER_SERVICE_BASE_URL")
                .or_else(|| normalized_env("CHATOS_USER_SERVICE_BASE_URL"))
                .or_else(|| normalized_env("USER_SERVICE_BASE_URL"))
                .unwrap_or_else(default_user_service_base_url),
            user_service_request_timeout: Duration::from_millis(user_service_request_timeout_ms),
            user_service_internal_secret: normalized_env(
                "PROJECT_SERVICE_USER_SERVICE_INTERNAL_SECRET",
            )
            .or_else(|| normalized_env("CHATOS_USER_SERVICE_INTERNAL_SECRET"))
            .or_else(|| normalized_env("USER_SERVICE_INTERNAL_API_SECRET")),
            local_connector_service_base_url: normalized_env(
                "PROJECT_SERVICE_LOCAL_CONNECTOR_SERVICE_BASE_URL",
            )
            .or_else(|| normalized_env("CHATOS_LOCAL_CONNECTOR_SERVICE_BASE_URL"))
            .or_else(|| normalized_env("LOCAL_CONNECTOR_SERVICE_BASE_URL"))
            .unwrap_or_else(default_local_connector_service_base_url),
            local_connector_service_request_timeout: Duration::from_millis(
                local_connector_service_request_timeout_ms,
            ),
            memory_engine_base_url: normalized_env("PROJECT_SERVICE_MEMORY_ENGINE_BASE_URL")
                .or_else(|| normalized_env("MEMORY_ENGINE_BASE_URL"))
                .unwrap_or_else(default_memory_engine_base_url),
            memory_engine_source_id: normalized_env("PROJECT_SERVICE_MEMORY_ENGINE_SOURCE_ID")
                .or_else(|| normalized_env("MEMORY_ENGINE_SOURCE_ID"))
                .unwrap_or_else(|| "project_management_agent".to_string()),
            memory_engine_operator_token: normalized_env(
                "PROJECT_SERVICE_MEMORY_ENGINE_OPERATOR_TOKEN",
            )
            .or_else(|| normalized_env("MEMORY_ENGINE_OPERATOR_TOKEN"))
            .or_else(|| Some(DEFAULT_MEMORY_ENGINE_OPERATOR_TOKEN.to_string())),
            memory_engine_request_timeout: Duration::from_millis(memory_engine_request_timeout_ms),
            sandbox_manager_base_url: normalized_env("PROJECT_SERVICE_SANDBOX_MANAGER_BASE_URL")
                .or_else(|| normalized_env("SANDBOX_MANAGER_BASE_URL"))
                .unwrap_or_else(default_sandbox_manager_base_url),
            sandbox_manager_client_id: normalized_env("PROJECT_SERVICE_SANDBOX_MANAGER_CLIENT_ID")
                .or_else(|| normalized_env("SANDBOX_MANAGER_SYSTEM_CLIENT_ID"))
                .or_else(|| Some(DEFAULT_SANDBOX_MANAGER_SYSTEM_CLIENT_ID.to_string())),
            sandbox_manager_client_key: normalized_env(
                "PROJECT_SERVICE_SANDBOX_MANAGER_CLIENT_KEY",
            )
            .or_else(|| normalized_env("SANDBOX_MANAGER_SYSTEM_CLIENT_KEY"))
            .or_else(|| Some(DEFAULT_SANDBOX_MANAGER_SYSTEM_CLIENT_KEY.to_string())),
            sandbox_image_mcp_request_timeout: Duration::from_millis(
                sandbox_image_mcp_request_timeout_ms,
            ),
            cloud_project_import_enabled: read_bool_env(
                "PROJECT_SERVICE_CLOUD_PROJECT_IMPORT_ENABLED",
                true,
            )?,
            cloud_project_max_zip_bytes: normalized_env(
                "PROJECT_SERVICE_CLOUD_PROJECT_MAX_ZIP_BYTES",
            )
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(200 * 1024 * 1024),
            cloud_project_max_unpacked_bytes: normalized_env(
                "PROJECT_SERVICE_CLOUD_PROJECT_MAX_UNPACKED_BYTES",
            )
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(1024 * 1024 * 1024),
            cloud_project_max_files: normalized_env("PROJECT_SERVICE_CLOUD_PROJECT_MAX_FILES")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(20_000),
            cloud_project_git_timeout: Duration::from_millis(cloud_project_git_timeout_ms),
            task_runner_base_url: normalized_env("PROJECT_SERVICE_TASK_RUNNER_BASE_URL"),
            task_runner_request_timeout: Duration::from_millis(task_runner_request_timeout_ms),
            task_runner_internal_secret: normalized_env(
                "PROJECT_SERVICE_TASK_RUNNER_INTERNAL_SECRET",
            )
            .or_else(|| normalized_env("TASK_RUNNER_INTERNAL_API_SECRET"))
            .or_else(|| normalized_env("PROJECT_SERVICE_SYNC_SECRET")),
            sync_secret: normalized_env("PROJECT_SERVICE_SYNC_SECRET"),
        })
    }

    pub fn bind_addr(&self) -> SocketAddr {
        SocketAddr::new(self.host, self.port)
    }
}

pub fn load_project_service_dotenv() {
    for path in project_service_dotenv_files() {
        let _ = dotenvy::from_path(path);
    }
}

fn project_service_dotenv_files() -> Vec<PathBuf> {
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

fn read_bool_env(key: &str, default: bool) -> Result<bool, String> {
    let Some(value) = normalized_env(key) else {
        return Ok(default);
    };
    match value.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err(format!("invalid {key}: expected true/false")),
    }
}

fn default_database_url() -> String {
    let database = normalized_env("PROJECT_SERVICE_MONGODB_DATABASE")
        .or_else(|| normalized_env("MONGODB_DB").map(|value| format!("{value}_project_management")))
        .unwrap_or_else(|| "project_management_service".to_string());
    let host = normalized_env("PROJECT_SERVICE_MONGODB_HOST")
        .or_else(|| normalized_env("DEV_MONGO_HOST"))
        .or_else(|| normalized_env("MONGODB_HOST"))
        .map(|value| match value.as_str() {
            "0.0.0.0" | "::" | "[::]" => "127.0.0.1".to_string(),
            _ => value,
        })
        .unwrap_or_else(|| "127.0.0.1".to_string());
    let port = normalized_env("PROJECT_SERVICE_MONGODB_PORT")
        .or_else(|| normalized_env("DEV_MONGO_PORT"))
        .or_else(|| normalized_env("MONGODB_PORT"))
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(27018);
    let user = normalized_env("PROJECT_SERVICE_MONGODB_USER")
        .or_else(|| normalized_env("MONGODB_USER"))
        .unwrap_or_else(|| "admin".to_string());
    let password = normalized_env("PROJECT_SERVICE_MONGODB_PASSWORD")
        .or_else(|| normalized_env("MONGODB_PASSWORD"))
        .unwrap_or_else(|| "admin".to_string());
    let auth_source = normalized_env("PROJECT_SERVICE_MONGODB_AUTH_SOURCE")
        .or_else(|| normalized_env("MONGODB_AUTH_SOURCE"))
        .unwrap_or_else(|| "admin".to_string());
    format!("mongodb://{user}:{password}@{host}:{port}/{database}?authSource={auth_source}")
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

fn default_local_connector_service_base_url() -> String {
    let host = normalized_env("LOCAL_CONNECTOR_SERVICE_HOST")
        .map(|value| match value.as_str() {
            "0.0.0.0" | "::" | "[::]" => "127.0.0.1".to_string(),
            _ => value,
        })
        .unwrap_or_else(|| "127.0.0.1".to_string());
    let port = normalized_env("LOCAL_CONNECTOR_SERVICE_PORT")
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(39230);
    format!("http://{host}:{port}")
}

fn default_memory_engine_base_url() -> String {
    let host = normalized_env("MEMORY_ENGINE_HOST")
        .map(|value| match value.as_str() {
            "0.0.0.0" | "::" | "[::]" => "127.0.0.1".to_string(),
            _ => value,
        })
        .unwrap_or_else(|| "127.0.0.1".to_string());
    let port = normalized_env("MEMORY_ENGINE_PORT")
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(7081);
    format!("http://{host}:{port}/api/memory-engine/v1")
}

fn default_sandbox_manager_base_url() -> String {
    let host = normalized_env("SANDBOX_MANAGER_HOST")
        .map(|value| match value.as_str() {
            "0.0.0.0" | "::" | "[::]" => "127.0.0.1".to_string(),
            _ => value,
        })
        .unwrap_or_else(|| "127.0.0.1".to_string());
    let port = normalized_env("SANDBOX_MANAGER_PORT")
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(8095);
    format!("http://{host}:{port}")
}
