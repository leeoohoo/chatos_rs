// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub(crate) use chatos_service_runtime::env_text as normalized_env;
use chatos_service_runtime::{
    env_flag as env_bool, parse_bool_text, validate_production_secret,
    DEFAULT_SANDBOX_MANAGER_AGENT_TOKEN_SECRET,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxBackendKind {
    Mock,
    Docker,
    Kata,
}

impl SandboxBackendKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mock => "mock",
            Self::Docker => "docker",
            Self::Kata => "kata",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockerAgentEndpointMode {
    Published,
    Container,
}

impl DockerAgentEndpointMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Published => "published",
            Self::Container => "container",
        }
    }
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub host: IpAddr,
    pub port: u16,
    pub database_url: String,
    pub mongodb_database: String,
    pub backend: SandboxBackendKind,
    pub work_root: PathBuf,
    pub pool_max_active: usize,
    pub pool_max_pending: usize,
    pub lease_ttl: Duration,
    pub cleanup_interval: Duration,
    pub agent_port: u16,
    pub docker_image: String,
    pub docker_network_mode: String,
    pub docker_agent_endpoint_mode: DockerAgentEndpointMode,
    pub docker_agent_publish: bool,
    pub docker_agent_bind_host: String,
    pub docker_agent_connect_host: String,
    pub docker_config: Option<PathBuf>,
    pub docker_host: Option<String>,
    pub docker_work_volume: Option<String>,
    pub runtime_http_proxy: Option<String>,
    pub runtime_https_proxy: Option<String>,
    pub runtime_no_proxy: Option<String>,
    pub kata_container_cli: String,
    pub kata_runtime: String,
    pub kata_image: String,
    pub kata_network_mode: String,
    pub image_tag_prefix: String,
    pub image_build_context: PathBuf,
    pub image_dockerfile: PathBuf,
    pub docker_maintenance_enabled: bool,
    pub docker_build_cache_max_used_space: String,
    pub docker_build_cache_reserved_space: String,
    pub docker_build_cache_timeout: Duration,
    pub require_auth: bool,
    pub user_service_base_url: String,
    pub user_service_request_timeout_ms: u64,
    pub system_client_max_lease_ttl_seconds: u64,
    pub internal_api_secrets: HashMap<String, String>,
    pub require_signed_internal_requests: bool,
    pub agent_token_secret: String,
    pub frontend_proxy_client_id: String,
    pub frontend_proxy_client_key: String,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, String> {
        let host = required_text("SANDBOX_MANAGER_HOST")?
            .parse::<IpAddr>()
            .map_err(|err| format!("SANDBOX_MANAGER_HOST must be a valid ip address: {err}"))?;
        let port = required_u16("SANDBOX_MANAGER_PORT")?;
        let backend = match normalized_env("SANDBOX_MANAGER_BACKEND")
            .unwrap_or_else(|| "auto".to_string())
            .to_ascii_lowercase()
            .as_str()
        {
            "auto" => default_backend_for_current_os(),
            "kata" => SandboxBackendKind::Kata,
            "docker" => SandboxBackendKind::Docker,
            _ => SandboxBackendKind::Mock,
        };
        let lease_ttl_seconds = required_u64("SANDBOX_MANAGER_LEASE_TTL_SECONDS")?.max(60);
        let cleanup_interval_seconds =
            required_u64("SANDBOX_MANAGER_CLEANUP_INTERVAL_SECONDS")?.max(5);
        let docker_image = normalized_env("SANDBOX_MANAGER_DOCKER_IMAGE")
            .unwrap_or_else(|| "chatos-sandbox-agent:latest".to_string());
        let docker_agent_endpoint_mode =
            match normalized_env("SANDBOX_MANAGER_DOCKER_AGENT_ENDPOINT_MODE")
                .unwrap_or_else(|| "published".to_string())
                .to_ascii_lowercase()
                .as_str()
            {
                "container" | "container_name" | "network" => DockerAgentEndpointMode::Container,
                _ => DockerAgentEndpointMode::Published,
            };
        let image_build_context = normalized_env("SANDBOX_MANAGER_IMAGE_BUILD_CONTEXT")
            .map(PathBuf::from)
            .unwrap_or_else(default_image_build_context);
        let image_dockerfile = normalized_env("SANDBOX_MANAGER_IMAGE_DOCKERFILE")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                image_build_context
                    .join("sandbox_manager_service")
                    .join("sandbox_agent")
                    .join("Dockerfile")
            });

        let lease_ttl = Duration::from_secs(lease_ttl_seconds);
        let system_client_max_lease_ttl_seconds =
            required_u64("SANDBOX_MANAGER_SYSTEM_CLIENT_MAX_LEASE_TTL_SECONDS")?.max(60);
        let work_root = PathBuf::from(required_text("SANDBOX_MANAGER_WORK_ROOT")?);
        if !work_root.is_absolute() {
            return Err("SANDBOX_MANAGER_WORK_ROOT must be an absolute path".to_string());
        }

        let config = Self {
            host,
            port,
            database_url: required_text("SANDBOX_MANAGER_DATABASE_URL")?,
            mongodb_database: required_text("SANDBOX_MANAGER_MONGODB_DATABASE")?,
            backend,
            work_root,
            pool_max_active: required_usize("SANDBOX_MANAGER_POOL_MAX_ACTIVE")?,
            pool_max_pending: required_usize("SANDBOX_MANAGER_POOL_MAX_PENDING")?,
            lease_ttl,
            cleanup_interval: Duration::from_secs(cleanup_interval_seconds.max(5)),
            agent_port: required_u16("SANDBOX_MANAGER_AGENT_PORT")?,
            docker_image: docker_image.clone(),
            docker_network_mode: normalized_env("SANDBOX_MANAGER_DOCKER_NETWORK")
                .unwrap_or_else(|| "bridge".to_string()),
            docker_agent_endpoint_mode,
            docker_agent_publish: env_bool("SANDBOX_MANAGER_DOCKER_PUBLISH_AGENT", true),
            docker_agent_bind_host: normalized_env("SANDBOX_MANAGER_DOCKER_AGENT_BIND_HOST")
                .unwrap_or_else(|| "127.0.0.1".to_string()),
            docker_agent_connect_host: normalized_env("SANDBOX_MANAGER_DOCKER_AGENT_CONNECT_HOST")
                .or_else(|| normalized_env("SANDBOX_MANAGER_DOCKER_AGENT_HOST"))
                .unwrap_or_else(|| "127.0.0.1".to_string()),
            docker_config: normalized_env("SANDBOX_MANAGER_DOCKER_CONFIG").map(PathBuf::from),
            docker_host: normalized_env("SANDBOX_MANAGER_DOCKER_HOST"),
            docker_work_volume: normalized_env("SANDBOX_MANAGER_DOCKER_WORK_VOLUME"),
            runtime_http_proxy: normalized_env("SANDBOX_MANAGER_RUNTIME_HTTP_PROXY"),
            runtime_https_proxy: normalized_env("SANDBOX_MANAGER_RUNTIME_HTTPS_PROXY"),
            runtime_no_proxy: normalized_env("SANDBOX_MANAGER_RUNTIME_NO_PROXY"),
            kata_container_cli: normalized_env("SANDBOX_MANAGER_KATA_CONTAINER_CLI")
                .unwrap_or_else(|| "nerdctl".to_string()),
            kata_runtime: normalized_env("SANDBOX_MANAGER_KATA_RUNTIME")
                .unwrap_or_else(|| "io.containerd.kata.v2".to_string()),
            kata_image: normalized_env("SANDBOX_MANAGER_KATA_IMAGE").unwrap_or(docker_image),
            kata_network_mode: normalized_env("SANDBOX_MANAGER_KATA_NETWORK")
                .unwrap_or_else(|| "bridge".to_string()),
            image_tag_prefix: normalized_env("SANDBOX_MANAGER_IMAGE_TAG_PREFIX")
                .unwrap_or_else(|| "chatos-sandbox-agent".to_string()),
            image_build_context,
            image_dockerfile,
            docker_maintenance_enabled: required_managed_bool(
                "SANDBOX_MANAGER_DOCKER_MAINTENANCE_ENABLED",
            )?,
            docker_build_cache_max_used_space: required_storage_limit_text(
                "SANDBOX_MANAGER_DOCKER_BUILD_CACHE_MAX_USED_SPACE",
            )?,
            docker_build_cache_reserved_space: required_storage_limit_text(
                "SANDBOX_MANAGER_DOCKER_BUILD_CACHE_RESERVED_SPACE",
            )?,
            docker_build_cache_timeout: Duration::from_secs(
                required_u64("SANDBOX_MANAGER_DOCKER_BUILD_CACHE_TIMEOUT_SECS")?.max(30),
            ),
            require_auth: required_managed_bool("SANDBOX_MANAGER_REQUIRE_AUTH")?,
            user_service_base_url: required_text("SANDBOX_MANAGER_USER_SERVICE_BASE_URL")?,
            user_service_request_timeout_ms: required_u64(
                "SANDBOX_MANAGER_USER_SERVICE_REQUEST_TIMEOUT_MS",
            )?
            .max(300),
            system_client_max_lease_ttl_seconds,
            internal_api_secrets: caller_internal_api_secrets(),
            require_signed_internal_requests: required_managed_bool(
                "SANDBOX_MANAGER_REQUIRE_SIGNED_INTERNAL_REQUESTS",
            )?,
            agent_token_secret: required_text("SANDBOX_MANAGER_AGENT_TOKEN_SECRET")?,
            frontend_proxy_client_id: required_text("SANDBOX_MANAGER_FRONTEND_PROXY_CLIENT_ID")?,
            frontend_proxy_client_key: required_text("SANDBOX_MANAGER_FRONTEND_PROXY_CLIENT_KEY")?,
        };

        if !config.require_auth {
            return Err("SANDBOX_MANAGER_REQUIRE_AUTH must be true".to_string());
        }
        if !config.require_signed_internal_requests {
            return Err(
                "SANDBOX_MANAGER_REQUIRE_SIGNED_INTERNAL_REQUESTS must be true".to_string(),
            );
        }
        for caller in ["project-service", "mcp-management-service"] {
            if !config.internal_api_secrets.contains_key(caller) {
                return Err(format!(
                    "dedicated Sandbox Manager internal secret is required for {caller}"
                ));
            }
        }
        for (caller, secret) in &config.internal_api_secrets {
            validate_production_secret(
                format!("Sandbox Manager internal secret for {caller}").as_str(),
                Some(secret.as_str()),
                &[
                    "change_me_task_runner_sandbox_manager_secret",
                    "change_me_project_service_sandbox_manager_secret",
                    "change_me_mcp_management_sandbox_manager_secret",
                ],
            )?;
        }
        validate_production_secret(
            "SANDBOX_MANAGER_AGENT_TOKEN_SECRET",
            Some(config.agent_token_secret.as_str()),
            &[DEFAULT_SANDBOX_MANAGER_AGENT_TOKEN_SECRET],
        )?;
        validate_production_secret(
            "SANDBOX_MANAGER_FRONTEND_PROXY_CLIENT_KEY",
            Some(config.frontend_proxy_client_key.as_str()),
            &["change_me_sandbox_manager_frontend_proxy_client_key"],
        )?;

        Ok(config)
    }

    pub fn bind_addr(&self) -> SocketAddr {
        SocketAddr::new(self.host, self.port)
    }
}

