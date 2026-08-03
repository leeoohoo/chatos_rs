// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::{HeaderMap, Method};

use super::ApiError;
use crate::config::AppConfig;
use crate::models::CurrentUser;

pub(super) const TOKEN_AUDIENCE: &str = "local-connector-service";
pub(super) const MCP_RELAY_SCOPE: &str = "relay.mcp";
pub(super) const TERMINAL_RELAY_SCOPE: &str = "relay.terminal";
pub(super) const SKILL_RELAY_SCOPE: &str = "relay.skill";
pub(super) const PLUGIN_RELAY_SCOPE: &str = "plugin.execute";
pub(super) const PLUGIN_UI_READ_SCOPE: &str = "plugin.ui.read";
pub(super) const PLUGIN_ARTIFACT_READ_SCOPE: &str = "plugin.artifact.read";
pub(super) const PLUGIN_ARTIFACT_WRITE_SCOPE: &str = "plugin.artifact.write";
pub(super) const SANDBOX_ROUTING_READ_SCOPE: &str = "sandbox-routing.read";
pub(super) const SANDBOX_SERVICE_SCOPE: &str = "sandbox.service";

const CHATOS_CALLER: &str = "chatos-backend";
const TASK_RUNNER_CALLER: &str = "task-runner";
const PROJECT_SERVICE_CALLER: &str = "project-service";
const MCP_MANAGEMENT_CALLER: &str = "mcp-management-service";

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
        None => {
            return Err(ApiError::unauthorized(
                "Local Connector caller is required for internal API requests",
            ));
        }
    };
    if !access.allowed_callers.contains(&caller) {
        return Err(ApiError::forbidden(
            "caller service is not allowed for this Local Connector operation",
        ));
    }

    let expected = config
        .internal_api_secrets
        .get(caller)
        .map(String::as_str)
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
            allowed_callers: &[TASK_RUNNER_CALLER, MCP_MANAGEMENT_CALLER],
        }),
        (
            &Method::POST,
            ["api", "local-connectors", "relay", _, "skills", "prepare" | "execute" | "cancel"],
        ) => Some(InternalAccess {
            scope: SKILL_RELAY_SCOPE,
            allowed_callers: &[TASK_RUNNER_CALLER],
        }),
        (
            &Method::POST,
            ["api", "local-connectors", "relay", _, "plugins", "prepare" | "execute" | "cancel"],
        ) => Some(InternalAccess {
            scope: PLUGIN_RELAY_SCOPE,
            allowed_callers: &[TASK_RUNNER_CALLER, MCP_MANAGEMENT_CALLER],
        }),
        (&Method::POST, ["api", "local-connectors", "relay", _, "plugins", "ui", "assets"]) => {
            Some(InternalAccess {
                scope: PLUGIN_UI_READ_SCOPE,
                allowed_callers: &[CHATOS_CALLER],
            })
        }
        (
            &Method::POST,
            ["api", "local-connectors", "relay", _, "plugins", "artifacts", "list" | "read"],
        ) => Some(InternalAccess {
            scope: PLUGIN_ARTIFACT_READ_SCOPE,
            allowed_callers: &[CHATOS_CALLER],
        }),
        (
            &Method::POST,
            ["api", "local-connectors", "relay", _, "plugins", "artifacts", "create" | "update"],
        ) => Some(InternalAccess {
            scope: PLUGIN_ARTIFACT_WRITE_SCOPE,
            allowed_callers: &[CHATOS_CALLER],
        }),
        (&Method::GET, ["api", "local-connectors", "sandbox-pairings"]) => Some(InternalAccess {
            scope: SANDBOX_ROUTING_READ_SCOPE,
            allowed_callers: &[
                TASK_RUNNER_CALLER,
                PROJECT_SERVICE_CALLER,
                MCP_MANAGEMENT_CALLER,
            ],
        }),
        (
            &Method::POST,
            ["api", "local-connectors", "sandbox-facade", _, "api", "local", "sandbox", "images", "mcp"],
        ) => Some(InternalAccess {
            scope: SANDBOX_SERVICE_SCOPE,
            allowed_callers: &[
                TASK_RUNNER_CALLER,
                PROJECT_SERVICE_CALLER,
                MCP_MANAGEMENT_CALLER,
            ],
        }),
        (&Method::GET, ["api", "local-connectors", "sandbox-facade", _, "api", "sandboxes", _]) => {
            Some(InternalAccess {
                scope: SANDBOX_SERVICE_SCOPE,
                allowed_callers: &[
                    TASK_RUNNER_CALLER,
                    PROJECT_SERVICE_CALLER,
                    MCP_MANAGEMENT_CALLER,
                ],
            })
        }
        (
            &Method::POST,
            ["api", "local-connectors", "sandbox-facade", _, "api", "sandboxes", _, "mcp"],
        ) => Some(InternalAccess {
            scope: SANDBOX_SERVICE_SCOPE,
            allowed_callers: &[MCP_MANAGEMENT_CALLER],
        }),
        (_, ["api", "local-connectors", "sandbox-facade", _, ..]) => Some(InternalAccess {
            scope: SANDBOX_SERVICE_SCOPE,
            allowed_callers: &[TASK_RUNNER_CALLER, PROJECT_SERVICE_CALLER],
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

pub(super) fn require_chatos_service_caller(user: &CurrentUser) -> Result<(), ApiError> {
    let owner_user_id = user.owner_user_id.as_deref().unwrap_or_default();
    if user.principal_type == "service"
        && !owner_user_id.is_empty()
        && user.user_id == format!("service:{CHATOS_CALLER}:{owner_user_id}")
    {
        return Ok(());
    }
    Err(ApiError::forbidden(
        "Plugin UI asset relay is restricted to ChatOS backend",
    ))
}

pub(super) fn require_mcp_management_service_caller(user: &CurrentUser) -> Result<(), ApiError> {
    let owner_user_id = user.owner_user_id.as_deref().unwrap_or_default();
    if user.principal_type == "service"
        && !owner_user_id.is_empty()
        && user.user_id == format!("service:{MCP_MANAGEMENT_CALLER}:{owner_user_id}")
    {
        return Ok(());
    }
    Err(ApiError::forbidden(
        "Local Sandbox MCP execution is restricted to MCP Management Service",
    ))
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
            "/api/local-connectors/devices",
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

    #[test]
    fn mcp_management_tokens_are_limited_to_mcp_plugin_and_sandbox_tool_relays() {
        let mut config = test_config();
        config.require_signed_internal_requests = true;
        config.internal_api_secrets.insert(
            MCP_MANAGEMENT_CALLER.to_string(),
            "a-long-mcp-management-local-connector-secret".to_string(),
        );
        let token = chatos_service_runtime::issue_internal_service_token(
            "a-long-mcp-management-local-connector-secret",
            MCP_MANAGEMENT_CALLER,
            TOKEN_AUDIENCE,
            MCP_RELAY_SCOPE,
            60,
        )
        .expect("issue MCP Management token");
        let headers = signed_headers(MCP_MANAGEMENT_CALLER, token.as_str());
        let user = internal_service_user_from_request(
            &config,
            &headers,
            &Method::POST,
            "/api/local-connectors/relay/device-1/mcp",
        )
        .expect("matching MCP relay request")
        .expect("service user");
        assert_eq!(user.user_id, "service:mcp-management-service:user-1");

        let plugin_token = chatos_service_runtime::issue_internal_service_token(
            "a-long-mcp-management-local-connector-secret",
            MCP_MANAGEMENT_CALLER,
            TOKEN_AUDIENCE,
            PLUGIN_RELAY_SCOPE,
            60,
        )
        .expect("issue Plugin relay token");
        let plugin_headers = signed_headers(MCP_MANAGEMENT_CALLER, plugin_token.as_str());
        let plugin_user = internal_service_user_from_request(
            &config,
            &plugin_headers,
            &Method::POST,
            "/api/local-connectors/relay/device-1/plugins/prepare",
        )
        .expect("matching Plugin relay request")
        .expect("service user");
        assert_eq!(plugin_user.owner_user_id.as_deref(), Some("user-1"));
        assert!(internal_service_user_from_request(
            &config,
            &plugin_headers,
            &Method::POST,
            "/api/local-connectors/relay/device-1/mcp",
        )
        .is_err());

        let sandbox_token = chatos_service_runtime::issue_internal_service_token(
            "a-long-mcp-management-local-connector-secret",
            MCP_MANAGEMENT_CALLER,
            TOKEN_AUDIENCE,
            SANDBOX_SERVICE_SCOPE,
            60,
        )
        .expect("issue Sandbox service token");
        let sandbox_headers = signed_headers(MCP_MANAGEMENT_CALLER, sandbox_token.as_str());
        let sandbox_user = internal_service_user_from_request(
            &config,
            &sandbox_headers,
            &Method::POST,
            "/api/local-connectors/sandbox-facade/pairing-1/api/local/sandbox/images/mcp",
        )
        .expect("matching Sandbox image facade request")
        .expect("service user");
        assert_eq!(sandbox_user.owner_user_id.as_deref(), Some("user-1"));
        for (method, path) in [
            (
                Method::GET,
                "/api/local-connectors/sandbox-facade/pairing-1/api/sandboxes/sandbox-1",
            ),
            (
                Method::POST,
                "/api/local-connectors/sandbox-facade/pairing-1/api/sandboxes/sandbox-1/mcp",
            ),
        ] {
            internal_service_user_from_request(&config, &sandbox_headers, &method, path)
                .expect("matching Local Sandbox runtime request")
                .expect("service user");
        }
        assert!(internal_service_user_from_request(
            &config,
            &sandbox_headers,
            &Method::POST,
            "/api/local-connectors/sandbox-facade/pairing-1/api/sandboxes/leases",
        )
        .is_err());

        for (method, path) in [
            (
                Method::POST,
                "/api/local-connectors/relay/device-1/terminal/exec",
            ),
            (
                Method::POST,
                "/api/local-connectors/relay/device-1/skills/execute",
            ),
        ] {
            assert!(internal_service_user_from_request(&config, &headers, &method, path).is_err());
        }
    }

    #[test]
    fn task_runner_plugin_relay_token_is_scope_and_path_bound() {
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
            PLUGIN_RELAY_SCOPE,
            60,
        )
        .expect("issue Plugin relay token");
        let headers = signed_headers(TASK_RUNNER_CALLER, token.as_str());
        let user = internal_service_user_from_request(
            &config,
            &headers,
            &Method::POST,
            "/api/local-connectors/relay/device-1/plugins/prepare",
        )
        .expect("matching Plugin request")
        .expect("service user");
        assert_eq!(user.owner_user_id.as_deref(), Some("user-1"));
        assert!(internal_service_user_from_request(
            &config,
            &headers,
            &Method::POST,
            "/api/local-connectors/relay/device-1/skills/prepare",
        )
        .is_err());
    }

    #[test]
    fn chatos_plugin_ui_read_token_is_scope_caller_and_path_bound() {
        let mut config = test_config();
        config.require_signed_internal_requests = true;
        config.internal_api_secrets.insert(
            CHATOS_CALLER.to_string(),
            "a-long-chatos-local-connector-secret".to_string(),
        );
        config.internal_api_secrets.insert(
            TASK_RUNNER_CALLER.to_string(),
            "a-long-task-runner-local-connector-secret".to_string(),
        );
        let token = chatos_service_runtime::issue_internal_service_token(
            "a-long-chatos-local-connector-secret",
            CHATOS_CALLER,
            TOKEN_AUDIENCE,
            PLUGIN_UI_READ_SCOPE,
            60,
        )
        .expect("issue Plugin UI read token");
        let headers = signed_headers(CHATOS_CALLER, token.as_str());
        let user = internal_service_user_from_request(
            &config,
            &headers,
            &Method::POST,
            "/api/local-connectors/relay/device-1/plugins/ui/assets",
        )
        .expect("matching Plugin UI asset request")
        .expect("service user");
        require_chatos_service_caller(&user).expect("ChatOS service caller");
        assert!(internal_service_user_from_request(
            &config,
            &headers,
            &Method::POST,
            "/api/local-connectors/relay/device-1/plugins/execute",
        )
        .is_err());

        let task_runner_token = chatos_service_runtime::issue_internal_service_token(
            "a-long-task-runner-local-connector-secret",
            TASK_RUNNER_CALLER,
            TOKEN_AUDIENCE,
            PLUGIN_UI_READ_SCOPE,
            60,
        )
        .expect("issue wrong-caller Plugin UI token");
        let task_runner_headers = signed_headers(TASK_RUNNER_CALLER, task_runner_token.as_str());
        assert!(internal_service_user_from_request(
            &config,
            &task_runner_headers,
            &Method::POST,
            "/api/local-connectors/relay/device-1/plugins/ui/assets",
        )
        .is_err());

        let artifact_token = chatos_service_runtime::issue_internal_service_token(
            "a-long-chatos-local-connector-secret",
            CHATOS_CALLER,
            TOKEN_AUDIENCE,
            PLUGIN_ARTIFACT_READ_SCOPE,
            60,
        )
        .expect("issue Plugin Artifact token");
        let artifact_headers = signed_headers(CHATOS_CALLER, artifact_token.as_str());
        internal_service_user_from_request(
            &config,
            &artifact_headers,
            &Method::POST,
            "/api/local-connectors/relay/device-1/plugins/artifacts/list",
        )
        .expect("matching Plugin Artifact request")
        .expect("service user");
        assert!(internal_service_user_from_request(
            &config,
            &headers,
            &Method::POST,
            "/api/local-connectors/relay/device-1/plugins/artifacts/read",
        )
        .is_err());
        assert!(internal_service_user_from_request(
            &config,
            &artifact_headers,
            &Method::POST,
            "/api/local-connectors/relay/device-1/plugins/ui/assets",
        )
        .is_err());

        let artifact_write_token = chatos_service_runtime::issue_internal_service_token(
            "a-long-chatos-local-connector-secret",
            CHATOS_CALLER,
            TOKEN_AUDIENCE,
            PLUGIN_ARTIFACT_WRITE_SCOPE,
            60,
        )
        .expect("issue Plugin Artifact write token");
        let artifact_write_headers = signed_headers(CHATOS_CALLER, artifact_write_token.as_str());
        internal_service_user_from_request(
            &config,
            &artifact_write_headers,
            &Method::POST,
            "/api/local-connectors/relay/device-1/plugins/artifacts/create",
        )
        .expect("matching Plugin Artifact write request")
        .expect("service user");
        assert!(internal_service_user_from_request(
            &config,
            &artifact_write_headers,
            &Method::POST,
            "/api/local-connectors/relay/device-1/plugins/artifacts/read",
        )
        .is_err());
        assert!(internal_service_user_from_request(
            &config,
            &artifact_headers,
            &Method::POST,
            "/api/local-connectors/relay/device-1/plugins/artifacts/update",
        )
        .is_err());

        let human = CurrentUser {
            principal_type: "human_user".to_string(),
            user_id: "user-1".to_string(),
            username: None,
            display_name: None,
            role: "user".to_string(),
            owner_user_id: None,
        };
        assert!(require_chatos_service_caller(&human).is_err());
    }

    #[test]
    fn task_runner_can_read_sandbox_routing_and_use_facade_with_scoped_tokens() {
        let mut config = test_config();
        config.require_signed_internal_requests = true;
        config.internal_api_secrets.insert(
            TASK_RUNNER_CALLER.to_string(),
            "a-long-task-runner-local-connector-secret".to_string(),
        );
        for (scope, method, path) in [
            (
                SANDBOX_ROUTING_READ_SCOPE,
                Method::GET,
                "/api/local-connectors/sandbox-pairings",
            ),
            (
                SANDBOX_SERVICE_SCOPE,
                Method::POST,
                "/api/local-connectors/sandbox-facade/pairing-1/api/sandboxes/leases",
            ),
        ] {
            let token = chatos_service_runtime::issue_internal_service_token(
                "a-long-task-runner-local-connector-secret",
                TASK_RUNNER_CALLER,
                TOKEN_AUDIENCE,
                scope,
                60,
            )
            .expect("issue token");
            let headers = signed_headers(TASK_RUNNER_CALLER, token.as_str());
            let user = internal_service_user_from_request(&config, &headers, &method, path)
                .expect("authorized request")
                .expect("service user");
            assert_eq!(user.owner_user_id.as_deref(), Some("user-1"));
        }
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
            plugin_hook_relay_request_timeout: Duration::from_secs(1),
            sandbox_image_relay_request_timeout: Duration::from_secs(1),
            public_base_url: None,
            internal_api_secrets: HashMap::new(),
            require_signed_internal_requests: false,
            require_device_connect_signature: true,
            allow_device_connect_query_token: false,
            device_connect_signature_max_skew: Duration::from_secs(300),
            active_session_lease_ttl: Duration::from_secs(90),
            managed_requirements_toml_path: None,
            managed_requirements_signing_key_path: None,
            managed_requirements_signing_key_id: None,
            managed_requirements_bundle_ttl: Duration::from_secs(3600),
        }
    }
}
