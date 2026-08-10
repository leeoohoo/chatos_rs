// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use chatos_mcp_management_sdk::SandboxExecutionTarget;

use super::ProviderCallError;

mod init;
mod manager_client;
mod pairing_client;
mod records;
mod runtime_calls;
mod target_validation;

const CALLER_SERVICE: &str = "mcp-management-service";
const TOKEN_AUDIENCE: &str = "local-connector-service";
const SANDBOX_ROUTING_SCOPE: &str = "sandbox-routing.read";
const SANDBOX_SERVICE_SCOPE: &str = "sandbox.service";

#[derive(Clone)]
pub(super) struct LocalSandboxProvider {
    http: reqwest::Client,
    base_url: String,
    internal_secret: Option<String>,
    request_timeout: Duration,
    response_limit_bytes: usize,
}

#[cfg(test)]
mod tests;
