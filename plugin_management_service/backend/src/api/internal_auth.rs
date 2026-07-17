// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(super) const INTERNAL_TOKEN_AUDIENCE: &str = "plugin-management-service";
pub(super) const CAPABILITIES_RESOLVE_SCOPE: &str = "capabilities.resolve";
pub(super) const AGENT_PROMPTS_RESOLVE_SCOPE: &str = "agent-prompts.resolve";
pub(super) const AGENT_PROMPTS_SYNC_SCOPE: &str = "agent-prompts.sync";
pub(super) const LOCAL_CONNECTOR_READ_SCOPE: &str = "local-connector.read";
pub(super) const LOCAL_CONNECTOR_WRITE_SCOPE: &str = "local-connector.write";

pub(super) fn require_local_connector_internal_request(
    state: &AppState,
    headers: &HeaderMap,
    required_scope: &str,
) -> Result<(), ApiError> {
    let caller = require_internal_caller_service(headers)?;
    if caller != "local-connector-service" {
        return Err(ApiError::forbidden(
            "local connector MCP sync requires local-connector-service caller",
        ));
    }
    require_internal_api_secret(state, headers, caller, required_scope)?;
    Ok(())
}

pub(super) fn require_internal_caller_service(headers: &HeaderMap) -> Result<&str, ApiError> {
    let caller_service = headers
        .get("x-plugin-management-caller-service")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::bad_request("caller service is required"))?;
    if !ALLOWED_INTERNAL_CALLER_SERVICES.contains(&caller_service) {
        return Err(ApiError::forbidden("caller service is not allowed"));
    }
    Ok(caller_service)
}

pub(super) fn require_internal_api_secret(
    state: &AppState,
    headers: &HeaderMap,
    caller_service: &str,
    required_scope: &str,
) -> Result<(), ApiError> {
    let expected = state
        .config
        .internal_api_secrets
        .get(caller_service)
        .map(String::as_str)
        .or(state.config.internal_api_secret.as_deref())
        .ok_or_else(|| ApiError::unauthorized("plugin management internal API is disabled"))?;
    if let Some(token) = headers
        .get("x-plugin-management-internal-token")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        chatos_service_runtime::verify_internal_service_token(
            token,
            expected,
            caller_service,
            INTERNAL_TOKEN_AUDIENCE,
            required_scope,
        )
        .map_err(|_| ApiError::unauthorized("invalid plugin management internal API token"))?;
        return Ok(());
    }
    if state.config.require_signed_internal_requests {
        return Err(ApiError::unauthorized(
            "signed plugin management internal API token is required",
        ));
    }
    let actual = headers
        .get("x-plugin-management-internal-secret")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| ApiError::unauthorized("missing plugin management internal API secret"))?;
    if !constant_time_eq(expected.as_bytes(), actual.as_bytes()) {
        return Err(ApiError::unauthorized(
            "invalid plugin management internal API secret",
        ));
    }
    Ok(())
}

pub(super) fn constant_time_eq(expected: &[u8], actual: &[u8]) -> bool {
    let mut difference = expected.len() ^ actual.len();
    for (left, right) in expected.iter().zip(actual.iter()) {
        difference |= usize::from(left ^ right);
    }
    difference == 0
}
