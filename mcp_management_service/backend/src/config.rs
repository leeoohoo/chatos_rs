// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::time::Duration;

use chatos_service_runtime::{env_text, parse_bool_text, validate_production_secret};

const DEFAULT_RUNTIME_GRANT_SECRET: &str = "change_me_mcp_management_runtime_grant_secret";
const DEFAULT_RUNTIME_SESSION_ENCRYPTION_SECRET: &str =
    "change_me_mcp_management_runtime_session_encryption_secret";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsyncToolDispatchMode {
    LocalQueue,
    RabbitMq,
}

impl AsyncToolDispatchMode {
    fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "local" | "local_queue" | "in_memory" => Ok(Self::LocalQueue),
            "rabbitmq" | "rabbit_mq" | "amqp" => Ok(Self::RabbitMq),
            other => Err(format!(
                "MCP_MANAGEMENT_ASYNC_TOOL_DISPATCH_MODE is invalid: {other}"
            )),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LocalQueue => "local_queue",
            Self::RabbitMq => "rabbitmq",
        }
    }
}

#[derive(Debug, Clone)]
pub struct AsyncToolDispatchTopology {
    pub mode: AsyncToolDispatchMode,
    pub worker_concurrency: usize,
    pub local_queue_buffer: usize,
    pub rabbitmq_url: Option<String>,
    pub rabbitmq_exchange: Option<String>,
    pub queue_name: Option<String>,
}

impl AsyncToolDispatchTopology {
    fn from_env() -> Result<Self, String> {
        let mode = AsyncToolDispatchMode::parse(
            required_text("MCP_MANAGEMENT_ASYNC_TOOL_DISPATCH_MODE")?.as_str(),
        )?;
        let worker_concurrency = required_u64("MCP_MANAGEMENT_ASYNC_TOOL_WORKER_CONCURRENCY")
            .and_then(|value| {
                usize::try_from(value).map_err(|_| {
                    "MCP_MANAGEMENT_ASYNC_TOOL_WORKER_CONCURRENCY is too large".to_string()
                })
            })?;
        let local_queue_buffer = match mode {
            AsyncToolDispatchMode::LocalQueue => {
                required_u64("MCP_MANAGEMENT_ASYNC_TOOL_LOCAL_QUEUE_BUFFER")?
                    .try_into()
                    .map_err(|_| {
                        "MCP_MANAGEMENT_ASYNC_TOOL_LOCAL_QUEUE_BUFFER is too large".to_string()
                    })?
            }
            AsyncToolDispatchMode::RabbitMq => 0,
        };
        let rabbitmq_url = match mode {
            AsyncToolDispatchMode::RabbitMq => {
                Some(required_text("MCP_MANAGEMENT_ASYNC_TOOL_RABBITMQ_URL")?)
            }
            AsyncToolDispatchMode::LocalQueue => None,
        };
        let rabbitmq_exchange = match mode {
            AsyncToolDispatchMode::RabbitMq => Some(required_text(
                "MCP_MANAGEMENT_ASYNC_TOOL_RABBITMQ_EXCHANGE",
            )?),
            AsyncToolDispatchMode::LocalQueue => None,
        };
        let queue_name = match mode {
            AsyncToolDispatchMode::RabbitMq => {
                Some(required_text("MCP_MANAGEMENT_ASYNC_TOOL_DISPATCH_QUEUE")?)
            }
            AsyncToolDispatchMode::LocalQueue => None,
        };
        let topology = Self {
            mode,
            worker_concurrency,
            local_queue_buffer,
            rabbitmq_url,
            rabbitmq_exchange,
            queue_name,
        };
        topology.validate()?;
        Ok(topology)
    }

