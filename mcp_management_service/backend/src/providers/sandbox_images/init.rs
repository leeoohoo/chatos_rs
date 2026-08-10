// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_management_sdk::{McpProviderKind, ResolvedMcpRoute};

use super::{
    is_sandbox_images_route, normalized_base_url, normalized_secret, SandboxImagesProvider,
};

impl SandboxImagesProvider {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::providers) fn new(
        cloud_http: reqwest::Client,
        cloud_base_url: impl Into<String>,
        cloud_internal_secret: Option<String>,
        local_http: reqwest::Client,
        local_base_url: impl Into<String>,
        local_internal_secret: Option<String>,
        request_timeout: std::time::Duration,
        image_request_timeout: std::time::Duration,
        response_limit_bytes: usize,
    ) -> Result<Self, String> {
        let cloud_base_url = normalized_base_url(cloud_base_url.into(), "Sandbox Manager")?;
        let local_base_url = normalized_base_url(local_base_url.into(), "Local Connector")?;
        Ok(Self {
            cloud_http,
            cloud_base_url,
            cloud_internal_secret: normalized_secret(cloud_internal_secret),
            local_http,
            local_base_url,
            local_internal_secret: normalized_secret(local_internal_secret),
            request_timeout,
            image_request_timeout,
            response_limit_bytes,
        })
    }

    pub(in crate::providers) fn supports(&self, route: &ResolvedMcpRoute) -> bool {
        if !is_sandbox_images_route(route) {
            return false;
        }
        match route.provider_kind {
            McpProviderKind::CloudSandbox => self.cloud_internal_secret.is_some(),
            McpProviderKind::LocalConnector => self.local_internal_secret.is_some(),
            _ => false,
        }
    }
}
