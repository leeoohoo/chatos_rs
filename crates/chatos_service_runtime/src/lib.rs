// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod config;
mod consul;
mod dotenv;
mod env_config;
mod error;
mod http_auth;
pub mod http_body;
mod http_client;
mod http_error;
mod identity;
mod internal_token;
#[cfg(feature = "axum-support")]
mod request_id;
mod runtime;
mod security;
mod utils;

pub use config::{DiscoveryMode, RuntimeConfig};
pub use consul::{ServiceEndpoint, ServiceRegistration};
pub use dotenv::{load_service_dotenv, service_dotenv_files};
pub use env_config::{env_bool_strict, env_flag, env_parse, env_text, parse_bool_text};
pub use error::ServiceRuntimeError;
pub use http_auth::{bearer_token_from_headers, query_has_nonempty_parameter, BearerTokenError};
pub use http_client::{build_http_client, http_client_builder, HttpClientTimeouts};
pub use http_error::{classify_http_request_error, HttpRequestErrorKind};
pub use identity::{normalize_owned_identity_text, normalized_identity_text};
pub use internal_token::{
    issue_internal_service_token, verify_internal_service_token, InternalServiceTokenClaims,
};
#[cfg(feature = "axum-support")]
pub use request_id::{
    request_id_from_headers, request_id_middleware, resolve_request_id, RequestId,
    REQUEST_ID_HEADER,
};
pub use runtime::{
    apply_config_center_env, register_current_service, resolve_service_base_url,
    resolve_service_url, ChatosServiceRuntime,
};
pub use security::{is_production_environment, validate_production_secret};

pub const DEFAULT_MEMORY_ENGINE_OPERATOR_TOKEN: &str = "chatos-memory-engine-dev-operator-token";
pub const DEFAULT_SANDBOX_MANAGER_OPERATOR_TOKEN: &str =
    "chatos-sandbox-manager-dev-operator-token";
pub const DEFAULT_SANDBOX_MANAGER_AGENT_TOKEN_SECRET: &str = "chatos-sandbox-agent-dev-secret";
pub const DEFAULT_SANDBOX_MANAGER_SYSTEM_CLIENT_ID: &str = "task_runner";
pub const DEFAULT_SANDBOX_MANAGER_SYSTEM_CLIENT_KEY: &str = "chatos-task-runner-sandbox-dev-key";
