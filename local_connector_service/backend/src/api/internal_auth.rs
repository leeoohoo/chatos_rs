// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::{HeaderMap, Method};

use super::ApiError;
use crate::config::AppConfig;
use crate::models::CurrentUser;

pub(super) const TOKEN_AUDIENCE: &str = "local-connector-service";
pub(super) const MCP_RELAY_SCOPE: &str = "relay.mcp";
pub(super) const TERMINAL_RELAY_SCOPE: &str = "relay.terminal";
pub(super) const REMOTE_CONNECTION_RELAY_SCOPE: &str = "remote-connection.execute";
pub(super) const SKILL_RELAY_SCOPE: &str = "relay.skill";
pub(super) const PLUGIN_RELAY_SCOPE: &str = "plugin.execute";
pub(super) const PLUGIN_UI_READ_SCOPE: &str = "plugin.ui.read";
pub(super) const PLUGIN_ARTIFACT_READ_SCOPE: &str = "plugin.artifact.read";
pub(super) const PLUGIN_ARTIFACT_WRITE_SCOPE: &str = "plugin.artifact.write";
pub(super) const WORKSPACE_DIRECTORY_WRITE_SCOPE: &str = "workspace.directory.write";
pub(super) const SANDBOX_ROUTING_READ_SCOPE: &str = "sandbox-routing.read";
pub(super) const SANDBOX_SERVICE_SCOPE: &str = "sandbox.service";
pub(super) const SYSTEM_STATS_READ_SCOPE: &str = "system.stats.read";
pub(super) const PROJECT_CONTEXT_AUTHORIZE_SCOPE: &str = "project-context.authorize";

const CHATOS_CALLER: &str = "chatos-backend";
const MCP_MANAGEMENT_CALLER: &str = "mcp-management-service";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct InternalServiceRequestIdentity {
    pub caller_service: String,
    pub scope: String,
    pub trace_id: String,
    pub owner_user_id: String,
}

#[cfg(test)]
pub(super) fn internal_service_user_from_request(
    config: &AppConfig,
    headers: &HeaderMap,
    method: &Method,
    path: &str,
) -> Result<Option<CurrentUser>, ApiError> {
    internal_service_auth_from_request(config, headers, method, path)
        .map(|auth| auth.map(|(user, _identity)| user))
}

pub(super) fn internal_service_auth_from_request(
    config: &AppConfig,
    headers: &HeaderMap,
    method: &Method,
    path: &str,
) -> Result<Option<(CurrentUser, InternalServiceRequestIdentity)>, ApiError> {
    let caller = header_text(headers, "x-local-connector-caller");
    let token = header_text(headers, "x-local-connector-internal-token");
    if caller.is_none() && token.is_none() {
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
    let token = token.ok_or_else(|| {
        ApiError::unauthorized("signed Local Connector internal API token is required")
    })?;
    let claims = chatos_service_runtime::verify_internal_service_token(
        token,
        expected,
        caller,
        TOKEN_AUDIENCE,
        access.scope,
    )
    .map_err(|_| ApiError::unauthorized("invalid Local Connector internal API token"))?;

    let owner_user_id = claims
        .owner_user_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ApiError::unauthorized("Local Connector internal API token is missing owner user id")
        })?
        .to_string();
    if let Some(header_owner_user_id) = header_text(headers, "x-local-connector-owner-user-id")
        .or_else(|| header_text(headers, "x-chatos-owner-user-id"))
    {
        if header_owner_user_id != owner_user_id {
            return Err(ApiError::unauthorized(
                "Local Connector owner user id header does not match the signed token",
            ));
        }
    }
    let service_name = caller.replace('-', "_");
    let user = CurrentUser {
        principal_type: "service".to_string(),
        token_jti: None,
        user_id: format!("service:{caller}:{owner_user_id}"),
        username: Some(service_name.clone()),
        display_name: Some(service_name),
        role: "service".to_string(),
        owner_user_id: Some(owner_user_id.clone()),
        scopes: Vec::new(),
    };
    Ok(Some((
        user,
        InternalServiceRequestIdentity {
            caller_service: caller.to_string(),
            scope: access.scope.to_string(),
            trace_id: claims.trace_id,
            owner_user_id,
        },
    )))
}

struct InternalAccess {
    scope: &'static str,
    allowed_callers: &'static [&'static str],
}

