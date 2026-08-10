// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp::{system_mcp_descriptor_by_resource_id, SystemMcpKey};
use chatos_mcp_management_sdk::{McpProviderKind, ResolvedMcpRoute};

use super::{ProjectServiceProvider, PROJECT_MANAGEMENT_OWNER_SERVICE};

impl ProjectServiceProvider {
    pub(in crate::providers) fn new(
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

    pub(in crate::providers) fn supports(&self, route: &ResolvedMcpRoute) -> bool {
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
