// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use chatos_mcp_management_sdk::{McpProviderKind, ResolvedMcpRoute};

use super::project_service::decode_jsonrpc_response;
use super::{ProviderCallError, ProviderCallOutcome};

mod request_builder;
mod runtime_calls;
mod support;
pub(crate) use support::{cloud_provider_ref, local_provider_ref};
use support::{is_sandbox_images_route, normalized_base_url, normalized_secret};

const CALLER_SERVICE: &str = "mcp-management-service";
const SANDBOX_MANAGER_AUDIENCE: &str = "sandbox-manager";
const LOCAL_CONNECTOR_AUDIENCE: &str = "local-connector-service";
const SANDBOX_SERVICE_SCOPE: &str = "sandbox.service";

#[derive(Clone)]
pub(super) struct SandboxImagesProvider {
    cloud_http: reqwest::Client,
    cloud_base_url: String,
    cloud_internal_secret: Option<String>,
    local_http: reqwest::Client,
    local_base_url: String,
    local_internal_secret: Option<String>,
    request_timeout: Duration,
    image_request_timeout: Duration,
    response_limit_bytes: usize,
}

impl SandboxImagesProvider {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        cloud_http: reqwest::Client,
        cloud_base_url: impl Into<String>,
        cloud_internal_secret: Option<String>,
        local_http: reqwest::Client,
        local_base_url: impl Into<String>,
        local_internal_secret: Option<String>,
        request_timeout: Duration,
        image_request_timeout: Duration,
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

    pub(super) fn supports(&self, route: &ResolvedMcpRoute) -> bool {
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

#[cfg(test)]
mod tests;