#[cfg(test)]
impl AppConfig {
    pub(crate) fn for_tests() -> Self {
        let image_build_context = default_image_build_context();
        Self {
            host: "127.0.0.1".parse().expect("test host"),
            port: 8095,
            database_url: "mongodb://127.0.0.1/sandbox_manager_test".to_string(),
            mongodb_database: "sandbox_manager_test".to_string(),
            backend: SandboxBackendKind::Mock,
            work_root: std::env::temp_dir().join("chatos-sandbox-manager-tests"),
            pool_max_active: 8,
            pool_max_pending: 80,
            lease_ttl: Duration::from_secs(7_200),
            cleanup_interval: Duration::from_secs(45),
            agent_port: 49_888,
            docker_image: "chatos-sandbox-agent:test".to_string(),
            docker_network_mode: "bridge".to_string(),
            docker_agent_endpoint_mode: DockerAgentEndpointMode::Published,
            docker_agent_publish: false,
            docker_agent_bind_host: "127.0.0.1".to_string(),
            docker_agent_connect_host: "127.0.0.1".to_string(),
            docker_config: None,
            docker_host: None,
            docker_work_volume: None,
            runtime_http_proxy: None,
            runtime_https_proxy: None,
            runtime_no_proxy: None,
            kata_container_cli: "nerdctl".to_string(),
            kata_runtime: "io.containerd.kata.v2".to_string(),
            kata_image: "chatos-sandbox-agent:test".to_string(),
            kata_network_mode: "bridge".to_string(),
            image_tag_prefix: "chatos-sandbox-agent".to_string(),
            image_dockerfile: image_build_context
                .join("sandbox_manager_service")
                .join("sandbox_agent")
                .join("Dockerfile"),
            image_build_context,
            docker_maintenance_enabled: true,
            docker_build_cache_max_used_space: "32gb".to_string(),
            docker_build_cache_reserved_space: "8gb".to_string(),
            docker_build_cache_timeout: Duration::from_secs(180),
            require_auth: true,
            user_service_base_url: "http://127.0.0.1:39190".to_string(),
            user_service_request_timeout_ms: 5_000,
            system_client_max_lease_ttl_seconds: 7_200,
            internal_api_secrets: HashMap::from([
                (
                    "project-service".to_string(),
                    "test-project-service-sandbox-manager-secret".to_string(),
                ),
                (
                    "mcp-management-service".to_string(),
                    "test-mcp-management-sandbox-manager-secret".to_string(),
                ),
            ]),
            require_signed_internal_requests: true,
            agent_token_secret: "test-sandbox-agent-token-secret".to_string(),
            frontend_proxy_client_id: "sandbox-manager-frontend-test".to_string(),
            frontend_proxy_client_key: "test-sandbox-manager-frontend-key".to_string(),
        }
    }
}

