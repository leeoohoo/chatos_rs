// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::{HeaderMap, StatusCode};

use crate::config::AppConfig;

pub(crate) const TOKEN_AUDIENCE: &str = "memory-engine";
pub(crate) const DATA_SCOPE: &str = "memory.data";
pub(crate) const OPERATOR_SCOPE: &str = "memory.operator";
pub(crate) const SOURCE_SCOPE: &str = "memory.source";
pub(crate) const ADMIN_SCOPE: &str = "memory.admin";

pub(crate) fn require_internal_request(
    config: &AppConfig,
    headers: &HeaderMap,
    required_scope: &str,
    allowed_callers: &[&str],
) -> Result<Option<chatos_service_runtime::InternalServiceTokenClaims>, (StatusCode, String)> {
    let caller = header_text(headers, "x-memory-caller");
    let token = header_text(headers, "x-memory-internal-token");
    if caller.is_none() && token.is_none() {
        return Ok(None);
    }
    let caller = caller.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            "Memory Engine caller is required for signed internal requests".to_string(),
        )
    })?;
    let token = token.ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            "signed Memory Engine internal API token is required".to_string(),
        )
    })?;
    if !allowed_callers.contains(&caller) {
        return Err((
            StatusCode::FORBIDDEN,
            "caller service is not allowed for this Memory Engine operation".to_string(),
        ));
    }
    let secret = config
        .internal_api_secrets
        .get(caller)
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                "Memory Engine internal API is disabled for caller".to_string(),
            )
        })?;
    let claims = chatos_service_runtime::verify_internal_service_token(
        token,
        secret,
        caller,
        TOKEN_AUDIENCE,
        required_scope,
    )
    .map_err(|_| {
        (
            StatusCode::UNAUTHORIZED,
            "invalid Memory Engine internal API token".to_string(),
        )
    })?;
    Ok(Some(claims))
}

pub(crate) fn scope_for_memory_path(path: &str) -> &'static str {
    if path.contains("/admin/sources") {
        SOURCE_SCOPE
    } else if path.contains("/admin/") {
        ADMIN_SCOPE
    } else {
        DATA_SCOPE
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
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    #[test]
    fn token_is_bound_to_caller_and_scope() {
        let mut config = test_config();
        config.internal_api_secrets.insert(
            "configuration-center".to_string(),
            "a-long-configuration-center-memory-secret".to_string(),
        );
        let token = chatos_service_runtime::issue_internal_service_token(
            "a-long-configuration-center-memory-secret",
            "configuration-center",
            TOKEN_AUDIENCE,
            OPERATOR_SCOPE,
            60,
        )
        .expect("issue token");
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-memory-caller",
            HeaderValue::from_static("configuration-center"),
        );
        headers.insert(
            "x-memory-internal-token",
            HeaderValue::from_str(token.as_str()).expect("token header"),
        );
        let claims =
            require_internal_request(&config, &headers, OPERATOR_SCOPE, &["configuration-center"])
                .expect("valid token")
                .expect("signed identity");
        assert_eq!(claims.caller, "configuration-center");
        assert!(
            require_internal_request(&config, &headers, DATA_SCOPE, &["configuration-center"])
                .is_err()
        );
    }

    fn test_config() -> AppConfig {
        let mut config = crate::config::test_app_config("postgresql://127.0.0.1:5432/test");
        config.ai_request_timeout_secs = 1;
        config
    }
}
