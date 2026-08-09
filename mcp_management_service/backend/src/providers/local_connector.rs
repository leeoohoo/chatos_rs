// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use super::ProviderCallError;

mod binding;
mod init;
mod request_builder;
mod runtime_calls;

const CALLER_SERVICE: &str = "mcp-management-service";
const TOKEN_AUDIENCE: &str = "local-connector-service";
const MCP_RELAY_SCOPE: &str = "relay.mcp";
const LOCAL_CONNECTOR_PROJECT_ID_HEADER: &str = "x-local-connector-project-id";

#[derive(Clone)]
pub(super) struct LocalConnectorProvider {
    http: reqwest::Client,
    base_url: String,
    internal_secret: Option<String>,
    request_timeout: Duration,
    response_limit_bytes: usize,
}

#[cfg(test)]
mod tests;
