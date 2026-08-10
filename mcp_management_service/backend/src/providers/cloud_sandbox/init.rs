// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp::{system_mcp_descriptor_by_resource_id, SystemMcpKey};
use chatos_mcp_management_sdk::{McpProviderKind, ResolvedMcpRoute};

use super::CloudSandboxProvider;

impl CloudSandboxProvider {
    pub(in crate::providers) fn new(
        http: reqwest::Client,
        base_url: impl Into<String>,
        request_timeout: std::time::Duration,
        internal_secret: Option<String>,
        response_limit_bytes: usize,
    ) -> Result<Self, String> {
        let base_url = base_url.into();
        let parsed = reqwest::Url::parse(base_url.as_str())
            .map_err(|err| format!("Sandbox Manager Provider base URL is invalid: {err}"))?;
        if !cfg!(test) && parsed.scheme() != "https" {
            return Err("Sandbox Manager Provider base URL must use https".to_string());
        }
        Ok(Self {
            http,
            base_url: base_url.trim().trim_end_matches('/').to_string(),
            request_timeout,
            internal_secret: internal_secret
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            response_limit_bytes,
        })
    }

    pub(in crate::providers) fn supports(&self, route: &ResolvedMcpRoute) -> bool {
        if self.internal_secret.is_none() || route.provider_kind != McpProviderKind::CloudSandbox {
            return false;
        }
        system_mcp_descriptor_by_resource_id(route.resource_id.as_str()).is_some_and(|descriptor| {
            matches!(
                descriptor.key,
                SystemMcpKey::CodeMaintainerRead
                    | SystemMcpKey::CodeMaintainerWrite
                    | SystemMcpKey::TerminalController
            )
        })
    }
}
