// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::HeaderMap;

use crate::config::AppConfig;
use crate::error::ApiError;

const INTERNAL_TOKEN_AUDIENCE: &str = "mcp-management-service";
const INTERNAL_SECRET_HEADER: &str = "x-mcp-management-internal-secret";
const INTERNAL_TOKEN_HEADER: &str = "x-mcp-management-internal-token";
const CALLER_SERVICE_HEADER: &str = "x-mcp-management-caller-service";

pub fn require_internal_request(
    config: &AppConfig,
    headers: &HeaderMap,
    required_scope: &str,
) -> Result<String, ApiError> {
    let caller = headers
        .get(CALLER_SERVICE_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::bad_request("caller service is required"))?;
    if !config.allowed_internal_callers.contains(caller) {
        return Err(ApiError::forbidden("caller service is not allowed"));
    }
    if let Some(token) = headers
        .get(INTERNAL_TOKEN_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        chatos_service_runtime::verify_internal_service_token(
            token,
            config.internal_api_secret.as_str(),
            caller,
            INTERNAL_TOKEN_AUDIENCE,
            required_scope,
        )
        .map_err(|_| ApiError::unauthorized("invalid MCP management internal API token"))?;
        return Ok(caller.to_string());
    }
    if config.require_signed_internal_requests {
        return Err(ApiError::unauthorized(
            "signed MCP management internal API token is required",
        ));
    }
    let secret = headers
        .get(INTERNAL_SECRET_HEADER)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| ApiError::unauthorized("missing MCP management internal API secret"))?;
    if !constant_time_eq(config.internal_api_secret.as_bytes(), secret.as_bytes()) {
        return Err(ApiError::unauthorized(
            "invalid MCP management internal API secret",
        ));
    }
    Ok(caller.to_string())
}

fn constant_time_eq(expected: &[u8], actual: &[u8]) -> bool {
    let mut difference = expected.len() ^ actual.len();
    for (left, right) in expected.iter().zip(actual.iter()) {
        difference |= usize::from(left ^ right);
    }
    difference == 0
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::net::{IpAddr, Ipv4Addr};

    use super::*;

    fn config(require_signed: bool) -> AppConfig {
        AppConfig {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 39280,
            internal_api_secret: "a-long-test-secret".to_string(),
            require_signed_internal_requests: require_signed,
            allowed_internal_callers: BTreeSet::from(["task-runner".to_string()]),
        }
    }

    #[test]
    fn signed_request_binds_scope_and_caller() {
        let token = chatos_service_runtime::issue_internal_service_token(
            "a-long-test-secret",
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
            require_internal_request(&config(true), &headers, "routes.resolve").unwrap(),
            "task-runner"
        );
        assert!(require_internal_request(&config(true), &headers, "catalog.read").is_err());
    }
}
