// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use super::project_service::decode_jsonrpc_response;
use super::{ProviderCallError, ProviderCallOutcome};

mod init;
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

#[cfg(test)]
mod tests;
