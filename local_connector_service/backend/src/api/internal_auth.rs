// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::{HeaderMap, Method};

use super::ApiError;
use crate::config::AppConfig;
use crate::models::CurrentUser;

pub(super) const TOKEN_AUDIENCE: &str = "local-connector-service";
pub(super) const REMOTE_CONNECTION_RELAY_SCOPE: &str = "remote-connection.execute";
pub(super) const PLUGIN_UI_READ_SCOPE: &str = "plugin.ui.read";
pub(super) const PLUGIN_ARTIFACT_READ_SCOPE: &str = "plugin.artifact.read";
pub(super) const PLUGIN_ARTIFACT_WRITE_SCOPE: &str = "plugin.artifact.write";
pub(super) const WORKSPACE_DIRECTORY_WRITE_SCOPE: &str = "workspace.directory.write";
pub(super) const SYSTEM_STATS_READ_SCOPE: &str = "system.stats.read";

const CHATOS_CALLER: &str = "chatos-backend";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct InternalServiceRequestIdentity {
    pub caller_service: String,
    pub scope: String,
    pub trace_id: String,
    pub owner_user_id: String,
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
    let caller = caller.ok_or_else(|| {
        ApiError::bad_request("Local Connector caller is required for signed internal requests")
    })?;
    if caller != CHATOS_CALLER {
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
        user_id: format!("service:{caller}:{owner_user_id}"),
        username: Some(service_name.clone()),
        display_name: Some(service_name),
        role: "service".to_string(),
        owner_user_id: Some(owner_user_id.clone()),
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
}

fn internal_access_for_request(method: &Method, path: &str) -> Option<InternalAccess> {
    let parts = path.trim_matches('/').split('/').collect::<Vec<_>>();
    let scope = match (method, parts.as_slice()) {
        (&Method::POST, ["api", "local-connectors", "relay", _, "plugins", "ui", "assets"]) => {
            PLUGIN_UI_READ_SCOPE
        }
        (
            &Method::POST,
            ["api", "local-connectors", "relay", _, "plugins", "artifacts", "list" | "read"],
        ) => PLUGIN_ARTIFACT_READ_SCOPE,
        (
            &Method::POST,
            ["api", "local-connectors", "relay", _, "plugins", "artifacts", "create" | "update"],
        ) => PLUGIN_ARTIFACT_WRITE_SCOPE,
        (
            &Method::POST | &Method::GET,
            ["api", "local-connectors", "relay", _, "workspaces", _, "directories"],
        ) => WORKSPACE_DIRECTORY_WRITE_SCOPE,
        (&Method::POST, ["api", "local-connectors", "relay", _, "workspaces", _, "filesystem"]) => {
            WORKSPACE_DIRECTORY_WRITE_SCOPE
        }
        (
            &Method::POST,
            ["api", "local-connectors", "relay", _, "remote-connections", "test" | "command" | "sftp"],
        ) => REMOTE_CONNECTION_RELAY_SCOPE,
        (&Method::GET, ["api", "local-connectors", "system", "stats"]) => SYSTEM_STATS_READ_SCOPE,
        _ => return None,
    };
    Some(InternalAccess { scope })
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
        "this relay operation is restricted to ChatOS backend",
    ))
}

fn header_text<'a>(headers: &'a HeaderMap, key: &'static str) -> Option<&'a str> {
    headers
        .get(key)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
}