    fn validate(&self) -> Result<(), String> {
        if self.worker_concurrency == 0 {
            return Err(
                "MCP_MANAGEMENT_ASYNC_TOOL_WORKER_CONCURRENCY must be at least 1".to_string(),
            );
        }
        match self.mode {
            AsyncToolDispatchMode::LocalQueue => {
                if self.local_queue_buffer == 0 {
                    return Err(
                        "MCP_MANAGEMENT_ASYNC_TOOL_LOCAL_QUEUE_BUFFER must be at least 1"
                            .to_string(),
                    );
                }
            }
            AsyncToolDispatchMode::RabbitMq => {
                if self
                    .rabbitmq_url
                    .as_deref()
                    .map(str::trim)
                    .is_none_or(str::is_empty)
                {
                    return Err(
                        "MCP_MANAGEMENT_ASYNC_TOOL_RABBITMQ_URL is required for RabbitMQ dispatch"
                            .to_string(),
                    );
                }
                if self
                    .rabbitmq_exchange
                    .as_deref()
                    .map(str::trim)
                    .is_none_or(str::is_empty)
                {
                    return Err(
                        "MCP_MANAGEMENT_ASYNC_TOOL_RABBITMQ_EXCHANGE is required for RabbitMQ dispatch"
                            .to_string(),
                    );
                }
                if self
                    .queue_name
                    .as_deref()
                    .map(str::trim)
                    .is_none_or(str::is_empty)
                {
                    return Err(
                        "MCP_MANAGEMENT_ASYNC_TOOL_DISPATCH_QUEUE is required for RabbitMQ dispatch"
                            .to_string(),
                    );
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub host: IpAddr,
    pub port: u16,
    pub internal_api_secret: String,
    pub require_signed_internal_requests: bool,
    pub allowed_internal_callers: BTreeSet<String>,
    pub plugin_management_service_base_url: String,
    pub plugin_management_internal_api_secret: Option<String>,
    pub project_service_base_url: String,
    pub project_service_internal_api_secret: Option<String>,
    pub task_runner_service_base_url: String,
    pub task_runner_internal_api_secret: Option<String>,
    pub task_runner_request_timeout: Duration,
    pub task_runner_ask_user_request_timeout: Duration,
    pub chatos_service_base_url: String,
    pub chatos_internal_api_secret: Option<String>,
    pub chatos_ask_user_request_timeout: Duration,
    pub chatos_browser_request_timeout: Duration,
    pub local_connector_service_base_url: String,
    pub local_connector_internal_api_secret: Option<String>,
    pub sandbox_manager_service_base_url: String,
    pub sandbox_manager_internal_api_secret: Option<String>,
    pub sandbox_manager_request_timeout: Duration,
    pub sandbox_image_request_timeout: Duration,
    pub embedded_work_dir: PathBuf,
    pub downstream_request_timeout: Duration,
    pub external_http_request_timeout: Duration,
    pub provider_response_limit_bytes: usize,
    pub public_base_url: String,
    pub runtime_grant_secret: String,
    pub runtime_session_database_url: Option<String>,
    pub runtime_session_encryption_secret: String,
    pub runtime_session_ttl: Duration,
    pub async_tool_dispatch_topology: AsyncToolDispatchTopology,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, String> {
        let host = env_text("MCP_MANAGEMENT_HOST")
            .and_then(|value| value.parse::<IpAddr>().ok())
            .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
        let port = env_text("MCP_MANAGEMENT_PORT")
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(39280);
        let internal_api_secret = required_text("MCP_MANAGEMENT_INTERNAL_API_SECRET")?;
        validate_production_secret(
            "MCP_MANAGEMENT_INTERNAL_API_SECRET",
            Some(internal_api_secret.as_str()),
            &["change_me_mcp_management_internal_secret"],
        )?;
        let require_signed_internal_requests =
            required_bool("MCP_MANAGEMENT_REQUIRE_SIGNED_INTERNAL_REQUESTS")?;
        let allowed_internal_callers =
            parse_callers(required_text("MCP_MANAGEMENT_ALLOWED_INTERNAL_CALLERS")?.as_str());
        if allowed_internal_callers.is_empty() {
            return Err("MCP_MANAGEMENT_ALLOWED_INTERNAL_CALLERS cannot be empty".to_string());
        }
        let runtime_grant_secret = required_text("MCP_MANAGEMENT_RUNTIME_GRANT_SECRET")?;
        validate_production_secret(
            "MCP_MANAGEMENT_RUNTIME_GRANT_SECRET",
            Some(runtime_grant_secret.as_str()),
            &[DEFAULT_RUNTIME_GRANT_SECRET],
        )?;
        let runtime_session_encryption_secret =
            required_text("MCP_MANAGEMENT_RUNTIME_SESSION_ENCRYPTION_SECRET")?;
        validate_production_secret(
            "MCP_MANAGEMENT_RUNTIME_SESSION_ENCRYPTION_SECRET",
            Some(runtime_session_encryption_secret.as_str()),
            &[DEFAULT_RUNTIME_SESSION_ENCRYPTION_SECRET],
        )?;
        let runtime_session_database_url = Some(
            env_text("MCP_MANAGEMENT_DATABASE_URL")
                .unwrap_or_else(|| "mongodb://127.0.0.1:27017/mcp_management_service".to_string()),
        );
        let plugin_management_internal_api_secret = Some(required_text(
            "PLUGIN_MANAGEMENT_MCP_MANAGEMENT_INTERNAL_API_SECRET",
        )?);
        let project_service_internal_api_secret = Some(required_text(
            "MCP_MANAGEMENT_PROJECT_SERVICE_INTERNAL_API_SECRET",
        )?);
        let task_runner_internal_api_secret = Some(required_text(
            "MCP_MANAGEMENT_TASK_RUNNER_INTERNAL_API_SECRET",
        )?);
        let chatos_internal_api_secret =
            Some(required_text("MCP_MANAGEMENT_CHATOS_INTERNAL_API_SECRET")?);
        let local_connector_internal_api_secret = Some(required_text(
            "MCP_MANAGEMENT_LOCAL_CONNECTOR_INTERNAL_API_SECRET",
        )?);
        let sandbox_manager_internal_api_secret = Some(required_text(
            "MCP_MANAGEMENT_SANDBOX_MANAGER_INTERNAL_API_SECRET",
        )?);
        validate_production_secret(
            "PLUGIN_MANAGEMENT_MCP_MANAGEMENT_INTERNAL_API_SECRET",
            plugin_management_internal_api_secret.as_deref(),
            &["change_me_plugin_management_mcp_management_secret"],
        )?;
        validate_production_secret(
            "MCP_MANAGEMENT_PROJECT_SERVICE_INTERNAL_API_SECRET",
            project_service_internal_api_secret.as_deref(),
            &["change_me_mcp_management_project_service_secret"],
        )?;
        validate_production_secret(
            "MCP_MANAGEMENT_TASK_RUNNER_INTERNAL_API_SECRET",
            task_runner_internal_api_secret.as_deref(),
            &["change_me_mcp_management_task_runner_secret"],
        )?;
        validate_production_secret(
            "MCP_MANAGEMENT_CHATOS_INTERNAL_API_SECRET",
            chatos_internal_api_secret.as_deref(),
            &["change_me_mcp_management_chatos_secret"],
        )?;
        validate_production_secret(
            "MCP_MANAGEMENT_LOCAL_CONNECTOR_INTERNAL_API_SECRET",
            local_connector_internal_api_secret.as_deref(),
            &["change_me_mcp_management_local_connector_secret"],
        )?;
        validate_production_secret(
            "MCP_MANAGEMENT_SANDBOX_MANAGER_INTERNAL_API_SECRET",
            sandbox_manager_internal_api_secret.as_deref(),
            &["change_me_mcp_management_sandbox_manager_secret"],
        )?;
        let downstream_request_timeout = Duration::from_millis(
            env_text("MCP_MANAGEMENT_DOWNSTREAM_REQUEST_TIMEOUT_MS")
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(5_000)
                .clamp(300, 60_000),
        );
        let external_http_request_timeout = Duration::from_millis(
            env_text("MCP_MANAGEMENT_EXTERNAL_HTTP_TOOL_TIMEOUT_MS")
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(60_000)
                .clamp(1_000, 10 * 60 * 1_000),
        );
        let runtime_session_ttl = Duration::from_secs(
            env_text("MCP_MANAGEMENT_RUNTIME_SESSION_TTL_SECONDS")
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(30 * 60)
                .clamp(5 * 60, 2 * 60 * 60),
        );
        let sandbox_manager_request_timeout = Duration::from_millis(
            env_text("MCP_MANAGEMENT_SANDBOX_TOOL_TIMEOUT_MS")
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(180_000)
                .clamp(1_000, 2 * 60 * 60 * 1_000),
        );
        let sandbox_image_request_timeout = Duration::from_millis(
            env_text("MCP_MANAGEMENT_SANDBOX_IMAGE_TOOL_TIMEOUT_MS")
                .or_else(|| env_text("SANDBOX_IMAGE_MCP_REQUEST_TIMEOUT_MS"))
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(2 * 60 * 60 * 1_000 + 30_000)
                .clamp(30_000, 3 * 60 * 60 * 1_000),
        );
        let task_runner_request_timeout = Duration::from_millis(
            env_text("MCP_MANAGEMENT_TASK_RUNNER_TOOL_TIMEOUT_MS")
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(180_000)
                .clamp(1_000, 2 * 60 * 60 * 1_000),
        );
        let task_runner_ask_user_request_timeout = Duration::from_millis(
            env_text("MCP_MANAGEMENT_TASK_RUNNER_ASK_USER_TOOL_TIMEOUT_MS")
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(chatos_mcp::ASK_USER_PROMPT_TIMEOUT_MS_DEFAULT + 5 * 60 * 1_000)
                .clamp(
                    chatos_mcp::ASK_USER_PROMPT_TIMEOUT_MS_DEFAULT,
                    7 * 24 * 60 * 60 * 1_000,
                ),
        );
        let chatos_ask_user_request_timeout = Duration::from_millis(
            env_text("MCP_MANAGEMENT_CHATOS_ASK_USER_TOOL_TIMEOUT_MS")
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(chatos_mcp::ASK_USER_PROMPT_TIMEOUT_MS_DEFAULT + 5 * 60 * 1_000)
                .clamp(
                    chatos_mcp::ASK_USER_PROMPT_TIMEOUT_MS_DEFAULT,
                    7 * 24 * 60 * 60 * 1_000,
                ),
        );
        let chatos_browser_request_timeout = Duration::from_millis(
            env_text("MCP_MANAGEMENT_CHATOS_BROWSER_TOOL_TIMEOUT_MS")
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(120_000)
                .clamp(30_000, 10 * 60 * 1_000),
        );
        let provider_response_limit_bytes =
            env_text("MCP_MANAGEMENT_PROVIDER_RESPONSE_LIMIT_BYTES")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(2 * 1024 * 1024)
                .clamp(64 * 1024, 16 * 1024 * 1024);
        let async_tool_dispatch_topology = AsyncToolDispatchTopology::from_env()?;
        let public_base_url = normalize_base_url(
            env_text("MCP_MANAGEMENT_PUBLIC_BASE_URL")
                .unwrap_or_else(|| format!("http://127.0.0.1:{port}")),
        );
        Ok(Self {
            host,
            port,
            internal_api_secret,
            require_signed_internal_requests,
            allowed_internal_callers,
            plugin_management_service_base_url: normalize_base_url(
                env_text("MCP_MANAGEMENT_PLUGIN_MANAGEMENT_SERVICE_BASE_URL")
                    .or_else(|| env_text("PLUGIN_MANAGEMENT_SERVICE_BASE_URL"))
                    .unwrap_or_else(|| "http://127.0.0.1:39260".to_string()),
            ),
            plugin_management_internal_api_secret,
            project_service_base_url: normalize_base_url(
                env_text("MCP_MANAGEMENT_PROJECT_SERVICE_BASE_URL")
                    .or_else(|| env_text("PROJECT_SERVICE_BASE_URL"))
                    .unwrap_or_else(|| "http://127.0.0.1:39210".to_string()),
            ),
            project_service_internal_api_secret,
            task_runner_service_base_url: normalize_base_url(
                env_text("MCP_MANAGEMENT_TASK_RUNNER_SERVICE_BASE_URL")
                    .or_else(|| env_text("TASK_RUNNER_SERVICE_BASE_URL"))
                    .or_else(|| env_text("TASK_RUNNER_BASE_URL"))
                    .unwrap_or_else(|| "http://127.0.0.1:39090".to_string()),
            ),
            task_runner_internal_api_secret,
            task_runner_request_timeout,
            task_runner_ask_user_request_timeout,
            chatos_service_base_url: normalize_base_url(
                env_text("MCP_MANAGEMENT_CHATOS_SERVICE_BASE_URL")
                    .or_else(|| env_text("CHATOS_BACKEND_BASE_URL"))
                    .unwrap_or_else(|| "http://127.0.0.1:3997".to_string()),
            ),
            chatos_internal_api_secret,
            chatos_ask_user_request_timeout,
            chatos_browser_request_timeout,
            local_connector_service_base_url: normalize_base_url(
                env_text("MCP_MANAGEMENT_LOCAL_CONNECTOR_SERVICE_BASE_URL")
                    .or_else(|| env_text("LOCAL_CONNECTOR_SERVICE_BASE_URL"))
                    .unwrap_or_else(|| "http://127.0.0.1:39230".to_string()),
            ),
            local_connector_internal_api_secret,
            sandbox_manager_service_base_url: normalize_base_url(
                env_text("MCP_MANAGEMENT_SANDBOX_MANAGER_SERVICE_BASE_URL")
                    .or_else(|| env_text("SANDBOX_MANAGER_SERVICE_BASE_URL"))
                    .unwrap_or_else(|| "http://127.0.0.1:8095".to_string()),
            ),
            sandbox_manager_internal_api_secret,
            sandbox_manager_request_timeout,
            sandbox_image_request_timeout,
            embedded_work_dir: env_text("MCP_MANAGEMENT_EMBEDDED_WORK_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| std::env::temp_dir().join("chatos-mcp-management")),
            downstream_request_timeout,
            external_http_request_timeout,
            provider_response_limit_bytes,
            public_base_url,
            runtime_grant_secret,
            runtime_session_database_url,
            runtime_session_encryption_secret,
            runtime_session_ttl,
            async_tool_dispatch_topology,
        })
    }

    pub fn bind_addr(&self) -> SocketAddr {
        SocketAddr::new(self.host, self.port)
    }

    pub async fn resolve_service_urls(&mut self) {
        self.plugin_management_service_base_url = chatos_service_runtime::resolve_service_base_url(
            "plugin-management-service",
            self.plugin_management_service_base_url.as_str(),
        )
        .await;
        self.project_service_base_url = chatos_service_runtime::resolve_service_base_url(
            "project-service",
            self.project_service_base_url.as_str(),
        )
        .await;
        self.task_runner_service_base_url = chatos_service_runtime::resolve_service_base_url(
            "task-runner",
            self.task_runner_service_base_url.as_str(),
        )
        .await;
        self.chatos_service_base_url = chatos_service_runtime::resolve_service_base_url(
            "chatos-backend",
            self.chatos_service_base_url.as_str(),
        )
        .await;
        self.local_connector_service_base_url = chatos_service_runtime::resolve_service_base_url(
            "local-connector-service",
            self.local_connector_service_base_url.as_str(),
        )
        .await;
        self.sandbox_manager_service_base_url = chatos_service_runtime::resolve_service_base_url(
            "sandbox-manager",
            self.sandbox_manager_service_base_url.as_str(),
        )
        .await;
    }

    #[cfg(test)]
    pub(crate) fn test() -> Self {
        Self {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 39280,
            internal_api_secret: "a-long-test-secret".to_string(),
            require_signed_internal_requests: true,
            allowed_internal_callers: BTreeSet::from([
                "chatos".to_string(),
                "task-runner".to_string(),
            ]),
            plugin_management_service_base_url: "http://127.0.0.1:39260".to_string(),
            plugin_management_internal_api_secret: Some(
                "a-long-plugin-management-secret".to_string(),
            ),
            project_service_base_url: "http://127.0.0.1:39210".to_string(),
            project_service_internal_api_secret: Some("a-long-project-service-secret".to_string()),
            task_runner_service_base_url: "http://127.0.0.1:39090".to_string(),
            task_runner_internal_api_secret: Some("a-long-task-runner-secret".to_string()),
            task_runner_request_timeout: Duration::from_secs(180),
            task_runner_ask_user_request_timeout: Duration::from_secs(86_700),
            chatos_service_base_url: "http://127.0.0.1:3997".to_string(),
            chatos_internal_api_secret: Some("a-long-chatos-secret".to_string()),
            chatos_ask_user_request_timeout: Duration::from_secs(86_700),
            chatos_browser_request_timeout: Duration::from_secs(120),
            local_connector_service_base_url: "http://127.0.0.1:39230".to_string(),
            local_connector_internal_api_secret: Some("a-long-local-connector-secret".to_string()),
            sandbox_manager_service_base_url: "http://127.0.0.1:8095".to_string(),
            sandbox_manager_internal_api_secret: Some("a-long-sandbox-manager-secret".to_string()),
            sandbox_manager_request_timeout: Duration::from_secs(180),
            sandbox_image_request_timeout: Duration::from_secs(2 * 60 * 60 + 30),
            embedded_work_dir: std::env::temp_dir().join("chatos-mcp-management-test"),
            downstream_request_timeout: Duration::from_secs(5),
            external_http_request_timeout: Duration::from_secs(60),
            provider_response_limit_bytes: 2 * 1024 * 1024,
            public_base_url: "http://127.0.0.1:39280".to_string(),
            runtime_grant_secret: "a-long-runtime-grant-secret".to_string(),
            runtime_session_database_url: None,
            runtime_session_encryption_secret: "a-long-runtime-session-encryption-secret"
                .to_string(),
            runtime_session_ttl: Duration::from_secs(30 * 60),
            async_tool_dispatch_topology: AsyncToolDispatchTopology {
                mode: AsyncToolDispatchMode::LocalQueue,
                worker_concurrency: 4,
                local_queue_buffer: 64,
                rabbitmq_url: None,
                rabbitmq_exchange: None,
                queue_name: None,
            },
        }
    }
}

fn normalize_base_url(value: String) -> String {
    value.trim().trim_end_matches('/').to_string()
}

fn parse_callers(value: &str) -> BTreeSet<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn required_text(key: &str) -> Result<String, String> {
    env_text(key).ok_or_else(|| format!("{key} is required from config center"))
}

fn required_u64(key: &str) -> Result<u64, String> {
    let value = required_text(key)?;
    value
        .parse::<u64>()
        .map_err(|_| format!("{key} must be an unsigned integer"))
}

fn required_bool(key: &str) -> Result<bool, String> {
    let value = required_text(key)?;
    parse_bool_text(value.as_str()).ok_or_else(|| format!("invalid {key}: expected true/false"))
}

pub fn load_mcp_management_dotenv() {
    chatos_service_runtime::load_service_dotenv(Path::new(env!("CARGO_MANIFEST_DIR")));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caller_list_is_trimmed_and_deduplicated() {
        assert_eq!(
            parse_callers("chatos, task-runner,chatos"),
            BTreeSet::from(["chatos".to_string(), "task-runner".to_string()])
        );
    }

    #[test]
    fn async_tool_dispatch_mode_parser_rejects_unknown_values() {
        assert!(AsyncToolDispatchMode::parse("rabbitmq").is_ok());
        assert!(AsyncToolDispatchMode::parse("local").is_ok());
        assert!(AsyncToolDispatchMode::parse("mystery").is_err());
    }

    #[test]
    fn rabbitmq_async_dispatch_requires_explicit_topology() {
        let topology = AsyncToolDispatchTopology {
            mode: AsyncToolDispatchMode::RabbitMq,
            worker_concurrency: 2,
            local_queue_buffer: 0,
            rabbitmq_url: None,
            rabbitmq_exchange: Some("mcp_management".to_string()),
            queue_name: Some("mcp_management.async.dispatch".to_string()),
        };
        assert!(topology.validate().is_err());
    }
}
