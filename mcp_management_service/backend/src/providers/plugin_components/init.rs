// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_management_sdk::{McpProviderKind, ResolvedMcpRoute};

use super::PluginComponentProvider;

impl PluginComponentProvider {
    pub(in crate::providers) fn new(
        http: reqwest::Client,
        base_url: impl Into<String>,
        request_timeout: std::time::Duration,
        internal_secret: Option<String>,
        response_limit_bytes: usize,
    ) -> Result<Self, String> {
        let base_url = base_url.into();
        let parsed = reqwest::Url::parse(base_url.as_str())
            .map_err(|error| format!("Plugin Component Provider base URL is invalid: {error}"))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err("Plugin Component Provider base URL must use http or https".to_string());
        }
        Ok(Self {
            http,
            base_url: base_url.trim().trim_end_matches('/').to_string(),
            internal_secret: internal_secret
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            request_timeout,
            response_limit_bytes,
        })
    }

    pub(in crate::providers) fn supports(&self, route: &ResolvedMcpRoute) -> bool {
        route
            .provider_ref
            .as_deref()
            .is_some_and(|value| value.starts_with("plugin-tool-binding:"))
            && route.provider_kind == McpProviderKind::PluginLocal
            && self.internal_secret.is_some()
    }
}
