// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use chatos_mcp::{system_mcp_descriptor_by_resource_id, SystemMcpKey};
use chatos_mcp_management_sdk::ResolvedMcpRoute;
use serde_json::Value;

const CLOUD_PROVIDER_REF: &str = "sandbox-images:cloud";
const LOCAL_PROVIDER_REF_PREFIX: &str = "sandbox-images:local:";
const TOOL_CREATE_IMAGE: &str = "create_image";
const DEFAULT_CREATE_TIMEOUT_MS: u64 = 30 * 60 * 1_000;
const MAX_CREATE_TIMEOUT_MS: u64 = 2 * 60 * 60 * 1_000;
const TRANSPORT_GRACE_MS: u64 = 30_000;

pub(crate) const fn cloud_provider_ref() -> &'static str {
    CLOUD_PROVIDER_REF
}

pub(crate) fn local_provider_ref(pairing_id: &str) -> String {
    format!("{LOCAL_PROVIDER_REF_PREFIX}{}", pairing_id.trim())
}

pub(super) fn is_sandbox_images_route(route: &ResolvedMcpRoute) -> bool {
    system_mcp_descriptor_by_resource_id(route.resource_id.as_str())
        .is_some_and(|descriptor| descriptor.key == SystemMcpKey::SandboxImages)
}

pub(super) fn normalized_base_url(value: String, provider: &str) -> Result<String, String> {
    let parsed = reqwest::Url::parse(value.as_str())
        .map_err(|error| format!("{provider} image base URL is invalid: {error}"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(format!("{provider} image base URL must use http or https"));
    }
    Ok(value.trim().trim_end_matches('/').to_string())
}

pub(super) fn normalized_secret(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub(super) fn call_timeout(
    tool_name: &str,
    arguments: &Value,
    request_timeout: Duration,
    image_request_timeout: Duration,
) -> Duration {
    if tool_name != TOOL_CREATE_IMAGE {
        return request_timeout.min(image_request_timeout);
    }
    let requested = arguments
        .get("timeout_ms")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_CREATE_TIMEOUT_MS)
        .clamp(1_000, MAX_CREATE_TIMEOUT_MS);
    Duration::from_millis(requested.saturating_add(TRANSPORT_GRACE_MS)).min(image_request_timeout)
}