fn internal_access_for_request(method: &Method, path: &str) -> Option<InternalAccess> {
    let parts = path.trim_matches('/').split('/').collect::<Vec<_>>();
    match (method, parts.as_slice()) {
        (&Method::POST, ["api", "local-connectors", "project-context", "authorize"]) => {
            Some(InternalAccess {
                scope: PROJECT_CONTEXT_AUTHORIZE_SCOPE,
                allowed_callers: &[MCP_MANAGEMENT_CALLER],
            })
        }
        (&Method::POST, ["api", "local-connectors", "relay", _, "mcp"]) => Some(InternalAccess {
            scope: MCP_RELAY_SCOPE,
            allowed_callers: &[MCP_MANAGEMENT_CALLER],
        }),
        (
            &Method::POST,
            ["api", "local-connectors", "relay", _, "skills", "prepare" | "execute" | "cancel"],
        ) => Some(InternalAccess {
            scope: SKILL_RELAY_SCOPE,
            allowed_callers: &[MCP_MANAGEMENT_CALLER],
        }),
        (
            &Method::POST,
            ["api", "local-connectors", "relay", _, "plugins", "prepare" | "execute" | "cancel"],
        ) => Some(InternalAccess {
            scope: PLUGIN_RELAY_SCOPE,
            allowed_callers: &[MCP_MANAGEMENT_CALLER],
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
        (
            &Method::POST,
            ["api", "local-connectors", "relay", _, "workspaces", _, "directories"],
        ) => Some(InternalAccess {
            scope: WORKSPACE_DIRECTORY_WRITE_SCOPE,
            allowed_callers: &[CHATOS_CALLER],
        }),
        (&Method::GET, ["api", "local-connectors", "relay", _, "workspaces", _, "directories"]) => {
            Some(InternalAccess {
                scope: WORKSPACE_DIRECTORY_WRITE_SCOPE,
                allowed_callers: &[CHATOS_CALLER],
            })
        }
        (&Method::POST, ["api", "local-connectors", "relay", _, "workspaces", _, "filesystem"]) => {
            Some(InternalAccess {
                scope: WORKSPACE_DIRECTORY_WRITE_SCOPE,
                allowed_callers: &[CHATOS_CALLER],
            })
        }
        (
            &Method::POST,
            ["api", "local-connectors", "relay", _, "remote-connections", "test" | "command" | "sftp"],
        ) => Some(InternalAccess {
            scope: REMOTE_CONNECTION_RELAY_SCOPE,
            allowed_callers: &[CHATOS_CALLER],
        }),
        (&Method::GET, ["api", "local-connectors", "sandbox-pairings"]) => Some(InternalAccess {
            scope: SANDBOX_ROUTING_READ_SCOPE,
            allowed_callers: &[MCP_MANAGEMENT_CALLER],
        }),
        (&Method::GET, ["api", "local-connectors", "system", "stats"]) => Some(InternalAccess {
            scope: SYSTEM_STATS_READ_SCOPE,
            allowed_callers: &[MCP_MANAGEMENT_CALLER, CHATOS_CALLER],
        }),
        (
            &Method::POST,
            ["api", "local-connectors", "sandbox-facade", _, "api", "local", "sandbox", "images", "mcp"],
        ) => Some(InternalAccess {
            scope: SANDBOX_SERVICE_SCOPE,
            allowed_callers: &[MCP_MANAGEMENT_CALLER],
        }),
        (&Method::GET, ["api", "local-connectors", "sandbox-facade", _, "api", "sandboxes", _]) => {
            Some(InternalAccess {
                scope: SANDBOX_SERVICE_SCOPE,
                allowed_callers: &[MCP_MANAGEMENT_CALLER],
            })
        }
        (
            &Method::POST,
            ["api", "local-connectors", "sandbox-facade", _, "api", "sandboxes", _, "mcp"],
        ) => Some(InternalAccess {
            scope: SANDBOX_SERVICE_SCOPE,
            allowed_callers: &[MCP_MANAGEMENT_CALLER],
        }),
        (
            &Method::POST,
            ["api", "local-connectors", "relay", _, "terminal", "exec" | "sessions" | "input"],
        )
        | (&Method::GET, ["api", "local-connectors", "relay", _, "terminal", "ws"]) => {
            Some(InternalAccess {
                scope: TERMINAL_RELAY_SCOPE,
                allowed_callers: &[MCP_MANAGEMENT_CALLER],
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

fn header_text<'a>(headers: &'a HeaderMap, key: &'static str) -> Option<&'a str> {
    headers
        .get(key)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
include!("internal_auth_inline_tests.rs");
