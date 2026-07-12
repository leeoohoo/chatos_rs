// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::{HeaderMap, Method};

use super::ApiError;
use crate::config::AppConfig;
use crate::models::CurrentUser;

pub(super) const TOKEN_AUDIENCE: &str = "local-connector-service";
pub(super) const MCP_RELAY_SCOPE: &str = "relay.mcp";
pub(super) const TERMINAL_RELAY_SCOPE: &str = "relay.terminal";
pub(super) const MODEL_RUNTIME_READ_SCOPE: &str = "model-runtime.read";

const CHATOS_CALLER: &str = "chatos-backend";
const TASK_RUNNER_CALLER: &str = "task-runner";
const PROJECT_SERVICE_CALLER: &str = "project-service";
const MEMORY_ENGINE_CALLER: &str = "memory-engine";

pub(super) fn internal_service_user_from_request(
    config: &AppConfig,
    headers: &HeaderMap,
    method: &Method,
    path: &str,
) -> Result<Option<CurrentUser>, ApiError> {
    let caller = header_text(headers, "x-local-connector-caller");
    let token = header_text(headers, "x-local-connector-internal-token");
    let legacy_secret = header_text(headers, "x-local-connector-internal-secret");
    if caller.is_none() && token.is_none() && legacy_secret.is_none() {
        return Ok(None);
    }

    let access = internal_access_for_request(method, path).ok_or_else(|| {
        ApiError::forbidden(
            "internal service credentials are not allowed for this Local Connector operation",
        )
    })?;
    let caller = match caller {
        Some(caller) => caller,
        None if token.is_some() => {
            return Err(ApiError::bad_request(
                "Local Connector caller is required for signed internal requests",
            ));
        }
        None if config.require_signed_internal_requests => {
            return Err(ApiError::unauthorized(
                "signed Local Connector internal API token is required",
            ));
        }
        None => "legacy-service",
    };
    if caller != "legacy-service" && !access.allowed_callers.contains(&caller) {
        return Err(ApiError::forbidden(
            "caller service is not allowed for this Local Connector operation",
        ));
    }

    if caller == "legacy-service" {
        let expected = config
            .legacy_internal_api_secret
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| ApiError::unauthorized("Local Connector internal API is disabled"))?;
        require_legacy_secret(legacy_secret, expected)?;
    } else {
        let expected = config
            .internal_api_secrets
            .get(caller)
            .map(String::as_str)
            .or_else(|| {
                (!config.require_signed_internal_requests)
                    .then_some(config.legacy_internal_api_secret.as_deref())
                    .flatten()
            })
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                ApiError::unauthorized("Local Connector internal API is disabled for caller")
            })?;
        if let Some(token) = token {
            chatos_service_runtime::verify_internal_service_token(
                token,
                expected,
                caller,
                TOKEN_AUDIENCE,
                access.scope,
            )
            .map_err(|_| ApiError::unauthorized("invalid Local Connector internal API token"))?;
        } else {
            if config.require_signed_internal_requests {
                return Err(ApiError::unauthorized(
                    "signed Local Connector internal API token is required",
                ));
            }
            require_legacy_secret(legacy_secret, expected)?;
        }
    }

    let owner_user_id = header_text(headers, "x-local-connector-owner-user-id")
        .or_else(|| header_text(headers, "x-chatos-owner-user-id"))
        .ok_or_else(|| ApiError::unauthorized("Local Connector owner user id is required"))?
        .to_string();
    let service_name = caller.replace('-', "_");
    Ok(Some(CurrentUser {
        principal_type: "service".to_string(),
        user_id: format!("service:{caller}:{owner_user_id}"),
        username: Some(service_name.clone()),
        display_name: Some(service_name),
        role: "service".to_string(),
        owner_user_id: Some(owner_user_id),
    }))
}

struct InternalAccess {
    scope: &'static str,
    allowed_callers: &'static [&'static str],
}

fn internal_access_for_request(method: &Method, path: &str) -> Option<InternalAccess> {
    let parts = path.trim_matches('/').split('/').collect::<Vec<_>>();
    match (method, parts.as_slice()) {
        (&Method::POST, ["api", "local-connectors", "relay", _, "mcp"]) => Some(InternalAccess {
            scope: MCP_RELAY_SCOPE,
            allowed_callers: &[TASK_RUNNER_CALLER],
        }),
        (&Method::GET, ["api", "local-connectors", "model-runtime", _]) => Some(InternalAccess {
            scope: MODEL_RUNTIME_READ_SCOPE,
            allowed_callers: &[
                CHATOS_CALLER,
                TASK_RUNNER_CALLER,
                PROJECT_SERVICE_CALLER,
                MEMORY_ENGINE_CALLER,
            ],
        }),
        (
            &Method::POST,
            ["api", "local-connectors", "relay", _, "terminal", "exec" | "sessions" | "input"],
        )
        | (&Method::GET, ["api", "local-connectors", "relay", _, "terminal", "ws"]) => {
            Some(InternalAccess {
                scope: TERMINAL_RELAY_SCOPE,
                allowed_callers: &[TASK_RUNNER_CALLER],
            })
        }
        _ => None,
    }
}

