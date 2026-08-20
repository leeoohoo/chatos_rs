// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::time::Duration;

use chatos_service_runtime::{
    env_bool_strict as bool_env, env_text as normalized_env, is_production_environment,
    validate_production_secret,
};

pub const CONFIG_CENTER_CALLER_BOOTSTRAP_SECRETS: &[(&str, &str, &str)] = &[
    (
        "chatos-backend",
        "CONFIG_CENTER_CHATOS_BACKEND_CALLER_SIGNING_SECRET",
        "change_me_config_center_chatos_backend_signing_secret",
    ),
    (
        "local-connector-service",
        "CONFIG_CENTER_LOCAL_CONNECTOR_SERVICE_CALLER_SIGNING_SECRET",
        "change_me_config_center_local_connector_signing_secret",
    ),
    (
        "mcp-management-service",
        "CONFIG_CENTER_MCP_MANAGEMENT_SERVICE_CALLER_SIGNING_SECRET",
        "change_me_config_center_mcp_management_signing_secret",
    ),
    (
        "memory-engine",
        "CONFIG_CENTER_MEMORY_ENGINE_CALLER_SIGNING_SECRET",
        "change_me_config_center_memory_engine_signing_secret",
    ),
    (
        "official-website",
        "CONFIG_CENTER_OFFICIAL_WEBSITE_CALLER_SIGNING_SECRET",
        "change_me_config_center_official_website_signing_secret",
    ),
    (
        "plugin-management-service",
        "CONFIG_CENTER_PLUGIN_MANAGEMENT_SERVICE_CALLER_SIGNING_SECRET",
        "change_me_config_center_plugin_management_signing_secret",
    ),
    (
        "project-service",
        "CONFIG_CENTER_PROJECT_SERVICE_CALLER_SIGNING_SECRET",
        "change_me_config_center_project_service_signing_secret",
    ),
    (
        "task-runner",
        "CONFIG_CENTER_TASK_RUNNER_CALLER_SIGNING_SECRET",
        "change_me_config_center_task_runner_signing_secret",
    ),
    (
        "user-service",
        "CONFIG_CENTER_USER_SERVICE_CALLER_SIGNING_SECRET",
        "change_me_config_center_user_service_signing_secret",
    ),
];

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub host: IpAddr,
    pub port: u16,
    pub internal_mtls_port: u16,
    pub database_url: String,
    pub mongodb_database: String,
    pub user_service_base_url: String,
    pub user_service_request_timeout: Duration,
    pub consul_http_addr: Option<String>,
    pub consul_required: bool,
    pub caller_signing_secrets: BTreeMap<String, String>,
    pub mtls_server_cert_path: PathBuf,
    pub mtls_server_key_path: PathBuf,
    pub mtls_client_ca_cert_path: PathBuf,
    pub mcp_management_mtls_ca_cert_path: PathBuf,
    pub mcp_management_mtls_client_identity_path: PathBuf,
    pub memory_engine_mtls_ca_cert_path: PathBuf,
    pub memory_engine_mtls_client_identity_path: PathBuf,
    pub cors_origins: Vec<String>,
    pub default_environment: String,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, String> {
        let host = normalized_env("CONFIG_CENTER_HOST")
            .and_then(|value| value.parse::<IpAddr>().ok())
            .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
        let port = normalized_env("CONFIG_CENTER_PORT")
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(39270);
        let internal_mtls_port = normalized_env("CONFIG_CENTER_INTERNAL_MTLS_PORT")
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(39272);
        if internal_mtls_port == port {
            return Err(
                "CONFIG_CENTER_INTERNAL_MTLS_PORT must differ from CONFIG_CENTER_PORT".to_string(),
            );
        }
        let mongodb_database = normalized_env("CONFIG_CENTER_MONGODB_DATABASE")
            .unwrap_or_else(|| "configuration_center".to_string());
        let caller_signing_secrets = CONFIG_CENTER_CALLER_BOOTSTRAP_SECRETS
            .iter()
            .map(|(service_name, env_key, development_default)| {
                let secret =
                    normalized_env(env_key).unwrap_or_else(|| (*development_default).to_string());
                validate_production_secret(
                    env_key,
                    Some(secret.as_str()),
                    &[*development_default],
                )?;
                Ok(((*service_name).to_string(), secret))
            })
            .collect::<Result<BTreeMap<_, _>, String>>()?;
        let config = Self {
            host,
            port,
            internal_mtls_port,
            database_url: normalized_env("CONFIG_CENTER_DATABASE_URL").unwrap_or_else(|| {
                format!("mongodb://admin:admin@127.0.0.1:27018/{mongodb_database}?authSource=admin")
            }),
            mongodb_database,
            user_service_base_url: normalized_env("CONFIG_CENTER_USER_SERVICE_BASE_URL")
                .or_else(|| normalized_env("CHATOS_USER_SERVICE_BASE_URL"))
                .unwrap_or_else(|| "http://127.0.0.1:39190".to_string()),
            user_service_request_timeout: Duration::from_millis(
                normalized_env("CONFIG_CENTER_USER_SERVICE_REQUEST_TIMEOUT_MS")
                    .and_then(|value| value.parse::<u64>().ok())
                    .unwrap_or(5_000)
                    .max(300),
            ),
            consul_http_addr: normalized_env("CHATOS_CONSUL_HTTP_ADDR")
                .or_else(|| Some("http://127.0.0.1:8500".to_string())),
            consul_required: bool_env(
                "CONFIG_CENTER_CONSUL_REQUIRED",
                is_production_environment(),
            )?,
            caller_signing_secrets,
            mtls_server_cert_path: required_path_env("CONFIG_CENTER_MTLS_SERVER_CERT_PATH")?,
            mtls_server_key_path: required_path_env("CONFIG_CENTER_MTLS_SERVER_KEY_PATH")?,
            mtls_client_ca_cert_path: required_path_env("CONFIG_CENTER_MTLS_CLIENT_CA_CERT_PATH")?,
            mcp_management_mtls_ca_cert_path: required_path_env(
                "MCP_MANAGEMENT_MTLS_CA_CERT_PATH",
            )?,
            mcp_management_mtls_client_identity_path: required_path_env(
                "MCP_MANAGEMENT_MTLS_CLIENT_IDENTITY_PATH",
            )?,
            memory_engine_mtls_ca_cert_path: required_path_env("MEMORY_ENGINE_MTLS_CA_CERT_PATH")?,
            memory_engine_mtls_client_identity_path: required_path_env(
                "MEMORY_ENGINE_MTLS_CLIENT_IDENTITY_PATH",
            )?,
            cors_origins: normalized_env("CONFIG_CENTER_CORS_ORIGINS")
                .map(|value| {
                    value
                        .split(',')
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(ToOwned::to_owned)
                        .collect()
                })
                .unwrap_or_else(|| {
                    vec![
                        "http://127.0.0.1:39271".to_string(),
                        "http://localhost:39271".to_string(),
                    ]
                }),
            default_environment: normalized_env("CHATOS_ENV")
                .unwrap_or_else(|| "local".to_string()),
        };
        Ok(config)
    }

    pub fn bind_addr(&self) -> SocketAddr {
        SocketAddr::new(self.host, self.port)
    }

    pub fn internal_mtls_bind_addr(&self) -> SocketAddr {
        SocketAddr::new(self.host, self.internal_mtls_port)
    }
}

fn required_path_env(key: &str) -> Result<PathBuf, String> {
    normalized_env(key)
        .map(PathBuf::from)
        .ok_or_else(|| format!("{key} is required"))
}

pub fn load_config_center_dotenv() {
    chatos_service_runtime::load_service_dotenv(Path::new(env!("CARGO_MANIFEST_DIR")));
}
