// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[cfg(test)]
use std::net::Ipv4Addr;

use chatos_service_runtime::{env_text, parse_bool_text, validate_production_secret};

const DEFAULT_RUNTIME_GRANT_SECRET: &str = "change_me_mcp_management_runtime_grant_secret";
const DEFAULT_RUNTIME_SESSION_ENCRYPTION_SECRET: &str =
    "change_me_mcp_management_runtime_session_encryption_secret";
const REQUIRED_INTERNAL_CALLERS: [&str; 4] = [
    "chatos",
    "task-runner",
    "project-service",
    "configuration-center",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsyncToolDispatchMode {
    #[cfg(test)]
    LocalQueue,
    RabbitMq,
}

impl AsyncToolDispatchMode {
    fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "rabbitmq" | "rabbit_mq" | "amqp" => Ok(Self::RabbitMq),
            other => Err(format!(
                "MCP_MANAGEMENT_ASYNC_TOOL_DISPATCH_MODE must be rabbitmq, got: {other}"
            )),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            #[cfg(test)]
            Self::LocalQueue => "local_queue",
            Self::RabbitMq => "rabbitmq",
        }
    }
}

#[derive(Debug, Clone)]
pub struct AsyncToolDispatchTopology {
    pub mode: AsyncToolDispatchMode,
    pub worker_concurrency: usize,
    pub queue_max_length: u32,
    pub queue_max_bytes: u64,
    pub rabbitmq_reconnect_delay: Duration,
    pub max_delivery_attempts: u32,
    pub retry_delay: Duration,
    pub rabbitmq_url: Option<String>,
    pub rabbitmq_exchange: Option<String>,
    pub cancellation_exchange: Option<String>,
    pub queue_name: Option<String>,
    pub retry_queue_name: Option<String>,
    pub dead_letter_queue_name: Option<String>,
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
        let max_delivery_attempts =
            required_u32("MCP_MANAGEMENT_ASYNC_TOOL_MAX_DELIVERY_ATTEMPTS")?;
        let queue_max_length = required_u32("MCP_MANAGEMENT_ASYNC_TOOL_QUEUE_MAX_LENGTH")?;
        let queue_max_bytes = required_u64("MCP_MANAGEMENT_ASYNC_TOOL_QUEUE_MAX_BYTES")?;
        let rabbitmq_reconnect_delay = Duration::from_millis(required_u64(
            "MCP_MANAGEMENT_ASYNC_TOOL_RABBITMQ_RECONNECT_MS",
        )?);
        let retry_delay =
            Duration::from_millis(required_u64("MCP_MANAGEMENT_ASYNC_TOOL_RETRY_DELAY_MS")?);
        let rabbitmq_url = match mode {
            AsyncToolDispatchMode::RabbitMq => {
                Some(required_text("MCP_MANAGEMENT_ASYNC_TOOL_RABBITMQ_URL")?)
            }
            #[cfg(test)]
            AsyncToolDispatchMode::LocalQueue => None,
        };
        let rabbitmq_exchange = match mode {
            AsyncToolDispatchMode::RabbitMq => Some(required_text(
                "MCP_MANAGEMENT_ASYNC_TOOL_RABBITMQ_EXCHANGE",
            )?),
            #[cfg(test)]
            AsyncToolDispatchMode::LocalQueue => None,
        };
        let cancellation_exchange = match mode {
            AsyncToolDispatchMode::RabbitMq => Some(required_text(
                "MCP_MANAGEMENT_INVOCATION_CANCELLATION_EXCHANGE",
            )?),
            #[cfg(test)]
            AsyncToolDispatchMode::LocalQueue => None,
        };
        let queue_name = match mode {
            AsyncToolDispatchMode::RabbitMq => {
                Some(required_text("MCP_MANAGEMENT_ASYNC_TOOL_DISPATCH_QUEUE")?)
            }
            #[cfg(test)]
            AsyncToolDispatchMode::LocalQueue => None,
        };
        let retry_queue_name = match mode {
            AsyncToolDispatchMode::RabbitMq => {
                Some(required_text("MCP_MANAGEMENT_ASYNC_TOOL_RETRY_QUEUE")?)
            }
            #[cfg(test)]
            AsyncToolDispatchMode::LocalQueue => None,
        };
        let dead_letter_queue_name = match mode {
            AsyncToolDispatchMode::RabbitMq => Some(required_text(
                "MCP_MANAGEMENT_ASYNC_TOOL_DEAD_LETTER_QUEUE",
            )?),
            #[cfg(test)]
            AsyncToolDispatchMode::LocalQueue => None,
        };
        let topology = Self {
            mode,
            worker_concurrency,
            queue_max_length,
            queue_max_bytes,
            rabbitmq_reconnect_delay,
            max_delivery_attempts,
            retry_delay,
            rabbitmq_url,
            rabbitmq_exchange,
            cancellation_exchange,
            queue_name,
            retry_queue_name,
            dead_letter_queue_name,
        };
        topology.validate()?;
        Ok(topology)
    }

    fn validate(&self) -> Result<(), String> {
        if !(1..=512).contains(&self.worker_concurrency) {
            return Err(
                "MCP_MANAGEMENT_ASYNC_TOOL_WORKER_CONCURRENCY must be between 1 and 512"
                    .to_string(),
            );
        }
        if !(1..=100).contains(&self.max_delivery_attempts) {
            return Err(
                "MCP_MANAGEMENT_ASYNC_TOOL_MAX_DELIVERY_ATTEMPTS must be between 1 and 100"
                    .to_string(),
            );
        }
        if self.queue_max_length == 0 {
            return Err(
                "MCP_MANAGEMENT_ASYNC_TOOL_QUEUE_MAX_LENGTH must be at least 1".to_string(),
            );
        }
        if !(1_024..=i64::MAX as u64).contains(&self.queue_max_bytes) {
            return Err(
                "MCP_MANAGEMENT_ASYNC_TOOL_QUEUE_MAX_BYTES must be between 1024 and i64::MAX"
                    .to_string(),
            );
        }
        if !(Duration::from_millis(100)..=Duration::from_secs(60))
            .contains(&self.rabbitmq_reconnect_delay)
        {
            return Err(
                "MCP_MANAGEMENT_ASYNC_TOOL_RABBITMQ_RECONNECT_MS must be between 100 and 60000"
                    .to_string(),
            );
        }
        if !(Duration::from_millis(100)..=Duration::from_secs(60 * 60)).contains(&self.retry_delay)
        {
            return Err(
                "MCP_MANAGEMENT_ASYNC_TOOL_RETRY_DELAY_MS must be between 100 and 3600000"
                    .to_string(),
            );
        }
        match self.mode {
            #[cfg(test)]
            AsyncToolDispatchMode::LocalQueue => {}
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
                    .cancellation_exchange
                    .as_deref()
                    .map(str::trim)
                    .is_none_or(str::is_empty)
                {
                    return Err(
                        "MCP_MANAGEMENT_INVOCATION_CANCELLATION_EXCHANGE is required for RabbitMQ dispatch"
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
                if self
                    .retry_queue_name
                    .as_deref()
                    .map(str::trim)
                    .is_none_or(str::is_empty)
                {
                    return Err(
                        "MCP_MANAGEMENT_ASYNC_TOOL_RETRY_QUEUE is required for RabbitMQ dispatch"
                            .to_string(),
                    );
                }
                if self
                    .dead_letter_queue_name
                    .as_deref()
                    .map(str::trim)
                    .is_none_or(str::is_empty)
                {
                    return Err(
                        "MCP_MANAGEMENT_ASYNC_TOOL_DEAD_LETTER_QUEUE is required for RabbitMQ dispatch"
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
    pub internal_mtls_port: u16,
    pub otlp_endpoint: String,
    pub otlp_trace_sample_ratio: f64,
    pub otlp_export_timeout: Duration,
    pub mtls_server_cert_path: PathBuf,
    pub mtls_server_key_path: PathBuf,
    pub mtls_client_ca_cert_path: PathBuf,
    pub internal_api_secrets: BTreeMap<String, String>,
    pub require_signed_internal_requests: bool,
    pub allowed_internal_callers: BTreeSet<String>,
    pub plugin_management_service_base_url: String,
    pub plugin_management_http_client: reqwest::Client,
    pub plugin_management_internal_api_secret: Option<String>,
    pub project_service_base_url: String,
    pub project_service_http_client: reqwest::Client,
    pub project_service_internal_api_secret: Option<String>,
    pub project_service_tool_timeout: Duration,
    pub task_runner_service_base_url: String,
    pub task_runner_mtls_ca_cert_path: PathBuf,
    pub task_runner_mtls_client_identity_path: PathBuf,
    pub task_runner_internal_api_secret: Option<String>,
    pub task_runner_request_timeout: Duration,
    pub task_runner_ask_user_request_timeout: Duration,
    pub chatos_service_base_url: String,
    pub chatos_http_client: reqwest::Client,
    pub chatos_internal_api_secret: Option<String>,
    pub chatos_ask_user_request_timeout: Duration,
    pub chatos_browser_request_timeout: Duration,
    pub local_connector_service_base_url: String,
    pub local_connector_http_client: reqwest::Client,
    pub local_connector_internal_api_secret: Option<String>,
    pub sandbox_manager_service_base_url: String,
    pub sandbox_manager_http_client: reqwest::Client,
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
        let host = required_text("MCP_MANAGEMENT_HOST")?
            .parse::<IpAddr>()
            .map_err(|err| format!("MCP_MANAGEMENT_HOST must be a valid IP address: {err}"))?;
        let port = required_u16("MCP_MANAGEMENT_PORT")?;
        let internal_mtls_port = required_u16("MCP_MANAGEMENT_INTERNAL_MTLS_PORT")?;
        if internal_mtls_port == port {
            return Err(
                "MCP_MANAGEMENT_INTERNAL_MTLS_PORT must differ from MCP_MANAGEMENT_PORT"
                    .to_string(),
            );
        }
        let otlp_endpoint = required_text("MCP_MANAGEMENT_OTEL_EXPORTER_OTLP_ENDPOINT")?;
        require_http_endpoint(
            "MCP_MANAGEMENT_OTEL_EXPORTER_OTLP_ENDPOINT",
            otlp_endpoint.as_str(),
        )?;
        let otlp_trace_sample_ratio = required_f64("MCP_MANAGEMENT_OTEL_TRACE_SAMPLE_RATIO")?;
        if !(0.0..=1.0).contains(&otlp_trace_sample_ratio) {
            return Err(
                "MCP_MANAGEMENT_OTEL_TRACE_SAMPLE_RATIO must be between 0 and 1".to_string(),
            );
        }
        let otlp_export_timeout_ms = required_u64("MCP_MANAGEMENT_OTEL_EXPORT_TIMEOUT_MS")?;
        if otlp_export_timeout_ms == 0 {
            return Err(
                "MCP_MANAGEMENT_OTEL_EXPORT_TIMEOUT_MS must be greater than zero".to_string(),
            );
        }
        let require_signed_internal_requests =
            required_bool("MCP_MANAGEMENT_REQUIRE_SIGNED_INTERNAL_REQUESTS")?;
        if !require_signed_internal_requests {
            return Err("MCP_MANAGEMENT_REQUIRE_SIGNED_INTERNAL_REQUESTS must be true".to_string());
        }
        let allowed_internal_callers =
            parse_callers(required_text("MCP_MANAGEMENT_ALLOWED_INTERNAL_CALLERS")?.as_str());
        if allowed_internal_callers.is_empty() {
            return Err("MCP_MANAGEMENT_ALLOWED_INTERNAL_CALLERS cannot be empty".to_string());
        }
        let required_internal_callers = REQUIRED_INTERNAL_CALLERS
            .into_iter()
            .map(ToOwned::to_owned)
            .collect::<BTreeSet<_>>();
        if allowed_internal_callers != required_internal_callers {
            return Err(format!(
                "MCP_MANAGEMENT_ALLOWED_INTERNAL_CALLERS must contain exactly: {}",
                REQUIRED_INTERNAL_CALLERS.join(",")
            ));
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
        let runtime_session_database_url = Some(required_text("MCP_MANAGEMENT_DATABASE_URL")?);
        let plugin_management_internal_api_secret = Some(required_text(
            "PLUGIN_MANAGEMENT_MCP_MANAGEMENT_INTERNAL_API_SECRET",
        )?);
        let project_service_caller_secret =
            required_text("MCP_MANAGEMENT_PROJECT_SERVICE_INTERNAL_API_SECRET")?;
        let project_service_internal_api_secret = Some(project_service_caller_secret.clone());
        let task_runner_caller_secret =
            required_text("MCP_MANAGEMENT_TASK_RUNNER_INTERNAL_API_SECRET")?;
        let task_runner_internal_api_secret = Some(task_runner_caller_secret.clone());
        let chatos_caller_secret = required_text("MCP_MANAGEMENT_CHATOS_INTERNAL_API_SECRET")?;
        let chatos_internal_api_secret = Some(chatos_caller_secret.clone());
        let configuration_center_caller_secret =
            required_text("MCP_MANAGEMENT_CONFIGURATION_CENTER_INTERNAL_API_SECRET")?;
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
        validate_production_secret(
            "MCP_MANAGEMENT_CONFIGURATION_CENTER_INTERNAL_API_SECRET",
            Some(configuration_center_caller_secret.as_str()),
            &["change_me_configuration_center_mcp_management_secret"],
        )?;
        let internal_api_secrets = BTreeMap::from([
            ("chatos".to_string(), chatos_caller_secret),
            ("task-runner".to_string(), task_runner_caller_secret),
            ("project-service".to_string(), project_service_caller_secret),
            (
                "configuration-center".to_string(),
                configuration_center_caller_secret,
            ),
        ]);
        let downstream_request_timeout = Duration::from_millis(
            required_u64("MCP_MANAGEMENT_DOWNSTREAM_REQUEST_TIMEOUT_MS")?.clamp(300, 60_000),
        );
        let project_service_tool_timeout = Duration::from_millis(
            required_u64("MCP_MANAGEMENT_PROJECT_SERVICE_TOOL_TIMEOUT_MS")?
                .clamp(1_000, 2 * 60 * 60 * 1_000),
        );
        let plugin_management_service_base_url = require_https_base_url(
            "MCP_MANAGEMENT_PLUGIN_MANAGEMENT_SERVICE_BASE_URL",
            normalize_base_url(required_text(
                "MCP_MANAGEMENT_PLUGIN_MANAGEMENT_SERVICE_BASE_URL",
            )?),
        )?;
        let plugin_management_http_client = chatos_service_runtime::build_mtls_http_client(
            chatos_service_runtime::HttpClientTimeouts::new(downstream_request_timeout),
            required_path("PLUGIN_MANAGEMENT_MTLS_CA_CERT_PATH")?.as_path(),
            required_path("PLUGIN_MANAGEMENT_MTLS_CLIENT_IDENTITY_PATH")?.as_path(),
        )?;
        let external_http_request_timeout = Duration::from_millis(
            required_u64("MCP_MANAGEMENT_EXTERNAL_HTTP_TOOL_TIMEOUT_MS")?
                .clamp(1_000, 10 * 60 * 1_000),
        );
        let runtime_session_ttl = Duration::from_secs(
            required_u64("MCP_MANAGEMENT_RUNTIME_SESSION_TTL_SECONDS")?.clamp(5 * 60, 2 * 60 * 60),
        );
        let sandbox_manager_request_timeout = Duration::from_millis(
            required_u64("MCP_MANAGEMENT_SANDBOX_TOOL_TIMEOUT_MS")?
                .clamp(1_000, 2 * 60 * 60 * 1_000),
        );
        let sandbox_image_request_timeout = Duration::from_millis(
            required_u64("MCP_MANAGEMENT_SANDBOX_IMAGE_TOOL_TIMEOUT_MS")?
                .clamp(30_000, 3 * 60 * 60 * 1_000),
        );
        let sandbox_manager_service_base_url = require_https_base_url(
            "MCP_MANAGEMENT_SANDBOX_MANAGER_SERVICE_BASE_URL",
            normalize_base_url(required_text(
                "MCP_MANAGEMENT_SANDBOX_MANAGER_SERVICE_BASE_URL",
            )?),
        )?;
        let sandbox_manager_http_client = chatos_service_runtime::build_mtls_http_client(
            chatos_service_runtime::HttpClientTimeouts::new(sandbox_image_request_timeout),
            required_path("SANDBOX_MANAGER_MTLS_CA_CERT_PATH")?.as_path(),
            required_path("SANDBOX_MANAGER_MTLS_CLIENT_IDENTITY_PATH")?.as_path(),
        )?;
        let task_runner_request_timeout = Duration::from_millis(
            required_u64("MCP_MANAGEMENT_TASK_RUNNER_TOOL_TIMEOUT_MS")?
                .clamp(1_000, 2 * 60 * 60 * 1_000),
        );
        let task_runner_ask_user_request_timeout = Duration::from_millis(
            required_u64("MCP_MANAGEMENT_TASK_RUNNER_ASK_USER_TOOL_TIMEOUT_MS")?.clamp(
                chatos_mcp::ASK_USER_PROMPT_TIMEOUT_MS_DEFAULT,
                7 * 24 * 60 * 60 * 1_000,
            ),
        );
        let chatos_ask_user_request_timeout = Duration::from_millis(
            required_u64("MCP_MANAGEMENT_CHATOS_ASK_USER_TOOL_TIMEOUT_MS")?.clamp(
                chatos_mcp::ASK_USER_PROMPT_TIMEOUT_MS_DEFAULT,
                7 * 24 * 60 * 60 * 1_000,
            ),
        );
        let chatos_browser_request_timeout = Duration::from_millis(
            required_u64("MCP_MANAGEMENT_CHATOS_BROWSER_TOOL_TIMEOUT_MS")?
                .clamp(30_000, 10 * 60 * 1_000),
        );
        let chatos_service_base_url = require_https_base_url(
            "MCP_MANAGEMENT_CHATOS_SERVICE_BASE_URL",
            normalize_base_url(required_text("MCP_MANAGEMENT_CHATOS_SERVICE_BASE_URL")?),
        )?;
        let chatos_http_client = chatos_service_runtime::build_mtls_http_client(
            chatos_service_runtime::HttpClientTimeouts::new(chatos_ask_user_request_timeout),
            required_path("CHATOS_MTLS_CA_CERT_PATH")?.as_path(),
            required_path("CHATOS_MTLS_CLIENT_IDENTITY_PATH")?.as_path(),
        )?;
        let provider_response_limit_bytes =
            required_usize("MCP_MANAGEMENT_PROVIDER_RESPONSE_LIMIT_BYTES")?
                .clamp(64 * 1024, 16 * 1024 * 1024);
        let async_tool_dispatch_topology = AsyncToolDispatchTopology::from_env()?;
        let public_base_url = normalize_base_url(required_text("MCP_MANAGEMENT_PUBLIC_BASE_URL")?);
        let project_service_base_url = require_https_base_url(
            "MCP_MANAGEMENT_PROJECT_SERVICE_BASE_URL",
            normalize_base_url(required_text("MCP_MANAGEMENT_PROJECT_SERVICE_BASE_URL")?),
        )?;
        let project_service_http_client = chatos_service_runtime::build_mtls_http_client(
            chatos_service_runtime::HttpClientTimeouts::new(project_service_tool_timeout),
            required_path("PROJECT_SERVICE_MTLS_CA_CERT_PATH")?.as_path(),
            required_path("PROJECT_SERVICE_MTLS_CLIENT_IDENTITY_PATH")?.as_path(),
        )?;
        let local_connector_service_base_url = require_https_base_url(
            "MCP_MANAGEMENT_LOCAL_CONNECTOR_SERVICE_BASE_URL",
            normalize_base_url(required_text(
                "MCP_MANAGEMENT_LOCAL_CONNECTOR_SERVICE_BASE_URL",
            )?),
        )?;
        let local_connector_http_client = chatos_service_runtime::build_mtls_http_client(
            chatos_service_runtime::HttpClientTimeouts::new(sandbox_image_request_timeout),
            required_path("LOCAL_CONNECTOR_MTLS_CA_CERT_PATH")?.as_path(),
            required_path("LOCAL_CONNECTOR_MTLS_CLIENT_IDENTITY_PATH")?.as_path(),
        )?;
        Ok(Self {
            host,
            port,
            internal_mtls_port,
            otlp_endpoint,
            otlp_trace_sample_ratio,
            otlp_export_timeout: Duration::from_millis(otlp_export_timeout_ms),
            mtls_server_cert_path: required_path("MCP_MANAGEMENT_MTLS_SERVER_CERT_PATH")?,
            mtls_server_key_path: required_path("MCP_MANAGEMENT_MTLS_SERVER_KEY_PATH")?,
            mtls_client_ca_cert_path: required_path("MCP_MANAGEMENT_MTLS_CLIENT_CA_CERT_PATH")?,
            internal_api_secrets,
            require_signed_internal_requests,
            allowed_internal_callers,
            plugin_management_service_base_url,
            plugin_management_http_client,
            plugin_management_internal_api_secret,
            project_service_base_url,
            project_service_http_client,
            project_service_internal_api_secret,
            project_service_tool_timeout,
            task_runner_service_base_url: require_https_base_url(
                "MCP_MANAGEMENT_TASK_RUNNER_SERVICE_BASE_URL",
                normalize_base_url(required_text(
                    "MCP_MANAGEMENT_TASK_RUNNER_SERVICE_BASE_URL",
                )?),
            )?,
            task_runner_mtls_ca_cert_path: required_path("TASK_RUNNER_MTLS_CA_CERT_PATH")?,
            task_runner_mtls_client_identity_path: required_path(
                "TASK_RUNNER_MTLS_CLIENT_IDENTITY_PATH",
            )?,
            task_runner_internal_api_secret,
            task_runner_request_timeout,
            task_runner_ask_user_request_timeout,
            chatos_service_base_url,
            chatos_http_client,
            chatos_internal_api_secret,
            chatos_ask_user_request_timeout,
            chatos_browser_request_timeout,
            local_connector_service_base_url,
            local_connector_http_client,
            local_connector_internal_api_secret,
            sandbox_manager_service_base_url,
            sandbox_manager_http_client,
            sandbox_manager_internal_api_secret,
            sandbox_manager_request_timeout,
            sandbox_image_request_timeout,
            embedded_work_dir: PathBuf::from(required_text("MCP_MANAGEMENT_EMBEDDED_WORK_DIR")?),
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

    pub fn internal_mtls_bind_addr(&self) -> SocketAddr {
        SocketAddr::new(self.host, self.internal_mtls_port)
    }

    #[cfg(test)]
    pub(crate) fn test() -> Self {
        Self {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 39280,
            internal_mtls_port: 39282,
            otlp_endpoint: "http://127.0.0.1:4317".to_string(),
            otlp_trace_sample_ratio: 1.0,
            otlp_export_timeout: Duration::from_secs(5),
            mtls_server_cert_path: PathBuf::from("/tmp/mcp-management-server.crt"),
            mtls_server_key_path: PathBuf::from("/tmp/mcp-management-server.key"),
            mtls_client_ca_cert_path: PathBuf::from("/tmp/mcp-management-ca.crt"),
            internal_api_secrets: BTreeMap::from([
                ("chatos".to_string(), "a-long-chatos-secret".to_string()),
                (
                    "task-runner".to_string(),
                    "a-long-task-runner-secret".to_string(),
                ),
                (
                    "project-service".to_string(),
                    "a-long-project-service-secret".to_string(),
                ),
                (
                    "configuration-center".to_string(),
                    "a-long-configuration-center-secret".to_string(),
                ),
            ]),
            require_signed_internal_requests: true,
            allowed_internal_callers: BTreeSet::from([
                "chatos".to_string(),
                "task-runner".to_string(),
                "project-service".to_string(),
                "configuration-center".to_string(),
            ]),
            plugin_management_service_base_url: "https://127.0.0.1:39262".to_string(),
            plugin_management_http_client: reqwest::Client::new(),
            plugin_management_internal_api_secret: Some(
                "a-long-plugin-management-secret".to_string(),
            ),
            project_service_base_url: "http://127.0.0.1:39210".to_string(),
            project_service_http_client: reqwest::Client::new(),
            project_service_internal_api_secret: Some("a-long-project-service-secret".to_string()),
            project_service_tool_timeout: Duration::from_secs(180),
            task_runner_service_base_url: "http://127.0.0.1:39090".to_string(),
            task_runner_mtls_ca_cert_path: PathBuf::new(),
            task_runner_mtls_client_identity_path: PathBuf::new(),
            task_runner_internal_api_secret: Some("a-long-task-runner-secret".to_string()),
            task_runner_request_timeout: Duration::from_secs(180),
            task_runner_ask_user_request_timeout: Duration::from_secs(86_700),
            chatos_service_base_url: "http://127.0.0.1:3997".to_string(),
            chatos_http_client: reqwest::Client::new(),
            chatos_internal_api_secret: Some("a-long-chatos-secret".to_string()),
            chatos_ask_user_request_timeout: Duration::from_secs(86_700),
            chatos_browser_request_timeout: Duration::from_secs(120),
            local_connector_service_base_url: "http://127.0.0.1:39230".to_string(),
            local_connector_http_client: reqwest::Client::new(),
            local_connector_internal_api_secret: Some("a-long-local-connector-secret".to_string()),
            sandbox_manager_service_base_url: "http://127.0.0.1:8095".to_string(),
            sandbox_manager_http_client: reqwest::Client::new(),
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
                queue_max_length: 10_000,
                queue_max_bytes: 256 * 1024 * 1024,
                rabbitmq_reconnect_delay: Duration::from_secs(3),
                max_delivery_attempts: 5,
                retry_delay: Duration::from_secs(5),
                rabbitmq_url: None,
                rabbitmq_exchange: None,
                cancellation_exchange: None,
                queue_name: None,
                retry_queue_name: None,
                dead_letter_queue_name: None,
            },
        }
    }
}

fn normalize_base_url(value: String) -> String {
    value.trim().trim_end_matches('/').to_string()
}

fn require_https_base_url(key: &str, value: String) -> Result<String, String> {
    let parsed =
        reqwest::Url::parse(value.as_str()).map_err(|err| format!("{key} is invalid: {err}"))?;
    if parsed.scheme() != "https" {
        return Err(format!("{key} must use https"));
    }
    Ok(value)
}

fn require_http_endpoint(key: &str, value: &str) -> Result<(), String> {
    let parsed = reqwest::Url::parse(value).map_err(|err| format!("{key} is invalid: {err}"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(format!("{key} must use http or https"));
    }
    Ok(())
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

fn required_path(key: &str) -> Result<PathBuf, String> {
    required_text(key).map(PathBuf::from)
}

fn required_u64(key: &str) -> Result<u64, String> {
    let value = required_text(key)?;
    value
        .parse::<u64>()
        .map_err(|_| format!("{key} must be an unsigned integer"))
}

fn required_f64(key: &str) -> Result<f64, String> {
    required_text(key)?
        .parse::<f64>()
        .map_err(|err| format!("{key} must be a valid number: {err}"))
}

fn required_u16(key: &str) -> Result<u16, String> {
    let value = required_text(key)?;
    value
        .parse::<u16>()
        .map_err(|err| format!("{key} must be a valid port: {err}"))
}

fn required_u32(key: &str) -> Result<u32, String> {
    let value = required_text(key)?;
    value
        .parse::<u32>()
        .map_err(|err| format!("{key} must be a valid unsigned integer: {err}"))
}

fn required_usize(key: &str) -> Result<usize, String> {
    let value = required_text(key)?;
    value
        .parse::<usize>()
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
        assert!(AsyncToolDispatchMode::parse("local").is_err());
        assert!(AsyncToolDispatchMode::parse("mystery").is_err());
    }

    #[test]
    fn rabbitmq_async_dispatch_requires_explicit_topology() {
        let topology = AsyncToolDispatchTopology {
            mode: AsyncToolDispatchMode::RabbitMq,
            worker_concurrency: 2,
            queue_max_length: 10_000,
            queue_max_bytes: 256 * 1024 * 1024,
            rabbitmq_reconnect_delay: Duration::from_secs(3),
            max_delivery_attempts: 5,
            retry_delay: Duration::from_secs(5),
            rabbitmq_url: None,
            rabbitmq_exchange: Some("mcp_management".to_string()),
            cancellation_exchange: Some("mcp_management.cancellations".to_string()),
            queue_name: Some("mcp_management.async.dispatch".to_string()),
            retry_queue_name: Some("mcp_management.async.retry".to_string()),
            dead_letter_queue_name: Some("mcp_management.async.dlq".to_string()),
        };
        assert!(topology.validate().is_err());
    }

    #[test]
    fn async_tool_worker_concurrency_is_bounded_for_rabbitmq_prefetch() {
        let mut topology = AppConfig::test().async_tool_dispatch_topology;
        topology.worker_concurrency = 513;

        assert!(topology.validate().is_err());
    }
}