fn require_legacy_secret(provided: Option<&str>, expected: &str) -> Result<(), ApiError> {
    let provided = provided
        .ok_or_else(|| ApiError::unauthorized("missing Local Connector internal API secret"))?;
    if !constant_time_eq(expected.as_bytes(), provided.as_bytes()) {
        return Err(ApiError::unauthorized(
            "invalid Local Connector internal API secret",
        ));
    }
    Ok(())
}

fn header_text<'a>(headers: &'a HeaderMap, key: &'static str) -> Option<&'a str> {
    headers
        .get(key)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
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
    use std::collections::HashMap;
    use std::net::{IpAddr, Ipv4Addr};
    use std::time::Duration;

    use axum::http::HeaderValue;

    use super::*;

    #[test]
    fn signed_token_is_bound_to_caller_scope_and_path() {
        let mut config = test_config();
        config.require_signed_internal_requests = true;
        config.internal_api_secrets.insert(
            TASK_RUNNER_CALLER.to_string(),
            "a-long-task-runner-local-connector-secret".to_string(),
        );
        let token = chatos_service_runtime::issue_internal_service_token(
            "a-long-task-runner-local-connector-secret",
            TASK_RUNNER_CALLER,
            TOKEN_AUDIENCE,
            MCP_RELAY_SCOPE,
            60,
        )
        .expect("issue token");
        let headers = signed_headers(TASK_RUNNER_CALLER, token.as_str());
        let user = internal_service_user_from_request(
            &config,
            &headers,
            &Method::POST,
            "/api/local-connectors/relay/device-1/mcp",
        )
        .expect("matching request")
        .expect("service user");
        assert_eq!(user.user_id, "service:task-runner:user-1");

        assert!(internal_service_user_from_request(
            &config,
            &headers,
            &Method::GET,
            "/api/local-connectors/model-runtime/model-1",
        )
        .is_err());
    }

    #[test]
    fn internal_credentials_cannot_access_management_routes() {
        let mut config = test_config();
        config.internal_api_secrets.insert(
            TASK_RUNNER_CALLER.to_string(),
            "a-long-task-runner-local-connector-secret".to_string(),
        );
        let token = chatos_service_runtime::issue_internal_service_token(
            "a-long-task-runner-local-connector-secret",
            TASK_RUNNER_CALLER,
            TOKEN_AUDIENCE,
            MCP_RELAY_SCOPE,
            60,
        )
        .expect("issue token");
        let headers = signed_headers(TASK_RUNNER_CALLER, token.as_str());
        assert!(internal_service_user_from_request(
            &config,
            &headers,
            &Method::GET,
            "/api/local-connectors/devices",
        )
        .is_err());
    }

    fn signed_headers(caller: &'static str, token: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("x-local-connector-caller", HeaderValue::from_static(caller));
        headers.insert(
            "x-local-connector-internal-token",
            HeaderValue::from_str(token).expect("token header"),
        );
        headers.insert(
            "x-local-connector-owner-user-id",
            HeaderValue::from_static("user-1"),
        );
        headers
    }

    fn test_config() -> AppConfig {
        AppConfig {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 0,
            database_url: "mongodb://127.0.0.1/test".to_string(),
            user_service_base_url: "http://127.0.0.1:39190".to_string(),
            user_service_request_timeout: Duration::from_secs(1),
            relay_request_timeout: Duration::from_secs(1),
            sandbox_image_relay_request_timeout: Duration::from_secs(1),
            public_base_url: None,
            legacy_internal_api_secret: Some("legacy-local-connector-secret".to_string()),
            internal_api_secrets: HashMap::new(),
            require_signed_internal_requests: false,
            memory_engine_base_url: "http://127.0.0.1:7081/api/memory-engine/v1".to_string(),
            memory_engine_operator_token: None,
            memory_engine_request_timeout: Duration::from_secs(1),
            require_device_connect_signature: true,
            allow_device_connect_query_token: false,
            device_connect_signature_max_skew: Duration::from_secs(300),
        }
    }
}
