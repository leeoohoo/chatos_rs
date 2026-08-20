// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp::{system_mcp_descriptor_by_resource_id, SystemMcpKey};
use chatos_mcp_management_sdk::{McpProviderKind, ResolvedMcpRoute};

use super::{ChatosProvider, CHATOS_MEMORY_PROVIDER_REF_PREFIX, CHATOS_PROVIDER_REF};

impl ChatosProvider {
    pub(in crate::providers) fn new(
        http: reqwest::Client,
        base_url: impl Into<String>,
        request_timeout: std::time::Duration,
        ask_user_request_timeout: std::time::Duration,
        browser_request_timeout: std::time::Duration,
        internal_secret: Option<String>,
        response_limit_bytes: usize,
    ) -> Result<Self, String> {
        let base_url = base_url.into();
        let parsed = reqwest::Url::parse(base_url.as_str())
            .map_err(|err| format!("ChatOS Provider base URL is invalid: {err}"))?;
        if parsed.scheme() != "https" && !cfg!(test) {
            return Err("ChatOS Provider base URL must use https".to_string());
        }
        Ok(Self {
            http,
            base_url: base_url.trim().trim_end_matches('/').to_string(),
            internal_secret: internal_secret
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            request_timeout,
            ask_user_request_timeout,
            browser_request_timeout,
            response_limit_bytes,
        })
    }

    pub(in crate::providers) fn supports(&self, route: &ResolvedMcpRoute) -> bool {
        if self.internal_secret.is_none() || route.provider_kind != McpProviderKind::InternalService
        {
            return false;
        }
        system_mcp_descriptor_by_resource_id(route.resource_id.as_str()).is_some_and(|descriptor| {
            match descriptor.key {
                SystemMcpKey::AgentBuilder
                | SystemMcpKey::AskUser
                | SystemMcpKey::BrowserTools
                | SystemMcpKey::Notepad => {
                    route.provider_ref.as_deref() == Some(CHATOS_PROVIDER_REF)
                }
                SystemMcpKey::MemorySkillReader
                | SystemMcpKey::MemoryCommandReader
                | SystemMcpKey::MemoryPluginReader => route
                    .provider_ref
                    .as_deref()
                    .and_then(|value| value.strip_prefix(CHATOS_MEMORY_PROVIDER_REF_PREFIX))
                    .is_some_and(|value| !value.trim().is_empty()),
                _ => false,
            }
        })
    }
}
