// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_service::MCP_ERROR_INTERNAL;
use serde_json::Value;

use super::ProviderCancelOutcome;

mod endpoint;
mod init;
mod request_builder;
mod response;
mod runtime_calls;
pub(in crate::providers) use response::decode_jsonrpc_response;

const CALLER_SERVICE: &str = "mcp-management-service";
const TOKEN_AUDIENCE: &str = "project-service";
const PROJECT_MCP_SCOPE: &str = "project.mcp";
const PROJECT_MANAGEMENT_OWNER_SERVICE: &str = "project_management_service";

#[derive(Debug, Clone, PartialEq)]
pub struct ProviderCallOutcome {
    pub result: Value,
    pub response_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderWaitingForUser {
    pub prompt_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderCallError {
    pub code: i32,
    pub message: String,
}

impl ProviderCallError {
    pub fn provider_unavailable(message: impl Into<String>) -> Self {
        Self {
            code: MCP_ERROR_INTERNAL,
            message: message.into(),
        }
    }

    pub(super) fn invalid_response(message: impl Into<String>) -> Self {
        Self {
            code: MCP_ERROR_INTERNAL,
            message: message.into(),
        }
    }
}

#[derive(Clone)]
pub(super) struct ProjectServiceProvider {
    http: reqwest::Client,
    base_url: String,
    internal_secret: Option<String>,
    request_timeout: std::time::Duration,
    response_limit_bytes: usize,
}

#[cfg(test)]
mod tests;
