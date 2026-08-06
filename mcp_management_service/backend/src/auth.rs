// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::HeaderMap;

use crate::config::AppConfig;
use crate::error::ApiError;

const INTERNAL_TOKEN_AUDIENCE: &str = "mcp-management-service";
const INTERNAL_TOKEN_HEADER: &str = "x-mcp-management-internal-token";
const CALLER_SERVICE_HEADER: &str = "x-mcp-management-caller-service";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InternalRequestIdentity {
    pub caller: String,
    pub trace_id: Option<String>,
}

impl InternalRequestIdentity {
    pub fn require_signed_trace_id(&self) -> Result<&str, ApiError> {
        self.trace_id.as_deref().ok_or_else(|| {
            ApiError::unauthorized("signed MCP management internal API token is required")
        })
    }
}

pub fn require_internal_request(
    config: &AppConfig,
    headers: &HeaderMap,
    required_scope: &str,
) -> Result<String, ApiError> {
    require_internal_request_identity(config, headers, required_scope)
        .map(|identity| identity.caller)
}

pub fn require_internal_request_identity(
    config: &AppConfig,
    headers: &HeaderMap,
    required_scope: &str,
) -> Result<InternalRequestIdentity, ApiError> {
    let caller = headers
        .get(CALLER_SERVICE_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::bad_request("caller service is required"))?;
    if !config.allowed_internal_callers.contains(caller) {
        return Err(ApiError::forbidden("caller service is not allowed"));
    }
    let secret = config
        .internal_api_secrets
        .get(caller)
        .ok_or_else(|| ApiError::unauthorized("MCP management caller secret is not configured"))?;
    let token = headers
        .get(INTERNAL_TOKEN_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ApiError::unauthorized("signed MCP management internal API token is required")
        })?;
    let claims = chatos_service_runtime::verify_internal_service_token(
        token,
        secret.as_str(),
        caller,
        INTERNAL_TOKEN_AUDIENCE,
        required_scope,
    )
    .map_err(|_| ApiError::unauthorized("invalid MCP management internal API token"))?;
    Ok(InternalRequestIdentity {
        caller: caller.to_string(),
        trace_id: Some(claims.trace_id),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> AppConfig {
        AppConfig::test()
    }

    #[test]
    fn signed_request_binds_scope_and_caller() {
        let token = chatos_service_runtime::issue_internal_service_token(
            "a-long-task-runner-secret",
            "task-runner",
            INTERNAL_TOKEN_AUDIENCE,
            "routes.resolve",
            60,
        )
        .unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(CALLER_SERVICE_HEADER, "task-runner".parse().unwrap());
        headers.insert(INTERNAL_TOKEN_HEADER, token.parse().unwrap());
        assert_eq!(
            require_internal_request(&config(), &headers, "routes.resolve").unwrap(),
            "task-runner"
        );
        assert!(require_internal_request(&config(), &headers, "catalog.read").is_err());
        let identity =
            require_internal_request_identity(&config(), &headers, "routes.resolve").unwrap();
        assert_eq!(identity.caller, "task-runner");
        assert!(identity.trace_id.is_some());
        assert!(identity.require_signed_trace_id().is_ok());
    }

    #[test]
    fn legacy_secret_header_is_rejected() {
        let mut headers = HeaderMap::new();
        headers.insert(CALLER_SERVICE_HEADER, "task-runner".parse().unwrap());
        headers.insert(
            "x-mcp-management-internal-secret",
            "a-long-test-secret".parse().unwrap(),
        );

        let error = require_internal_request(&config(), &headers, "routes.resolve")
            .expect_err("legacy secret header must not authenticate");
        let response = axum::response::IntoResponse::into_response(error);
        assert_eq!(response.status(), axum::http::StatusCode::UNAUTHORIZED);
    }
}
