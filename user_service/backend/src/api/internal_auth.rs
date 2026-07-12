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

pub(super) fn require_project_service_internal_request(
    config: &AppConfig,
    headers: &HeaderMap,
    required_scope: &str,
) -> Result<(), (StatusCode, Json<Value>)> {
    let expected = config
        .user_service_internal_api_secret
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| forbidden("project service user API secret is not configured"))?;
    verify_project_service_internal_request(
        headers,
        expected,
        required_scope,
        chatos_service_runtime::is_production_environment(),
    )
}

fn verify_project_service_internal_request(
    headers: &HeaderMap,
    expected: &str,
    required_scope: &str,
    require_signed: bool,
) -> Result<(), (StatusCode, Json<Value>)> {
    let caller = header_text(headers, "x-user-service-caller");
    let token = header_text(headers, "x-user-service-internal-token");
    if let Some(caller) = caller {
        if caller != PROJECT_SERVICE_CALLER {
            return Err(forbidden("user service internal caller is not allowed"));
        }
        if let Some(token) = token {
            chatos_service_runtime::verify_internal_service_token(
                token,
                expected,
                PROJECT_SERVICE_CALLER,
                USER_SERVICE_TOKEN_AUDIENCE,
                required_scope,
            )
            .map_err(|_| unauthorized("invalid user service internal API token"))?;
            return Ok(());
        }
        if require_signed {
            return Err(unauthorized(
                "signed user service internal API token is required",
            ));
        }
    } else if token.is_some() {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "user service caller is required for signed internal requests",
        ));
    } else if require_signed {
        return Err(unauthorized(
            "signed user service internal API token is required",
        ));
    }

    let provided = header_text(headers, "x-user-service-internal-secret")
        .ok_or_else(|| unauthorized("missing user service internal secret"))?;
    if !constant_time_eq(expected.as_bytes(), provided.as_bytes()) {
        return Err(unauthorized("invalid user service internal secret"));
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

        verify_project_service_internal_request(
            &headers,
            "a-long-project-user-service-secret",
            HARNESS_ACCESS_READ_SCOPE,
            true,
        )
        .expect("matching signed request");
        let err = verify_project_service_internal_request(
            &headers,
            "a-long-project-user-service-secret",
            MODEL_SETTINGS_READ_SCOPE,
            true,
        )
        .expect_err("scope mismatch must fail");
        assert_eq!(err.0, StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn production_style_auth_rejects_legacy_secret_only() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-user-service-internal-secret",
            HeaderValue::from_static("a-long-project-user-service-secret"),
        );
        let err = verify_project_service_internal_request(
            &headers,
            "a-long-project-user-service-secret",
            HARNESS_ACCESS_READ_SCOPE,
            true,
        )
        .expect_err("legacy auth must fail");
        assert_eq!(err.0, StatusCode::UNAUTHORIZED);
    }
}
