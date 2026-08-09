// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{CloudStdioProvider, McpProviderKind, ResolvedMcpRoute};

impl CloudStdioProvider {
    pub(in crate::providers) fn new(
        http: reqwest::Client,
        base_url: impl Into<String>,
        request_timeout: std::time::Duration,
        internal_secret: Option<String>,
        response_limit_bytes: usize,
    ) -> Result<Self, String> {
        let base_url = base_url.into();
        let parsed = reqwest::Url::parse(base_url.as_str())
            .map_err(|error| format!("Sandbox Manager cloud stdio base URL is invalid: {error}"))?;
        if !cfg!(test) && parsed.scheme() != "https" {
            return Err("Sandbox Manager cloud stdio base URL must use https".to_string());
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
        self.internal_secret.is_some()
            && route.provider_kind == McpProviderKind::CloudStdio
            && route
                .provider_ref
                .as_deref()
                .is_some_and(|provider_ref| provider_ref.starts_with("sandbox:"))
    }
}
