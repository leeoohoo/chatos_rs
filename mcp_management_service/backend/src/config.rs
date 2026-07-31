// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::Path;

use chatos_service_runtime::{
    env_bool_strict, env_text, is_production_environment, validate_production_secret,
};

const DEFAULT_INTERNAL_SECRET: &str = "change_me_mcp_management_internal_secret";

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub host: IpAddr,
    pub port: u16,
    pub internal_api_secret: String,
    pub require_signed_internal_requests: bool,
    pub allowed_internal_callers: BTreeSet<String>,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, String> {
        let host = env_text("MCP_MANAGEMENT_HOST")
            .and_then(|value| value.parse::<IpAddr>().ok())
            .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
        let port = env_text("MCP_MANAGEMENT_PORT")
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(39280);
        let internal_api_secret = env_text("MCP_MANAGEMENT_INTERNAL_API_SECRET")
            .unwrap_or_else(|| DEFAULT_INTERNAL_SECRET.to_string());
        validate_production_secret(
            "MCP_MANAGEMENT_INTERNAL_API_SECRET",
            Some(internal_api_secret.as_str()),
            &[DEFAULT_INTERNAL_SECRET],
        )?;
        let require_signed_internal_requests = env_bool_strict(
            "MCP_MANAGEMENT_REQUIRE_SIGNED_INTERNAL_REQUESTS",
            is_production_environment(),
        )?;
        let allowed_internal_callers = env_text("MCP_MANAGEMENT_ALLOWED_INTERNAL_CALLERS")
            .map(|value| parse_callers(value.as_str()))
            .unwrap_or_else(default_internal_callers);
        if allowed_internal_callers.is_empty() {
            return Err("MCP_MANAGEMENT_ALLOWED_INTERNAL_CALLERS cannot be empty".to_string());
        }
        Ok(Self {
            host,
            port,
            internal_api_secret,
            require_signed_internal_requests,
            allowed_internal_callers,
        })
    }

    pub fn bind_addr(&self) -> SocketAddr {
        SocketAddr::new(self.host, self.port)
    }
}

fn parse_callers(value: &str) -> BTreeSet<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn default_internal_callers() -> BTreeSet<String> {
    [
        "chatos",
        "task-runner",
        "project-service",
        "memory-engine",
        "local-connector-service",
        "sandbox-manager",
        "plugin-management-service",
    ]
    .into_iter()
    .map(ToOwned::to_owned)
    .collect()
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
}
