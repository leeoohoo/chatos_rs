// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp::{system_mcp_descriptor_by_resource_id, SystemMcpKey};
use chatos_mcp_management_sdk::{McpProviderKind, ResolvedMcpRoute};
use chatos_mcp_service::MCP_ERROR_INTERNAL;
use serde_json::Value;

use super::ProviderCancelOutcome;

mod endpoint;
mod request_builder;
mod response;
mod runtime_calls;
pub(in crate::providers) use response::decode_jsonrpc_response;

const CALLER_SERVICE: &str = "mcp-management-service";
const TOKEN_AUDIENCE: &str = "project-service";
const PROJECT_MCP_SCOPE: &str = "project.mcp";
const PROJECT_READ_SCOPE: &str = "project.read";
const PROJECT_HARNESS_SCOPE: &str = "project.harness";
const PROJECT_ENVIRONMENT_SCOPE: &str = "project.environment";
const PROJECT_MANAGEMENT_OWNER_SERVICE: &str = "project_management_service";

#[derive(Debug, Clone, PartialEq)]
pub struct ProviderCallOutcome {
    pub result: Value,
    pub response_bytes: usize,
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
    response_limit_bytes: usize,
}

impl ProjectServiceProvider {
    pub(super) fn new(
        http: reqwest::Client,
        base_url: impl Into<String>,
        internal_secret: Option<String>,
        response_limit_bytes: usize,
    ) -> Result<Self, String> {
        let base_url = base_url.into();
        let parsed = reqwest::Url::parse(base_url.as_str())
            .map_err(|err| format!("project service Provider base URL is invalid: {err}"))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err("project service Provider base URL must use http or https".to_string());
        }
        Ok(Self {
            http,
            base_url: base_url.trim().trim_end_matches('/').to_string(),
            internal_secret: internal_secret
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            response_limit_bytes,
        })
    }

    pub(super) fn supports(&self, route: &ResolvedMcpRoute) -> bool {
        if self.internal_secret.is_none() {
            return false;
        }
        let Some(descriptor) = system_mcp_descriptor_by_resource_id(route.resource_id.as_str())
        else {
            return false;
        };
        match route.provider_kind {
            McpProviderKind::Harness => matches!(
                descriptor.key,
                SystemMcpKey::CodeMaintainerRead | SystemMcpKey::CodeMaintainerWrite
            ),
            McpProviderKind::InternalService
                if route.provider_ref.as_deref() == Some(PROJECT_MANAGEMENT_OWNER_SERVICE) =>
            {
                matches!(
                    descriptor.key,
                    SystemMcpKey::ProjectManagement
                        | SystemMcpKey::ProjectEnvironment
                        | SystemMcpKey::ProjectRuntimeEnvironment
                )
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests;