fn caller_internal_api_secrets() -> HashMap<String, String> {
    [
        (
            "project-service",
            "PROJECT_SERVICE_SANDBOX_MANAGER_INTERNAL_API_SECRET",
        ),
        (
            "mcp-management-service",
            "MCP_MANAGEMENT_SANDBOX_MANAGER_INTERNAL_API_SECRET",
        ),
    ]
    .into_iter()
    .filter_map(|(caller, env_name)| {
        normalized_env(env_name).map(|secret| (caller.to_string(), secret))
    })
    .collect()
}

pub fn load_sandbox_manager_dotenv() {
    chatos_service_runtime::load_service_dotenv(Path::new(env!("CARGO_MANIFEST_DIR")));
}

fn required_text(key: &str) -> Result<String, String> {
    normalized_env(key).ok_or_else(|| format!("{key} is required from configuration center"))
}

fn required_u64(key: &str) -> Result<u64, String> {
    let value = required_text(key)?;
    value
        .parse::<u64>()
        .map_err(|_| format!("{key} must be an unsigned integer"))
}

fn required_u16(key: &str) -> Result<u16, String> {
    let value = required_text(key)?;
    value
        .parse::<u16>()
        .map_err(|_| format!("{key} must be an unsigned integer"))
}

fn required_usize(key: &str) -> Result<usize, String> {
    let value = required_u64(key)?;
    usize::try_from(value).map_err(|_| format!("{key} is too large"))
}

