// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub(crate) use chatos_service_runtime::env_text as normalized_env;
use chatos_service_runtime::{
    env_flag, is_production_environment, validate_production_secret,
    DEFAULT_MEMORY_ENGINE_OPERATOR_TOKEN,
};

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub host: IpAddr,
    pub port: u16,
    pub database_url: String,
    pub user_service_base_url: String,
    pub user_service_request_timeout: Duration,
    pub relay_request_timeout: Duration,
    pub sandbox_image_relay_request_timeout: Duration,
    pub public_base_url: Option<String>,
    pub legacy_internal_api_secret: Option<String>,
    pub internal_api_secrets: HashMap<String, String>,
    pub require_signed_internal_requests: bool,
    pub memory_engine_base_url: String,
    pub memory_engine_operator_token: Option<String>,
    pub memory_engine_request_timeout: Duration,
    pub require_device_connect_signature: bool,
    pub allow_device_connect_query_token: bool,
    pub device_connect_signature_max_skew: Duration,
    pub active_session_lease_ttl: Duration,
    pub managed_requirements_toml_path: Option<PathBuf>,
    pub managed_requirements_signing_key_path: Option<PathBuf>,
    pub managed_requirements_signing_key_id: Option<String>,
    pub managed_requirements_bundle_ttl: Duration,
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
        let sandbox_image_relay_timeout_ms =
            std::env::var("LOCAL_CONNECTOR_SANDBOX_IMAGE_RELAY_REQUEST_TIMEOUT_MS")
                .ok()
                .or_else(|| std::env::var("SANDBOX_IMAGE_MCP_REQUEST_TIMEOUT_MS").ok())
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(2 * 60 * 60 * 1_000)
                .max(10_000);
        let memory_timeout_ms = std::env::var("LOCAL_CONNECTOR_MEMORY_ENGINE_REQUEST_TIMEOUT_MS")
            .ok()
            .or_else(|| std::env::var("MEMORY_ENGINE_REQUEST_TIMEOUT_MS").ok())
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(30_000)
            .max(1_000);
        let signature_skew_seconds =
            normalized_env("LOCAL_CONNECTOR_DEVICE_SIGNATURE_MAX_SKEW_SECONDS")
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(300)
                .clamp(30, 3600);
        let active_session_lease_ttl_seconds =
            normalized_env("LOCAL_CONNECTOR_ACTIVE_SESSION_LEASE_TTL_SECONDS")
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(90)
                .clamp(30, 600);
        let managed_requirements_bundle_ttl_seconds =
            normalized_env("LOCAL_CONNECTOR_MANAGED_REQUIREMENTS_BUNDLE_TTL_SECONDS")
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(24 * 60 * 60)
                .clamp(300, 7 * 24 * 60 * 60);

        let config = Self {
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
            sandbox_image_relay_request_timeout: Duration::from_millis(
                sandbox_image_relay_timeout_ms,
            ),
            public_base_url: normalized_env("LOCAL_CONNECTOR_PUBLIC_BASE_URL"),
            legacy_internal_api_secret: normalized_env("LOCAL_CONNECTOR_INTERNAL_API_SECRET"),
            internal_api_secrets: caller_internal_api_secrets(),
            require_signed_internal_requests: env_flag(
                "LOCAL_CONNECTOR_REQUIRE_SIGNED_INTERNAL_REQUESTS",
                is_production_environment(),
            ),
            memory_engine_base_url: normalize_memory_engine_base_url(
                normalized_env("LOCAL_CONNECTOR_MEMORY_ENGINE_BASE_URL")
                    .or_else(|| normalized_env("MEMORY_ENGINE_BASE_URL"))
                    .unwrap_or_else(default_memory_engine_base_url),
            ),
            memory_engine_operator_token: normalized_env(
                "LOCAL_CONNECTOR_MEMORY_ENGINE_INTERNAL_API_SECRET",
            )
            .or_else(|| normalized_env("LOCAL_CONNECTOR_MEMORY_ENGINE_OPERATOR_TOKEN"))
            .or_else(|| normalized_env("MEMORY_ENGINE_OPERATOR_TOKEN")),
            memory_engine_request_timeout: Duration::from_millis(memory_timeout_ms),
            require_device_connect_signature: env_flag(
                "LOCAL_CONNECTOR_REQUIRE_DEVICE_CONNECT_SIGNATURE",
                true,
            ),
            allow_device_connect_query_token: env_flag(
                "LOCAL_CONNECTOR_ALLOW_DEVICE_CONNECT_QUERY_TOKEN",
                false,
            ),
            device_connect_signature_max_skew: Duration::from_secs(signature_skew_seconds),
            active_session_lease_ttl: Duration::from_secs(active_session_lease_ttl_seconds),
            managed_requirements_toml_path: normalized_env(
                "LOCAL_CONNECTOR_MANAGED_REQUIREMENTS_TOML_PATH",
            )
            .map(PathBuf::from),
            managed_requirements_signing_key_path: normalized_env(
                "LOCAL_CONNECTOR_MANAGED_REQUIREMENTS_SIGNING_KEY_PATH",
            )
            .map(PathBuf::from),
            managed_requirements_signing_key_id: normalized_env(
                "LOCAL_CONNECTOR_MANAGED_REQUIREMENTS_SIGNING_KEY_ID",
            ),
            managed_requirements_bundle_ttl: Duration::from_secs(
                managed_requirements_bundle_ttl_seconds,
            ),
        };

        if config.require_signed_internal_requests {
            for caller in [
                "chatos-backend",
                "task-runner",
                "project-service",
                "memory-engine",
            ] {
                if !config.internal_api_secrets.contains_key(caller) {
                    return Err(format!(
                        "dedicated Local Connector internal secret is required for {caller}"
                    ));
                }
            }
        }
        if config.legacy_internal_api_secret.is_some() {
            validate_production_secret(
                "LOCAL_CONNECTOR_INTERNAL_API_SECRET",
                config.legacy_internal_api_secret.as_deref(),
                &[
                    "chatos-local-connector-dev-secret",
                    "change_me_task_runner_internal_secret",
                    "change_me_chatos_local_connector_secret",
                    "change_me_task_runner_local_connector_secret",
                    "change_me_project_service_local_connector_secret",
                    "change_me_memory_engine_local_connector_secret",
                ],
            )?;
        }
        for (caller, secret) in &config.internal_api_secrets {
            validate_production_secret(
                format!("Local Connector internal secret for {caller}").as_str(),
                Some(secret.as_str()),
                &[
                    "chatos-local-connector-dev-secret",
                    "change_me_task_runner_internal_secret",
                    "change_me_chatos_local_connector_secret",
                    "change_me_task_runner_local_connector_secret",
                    "change_me_project_service_local_connector_secret",
                    "change_me_memory_engine_local_connector_secret",
                ],
            )?;
        }
        if is_production_environment() || config.memory_engine_operator_token.is_some() {
            validate_production_secret(
                "LOCAL_CONNECTOR_MEMORY_ENGINE_INTERNAL_API_SECRET",
                config.memory_engine_operator_token.as_deref(),
                &[
                    DEFAULT_MEMORY_ENGINE_OPERATOR_TOKEN,
                    "change_me_local_connector_memory_engine_secret",
                ],
            )?;
        }

        Ok(config)
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

