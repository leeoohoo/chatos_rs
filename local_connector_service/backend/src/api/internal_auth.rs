// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::{HeaderMap, Method};

use super::ApiError;
use crate::config::AppConfig;
use crate::models::CurrentUser;

pub(super) const TOKEN_AUDIENCE: &str = "local-connector-service";
pub(super) const SYSTEM_STATS_READ_SCOPE: &str = "system.stats.read";

const CHATOS_CALLER: &str = "chatos-backend";

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
    match (method, path) {
        (&Method::GET, "/api/local-connectors/system/stats") => Some(InternalAccess {
            scope: SYSTEM_STATS_READ_SCOPE,
            allowed_callers: &[CHATOS_CALLER],
        }),
        _ => None,
    }
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
