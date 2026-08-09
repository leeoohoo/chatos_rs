// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use super::project_service::decode_jsonrpc_response;
use super::ProviderCallError;

mod init;
mod manager_client;
mod runtime_calls;
mod validation;

const CALLER_SERVICE: &str = "mcp-management-service";
const TOKEN_AUDIENCE: &str = "sandbox-manager";
const INTERNAL_SCOPE: &str = "sandbox.service";

#[derive(Clone)]
pub(super) struct CloudSandboxProvider {
    http: reqwest::Client,
    base_url: String,
    request_timeout: Duration,
    internal_secret: Option<String>,
    response_limit_bytes: usize,
}

#[cfg(test)]
mod tests;
