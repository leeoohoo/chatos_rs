// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde_json::Value;

use crate::config::AppConfig;

use super::{error, forbidden};

pub(super) const USER_SERVICE_TOKEN_AUDIENCE: &str = "user-service";
pub(super) const PROJECT_SERVICE_CALLER: &str = "project-service";
pub(super) const HARNESS_REPO_WRITE_SCOPE: &str = "harness.repo.write";
pub(super) const HARNESS_ACCESS_READ_SCOPE: &str = "harness.access.read";
pub(super) const MODEL_SETTINGS_READ_SCOPE: &str = "model-settings.read";
pub(super) const MODEL_RUNTIME_READ_SCOPE: &str = "model-runtime.read";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct UserServiceInternalRequestIdentity {
    pub caller_service: String,
    pub scope: String,
    pub trace_id: String,
}

pub(super) struct UserServiceInternalResourceAudit<'a> {
    pub represented_user_id: Option<&'a str>,
    pub project_id: Option<&'a str>,
    pub resource_type: &'a str,
    pub resource_id: &'a str,
    pub resource_name: Option<&'a str>,
    pub action: &'a str,
    pub outcome: &'a str,
}

pub(super) fn require_project_service_internal_request(
    config: &AppConfig,
    headers: &HeaderMap,
    required_scope: &str,
) -> Result<UserServiceInternalRequestIdentity, (StatusCode, Json<Value>)> {
    let expected = config
        .user_service_internal_api_secret
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| forbidden("project service user API secret is not configured"))?;
    verify_project_service_internal_request(headers, expected, required_scope)
}

fn verify_project_service_internal_request(
    headers: &HeaderMap,
    expected: &str,
    required_scope: &str,
) -> Result<UserServiceInternalRequestIdentity, (StatusCode, Json<Value>)> {
    let token = header_text(headers, "x-user-service-internal-token");
    let caller = match header_text(headers, "x-user-service-caller") {
        Some(caller) => caller,
        None if token.is_some() => {
            return Err(error(
                StatusCode::BAD_REQUEST,
                "user service caller is required for signed internal requests",
            ));
        }
        None => {
            return Err(unauthorized(
                "signed user service internal API token is required",
            ));
        }
    };
    if caller != PROJECT_SERVICE_CALLER {
        return Err(forbidden("user service internal caller is not allowed"));
    }
    let token =
        token.ok_or_else(|| unauthorized("signed user service internal API token is required"))?;
    let claims = chatos_service_runtime::verify_internal_service_token(
        token,
        expected,
        PROJECT_SERVICE_CALLER,
        USER_SERVICE_TOKEN_AUDIENCE,
        required_scope,
    )
    .map_err(|_| unauthorized("invalid user service internal API token"))?;
    Ok(UserServiceInternalRequestIdentity {
        caller_service: caller.to_string(),
        scope: required_scope.to_string(),
        trace_id: claims.trace_id,
    })
}

pub(super) fn record_user_service_internal_resource_access(
    identity: &UserServiceInternalRequestIdentity,
    access: UserServiceInternalResourceAudit<'_>,
) {
    let event = chatos_service_runtime::InternalResourceAccessAudit {
        caller_service: identity.caller_service.clone(),
        audience_service: USER_SERVICE_TOKEN_AUDIENCE.to_string(),
        scope: identity.scope.clone(),
        trace_id: identity.trace_id.clone(),
        represented_user_id: normalized_optional(access.represented_user_id),
        tenant_id: None,
        project_id: normalized_optional(access.project_id),
        resource_type: access.resource_type.to_string(),
        resource_id: access.resource_id.to_string(),
        resource_name: normalized_optional(access.resource_name),
        action: access.action.to_string(),
        outcome: access.outcome.to_string(),
    };
    if let Err(error) = chatos_service_runtime::record_internal_resource_access(&event) {
        tracing::error!(
            target: "chatos_internal_audit",
            trace_id = identity.trace_id.as_str(),
            error = error.as_str(),
            "user service internal resource audit validation failed"
        );
    }
}

fn normalized_optional(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn header_text<'a>(headers: &'a HeaderMap, key: &'static str) -> Option<&'a str> {
    headers
        .get(key)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn unauthorized(message: impl Into<String>) -> (StatusCode, Json<Value>) {
    error(StatusCode::UNAUTHORIZED, message)
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    #[test]
    fn signed_token_is_bound_to_project_service_audience_and_scope() {
        let token = chatos_service_runtime::issue_internal_service_token(
            "a-long-project-user-service-secret",
            PROJECT_SERVICE_CALLER,
            USER_SERVICE_TOKEN_AUDIENCE,
            HARNESS_ACCESS_READ_SCOPE,
            60,
        )
        .expect("issue token");
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-user-service-caller",
            HeaderValue::from_static(PROJECT_SERVICE_CALLER),
        );
        headers.insert(
            "x-user-service-internal-token",
            HeaderValue::from_str(token.as_str()).expect("token header"),
        );

        let identity = verify_project_service_internal_request(
            &headers,
            "a-long-project-user-service-secret",
            HARNESS_ACCESS_READ_SCOPE,
        )
        .expect("matching signed request");
        assert_eq!(identity.caller_service, PROJECT_SERVICE_CALLER);
        assert_eq!(identity.scope, HARNESS_ACCESS_READ_SCOPE);
        uuid::Uuid::parse_str(identity.trace_id.as_str()).expect("signed trace id");
        let err = verify_project_service_internal_request(
            &headers,
            "a-long-project-user-service-secret",
            MODEL_SETTINGS_READ_SCOPE,
        )
        .expect_err("scope mismatch must fail");
        assert_eq!(err.0, StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn legacy_secret_is_always_rejected() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-user-service-internal-secret",
            HeaderValue::from_static("a-long-project-user-service-secret"),
        );
        let err = verify_project_service_internal_request(
            &headers,
            "a-long-project-user-service-secret",
            HARNESS_ACCESS_READ_SCOPE,
        )
        .expect_err("legacy auth must fail in every environment");
        assert_eq!(err.0, StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn caller_is_required_when_a_signed_token_is_present() {
        let token = chatos_service_runtime::issue_internal_service_token(
            "a-long-project-user-service-secret",
            PROJECT_SERVICE_CALLER,
            USER_SERVICE_TOKEN_AUDIENCE,
            HARNESS_ACCESS_READ_SCOPE,
            60,
        )
        .expect("issue token");
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-user-service-internal-token",
            HeaderValue::from_str(token.as_str()).expect("token header"),
        );
        let err = verify_project_service_internal_request(
            &headers,
            "a-long-project-user-service-secret",
            HARNESS_ACCESS_READ_SCOPE,
        )
        .expect_err("caller is part of the signed request identity");
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
    }
}