fn caller_internal_api_secrets() -> HashMap<String, String> {
    [
        (
            "chatos-backend",
            "CHATOS_LOCAL_CONNECTOR_INTERNAL_API_SECRET",
        ),
        (
            "task-runner",
            "TASK_RUNNER_LOCAL_CONNECTOR_INTERNAL_API_SECRET",
        ),
        (
            "project-service",
            "PROJECT_SERVICE_LOCAL_CONNECTOR_INTERNAL_API_SECRET",
        ),
        (
            "memory-engine",
            "MEMORY_ENGINE_LOCAL_CONNECTOR_INTERNAL_API_SECRET",
        ),
    ]
    .into_iter()
    .filter_map(|(caller, env_name)| {
        normalized_env(env_name).map(|secret| (caller.to_string(), secret))
    })
    .collect()
}

pub fn load_local_connector_dotenv() {
    chatos_service_runtime::load_service_dotenv(Path::new(env!("CARGO_MANIFEST_DIR")));
}

fn default_database_url() -> String {
    "mongodb://admin:admin@127.0.0.1:27018/local_connector_service?authSource=admin".to_string()
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

fn normalize_memory_engine_base_url(mut base_url: String) -> String {
    while base_url.ends_with('/') {
        base_url.pop();
    }
    if base_url.ends_with("/api/memory-engine/v1") || base_url.contains("/api/memory-engine/") {
        base_url
    } else {
        format!("{base_url}/api/memory-engine/v1")
    }
}
