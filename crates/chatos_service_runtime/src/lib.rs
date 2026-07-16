// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod config;
mod consul;
mod env_config;
mod error;
pub mod http_body;
mod http_client;
mod internal_token;
mod runtime;
mod security;
mod utils;

pub use config::{DiscoveryMode, RuntimeConfig};
pub use consul::{ServiceEndpoint, ServiceRegistration};
pub use env_config::env_text;
pub use error::ServiceRuntimeError;
pub use internal_token::{
    issue_internal_service_token, verify_internal_service_token, InternalServiceTokenClaims,
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
