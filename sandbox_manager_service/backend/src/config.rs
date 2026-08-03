// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub(crate) use chatos_service_runtime::env_text as normalized_env;
use chatos_service_runtime::{
    env_flag as env_bool, env_parse, parse_bool_text, validate_production_secret,
    DEFAULT_SANDBOX_MANAGER_AGENT_TOKEN_SECRET, DEFAULT_SANDBOX_MANAGER_OPERATOR_TOKEN,
    DEFAULT_SANDBOX_MANAGER_SYSTEM_CLIENT_KEY,
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
    pub operator_token: Option<String>,
    pub user_service_base_url: String,
    pub user_service_request_timeout_ms: u64,
    pub system_client_id: Option<String>,
    pub system_client_key: Option<String>,
    pub system_client_scopes: Vec<String>,
    pub system_client_allowed_tenant_ids: Vec<String>,
    pub system_client_allowed_project_ids: Vec<String>,
    pub system_client_allowed_tools: Vec<String>,
    pub system_client_max_lease_ttl_seconds: u64,
    pub internal_api_secrets: HashMap<String, String>,
    pub require_signed_internal_requests: bool,
    pub agent_token_secret: String,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, String> {
        let host =
            env_parse("SANDBOX_MANAGER_HOST").unwrap_or(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
        let port = env_parse("SANDBOX_MANAGER_PORT").unwrap_or(8095);
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
        let lease_ttl_seconds = env_parse("SANDBOX_MANAGER_LEASE_TTL_SECONDS").unwrap_or(7_200);
        let cleanup_interval_seconds =
            env_parse("SANDBOX_MANAGER_CLEANUP_INTERVAL_SECONDS").unwrap_or(30);
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
            env_parse("SANDBOX_MANAGER_SYSTEM_CLIENT_MAX_LEASE_TTL_SECONDS")
                .unwrap_or(lease_ttl_seconds)
                .max(60);

        let config = Self {
            host,
            port,
            database_url: normalized_env("SANDBOX_MANAGER_DATABASE_URL")
                .unwrap_or_else(default_database_url),
            mongodb_database: normalized_env("SANDBOX_MANAGER_MONGODB_DATABASE")
                .unwrap_or_else(|| "sandbox_manager_service".to_string()),
            backend,
            work_root: normalized_env("SANDBOX_MANAGER_WORK_ROOT")
                .map(PathBuf::from)
                .unwrap_or_else(default_work_root),
            pool_max_active: env_parse("SANDBOX_MANAGER_POOL_MAX_ACTIVE").unwrap_or(5),
            pool_max_pending: env_parse("SANDBOX_MANAGER_POOL_MAX_PENDING").unwrap_or(50),
            lease_ttl,
            cleanup_interval: Duration::from_secs(cleanup_interval_seconds.max(5)),
            agent_port: env_parse("SANDBOX_MANAGER_AGENT_PORT").unwrap_or(49_888),
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
            docker_maintenance_enabled: env_bool(
                "SANDBOX_MANAGER_DOCKER_MAINTENANCE_ENABLED",
                true,
            ),
            docker_build_cache_max_used_space: storage_limit_env(
                "SANDBOX_MANAGER_DOCKER_BUILD_CACHE_MAX_USED_SPACE",
                "32gb",
            ),
            docker_build_cache_reserved_space: storage_limit_env(
                "SANDBOX_MANAGER_DOCKER_BUILD_CACHE_RESERVED_SPACE",
                "8gb",
            ),
            docker_build_cache_timeout: Duration::from_secs(
                env_parse::<u64>("SANDBOX_MANAGER_DOCKER_BUILD_CACHE_TIMEOUT_SECS")
                    .unwrap_or(180)
                    .max(30),
            ),
            require_auth: env_bool("SANDBOX_MANAGER_REQUIRE_AUTH", true),
            operator_token: Some(required_text("SANDBOX_MANAGER_OPERATOR_TOKEN")?),
            user_service_base_url: normalized_env("SANDBOX_MANAGER_USER_SERVICE_BASE_URL")
                .or_else(|| normalized_env("CHATOS_USER_SERVICE_BASE_URL"))
                .or_else(|| normalized_env("USER_SERVICE_BASE_URL"))
                .unwrap_or_else(|| "http://127.0.0.1:39190".to_string()),
            user_service_request_timeout_ms: env_parse(
                "SANDBOX_MANAGER_USER_SERVICE_REQUEST_TIMEOUT_MS",
            )
            .or_else(|| env_parse("CHATOS_USER_SERVICE_REQUEST_TIMEOUT_MS"))
            .or_else(|| env_parse("USER_SERVICE_DOWNSTREAM_REQUEST_TIMEOUT_MS"))
            .unwrap_or(5_000)
            .max(300),
            system_client_id: Some(required_text("SANDBOX_MANAGER_SYSTEM_CLIENT_ID")?),
            system_client_key: Some(required_text("SANDBOX_MANAGER_SYSTEM_CLIENT_KEY")?),
            system_client_scopes: env_csv(
                "SANDBOX_MANAGER_SYSTEM_CLIENT_SCOPES",
                &[
                    "sandbox.lease.create",
                    "sandbox.lease.read",
                    "sandbox.lease.release",
                    "sandbox.mcp.tools",
                    "sandbox.mcp.call",
                    "sandbox.pool.read",
                    "sandbox.images.read",
                ],
            ),
            system_client_allowed_tenant_ids: env_csv(
                "SANDBOX_MANAGER_SYSTEM_CLIENT_ALLOWED_TENANT_IDS",
                &["*"],
            ),
            system_client_allowed_project_ids: env_csv(
                "SANDBOX_MANAGER_SYSTEM_CLIENT_ALLOWED_PROJECT_IDS",
                &["*"],
            ),
            system_client_allowed_tools: env_csv(
                "SANDBOX_MANAGER_SYSTEM_CLIENT_ALLOWED_TOOLS",
                &["*"],
            ),
            system_client_max_lease_ttl_seconds,
            internal_api_secrets: caller_internal_api_secrets(),
            require_signed_internal_requests: required_managed_bool(
                "SANDBOX_MANAGER_REQUIRE_SIGNED_INTERNAL_REQUESTS",
            )?,
            agent_token_secret: required_text("SANDBOX_MANAGER_AGENT_TOKEN_SECRET")?,
        };

        if config.require_auth {
            if config.operator_token.is_some() {
                validate_production_secret(
                    "SANDBOX_MANAGER_OPERATOR_TOKEN",
                    config.operator_token.as_deref(),
                    &[DEFAULT_SANDBOX_MANAGER_OPERATOR_TOKEN],
                )?;
            }
            if config.system_client_key.is_some() {
                validate_production_secret(
                    "SANDBOX_MANAGER_SYSTEM_CLIENT_KEY",
                    config.system_client_key.as_deref(),
                    &[DEFAULT_SANDBOX_MANAGER_SYSTEM_CLIENT_KEY],
                )?;
            }
            if config.require_signed_internal_requests {
                for caller in ["task-runner", "project-service", "mcp-management-service"] {
                    if !config.internal_api_secrets.contains_key(caller) {
                        return Err(format!(
                            "dedicated Sandbox Manager internal secret is required for {caller}"
                        ));
                    }
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
        }

        Ok(config)
    }

    pub fn bind_addr(&self) -> SocketAddr {
        SocketAddr::new(self.host, self.port)
    }
}

fn caller_internal_api_secrets() -> HashMap<String, String> {
    [
        (
            "task-runner",
            "TASK_RUNNER_SANDBOX_MANAGER_INTERNAL_API_SECRET",
        ),
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

fn env_csv(key: &str, default_values: &[&str]) -> Vec<String> {
    normalized_env(key)
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .filter(|values| !values.is_empty())
        .unwrap_or_else(|| {
            default_values
                .iter()
                .map(|value| value.to_string())
                .collect()
        })
}

fn required_text(key: &str) -> Result<String, String> {
    normalized_env(key).ok_or_else(|| format!("{key} is required from configuration center"))
}

fn storage_limit_env(key: &str, default_value: &str) -> String {
    normalized_env(key)
        .filter(|value| valid_storage_limit(value.as_str()))
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_else(|| default_value.to_string())
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

fn default_database_url() -> String {
    let host = normalized_env("SANDBOX_MANAGER_MONGODB_HOST")
        .or_else(|| normalized_env("DEV_MONGO_HOST"))
        .or_else(|| normalized_env("MONGODB_HOST"))
        .map(|value| match value.as_str() {
            "0.0.0.0" | "::" | "[::]" => "127.0.0.1".to_string(),
            _ => value,
        })
        .unwrap_or_else(|| "127.0.0.1".to_string());
    let port = normalized_env("SANDBOX_MANAGER_MONGODB_PORT")
        .or_else(|| normalized_env("DEV_MONGO_PORT"))
        .or_else(|| normalized_env("MONGODB_PORT"))
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(27018);
    let user = normalized_env("SANDBOX_MANAGER_MONGODB_USER")
        .or_else(|| normalized_env("MONGODB_USER"))
        .unwrap_or_else(|| "admin".to_string());
    let password = normalized_env("SANDBOX_MANAGER_MONGODB_PASSWORD")
        .or_else(|| normalized_env("MONGODB_PASSWORD"))
        .unwrap_or_else(|| "admin".to_string());
    let auth_source = normalized_env("SANDBOX_MANAGER_MONGODB_AUTH_SOURCE")
        .or_else(|| normalized_env("MONGODB_AUTH_SOURCE"))
        .unwrap_or_else(|| "admin".to_string());
    let database = normalized_env("SANDBOX_MANAGER_MONGODB_DATABASE")
        .unwrap_or_else(|| "sandbox_manager_service".to_string());
    format!("mongodb://{user}:{password}@{host}:{port}/{database}?authSource={auth_source}")
}

fn default_work_root() -> PathBuf {
    PathBuf::from(".chatos").join("sandboxes")
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