fn required_storage_limit_text(key: &str) -> Result<String, String> {
    let value = required_text(key)?;
    if !valid_storage_limit(value.as_str()) {
        return Err(format!("{key} must be a valid storage limit like 32gb"));
    }
    Ok(value.to_ascii_lowercase())
}

fn valid_storage_limit(value: &str) -> bool {
    let value = value.trim().to_ascii_lowercase();
    let digits_end = value
        .char_indices()
        .find_map(|(index, character)| (!character.is_ascii_digit()).then_some(index))
        .unwrap_or(value.len());
    let (digits, suffix) = value.split_at(digits_end);
    !digits.is_empty()
        && digits.parse::<u64>().is_ok_and(|value| value > 0)
        && matches!(suffix, "" | "b" | "kb" | "mb" | "gb" | "tb")
}

fn default_image_build_context() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn default_backend_for_current_os() -> SandboxBackendKind {
    match std::env::consts::OS {
        "linux" if command_exists("nerdctl") => SandboxBackendKind::Kata,
        "linux" if command_exists("docker") => SandboxBackendKind::Docker,
        "linux" => SandboxBackendKind::Kata,
        "macos" | "windows" => SandboxBackendKind::Docker,
        _ => SandboxBackendKind::Docker,
    }
}

fn command_exists(command: &str) -> bool {
    std::env::var_os("PATH")
        .and_then(|paths| {
            std::env::split_paths(&paths)
                .map(|path| path.join(command))
                .find(|candidate| candidate.is_file())
        })
        .is_some()
}

fn required_managed_bool(key: &str) -> Result<bool, String> {
    let value = normalized_env(key)
        .ok_or_else(|| format!("{key} is required from configuration center"))?;
    parse_bool_text(value.as_str()).ok_or_else(|| format!("invalid {key}: expected true/false"))
}

#[cfg(test)]
mod tests {
    use super::valid_storage_limit;

    #[test]
    fn docker_storage_limit_rejects_shell_or_ambiguous_values() {
        assert!(valid_storage_limit("32gb"));
        assert!(valid_storage_limit("8192MB"));
        assert!(!valid_storage_limit("0gb"));
        assert!(!valid_storage_limit("32 gb"));
        assert!(!valid_storage_limit("32gb; rm -rf /"));
    }
}
