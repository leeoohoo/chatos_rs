// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::{HeaderMap, StatusCode};

use crate::config::AppConfig;

pub(crate) const TOKEN_AUDIENCE: &str = "memory-engine";
pub(crate) const DATA_SCOPE: &str = "memory.data";
pub(crate) const OPERATOR_SCOPE: &str = "memory.operator";
pub(crate) const SOURCE_SCOPE: &str = "memory.source";
pub(crate) const ADMIN_SCOPE: &str = "memory.admin";
pub(crate) const MODEL_PROFILE_SYNC_SCOPE: &str = "model-profile.sync";

pub(crate) fn require_internal_request(
    config: &AppConfig,
    headers: &HeaderMap,
    required_scope: &str,
    allowed_callers: &[&str],
) -> Result<bool, (StatusCode, String)> {
    let caller = header_text(headers, "x-memory-caller");
    let token = header_text(headers, "x-memory-internal-token");
    if caller.is_none() && token.is_none() {
        return Ok(false);
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
    chatos_service_runtime::verify_internal_service_token(
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
    Ok(true)
}

pub(crate) fn scope_for_memory_path(path: &str) -> &'static str {
    if path.contains("/admin/model-profiles") {
        MODEL_PROFILE_SYNC_SCOPE
    } else if path.contains("/admin/sources") {
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
    use std::collections::HashMap;

    use axum::http::HeaderValue;

    use super::*;

    #[test]
    fn token_is_bound_to_caller_and_scope() {
        let mut config = test_config();
        config.internal_api_secrets.insert(
            "task-runner".to_string(),
            "a-long-task-runner-memory-secret".to_string(),
        );
        let token = chatos_service_runtime::issue_internal_service_token(
            "a-long-task-runner-memory-secret",
            "task-runner",
            TOKEN_AUDIENCE,
            DATA_SCOPE,
            60,
        )
        .expect("issue token");
        let mut headers = HeaderMap::new();
        headers.insert("x-memory-caller", HeaderValue::from_static("task-runner"));
        headers.insert(
            "x-memory-internal-token",
            HeaderValue::from_str(token.as_str()).expect("token header"),
        );
        assert!(
            require_internal_request(&config, &headers, DATA_SCOPE, &["task-runner"])
                .expect("valid token")
        );
        assert!(
            require_internal_request(&config, &headers, OPERATOR_SCOPE, &["task-runner"]).is_err()
        );
    }

    fn test_config() -> AppConfig {
        AppConfig {
            host: "127.0.0.1".to_string(),
            port: 0,
            mongodb_uri: "mongodb://127.0.0.1/test".to_string(),
            mongodb_database: "test".to_string(),
            ai_request_timeout_secs: 1,
            openai_api_key: None,
            openai_base_url: "http://127.0.0.1".to_string(),
            openai_model: "test".to_string(),
            openai_temperature: 0.0,
            worker_enabled: false,
            worker_interval_secs: 30,
            worker_max_threads_per_tick: 1,
            worker_summary_concurrency: 1,
            worker_rollup_concurrency: 1,
            worker_subject_memory_concurrency: 1,
            worker_reconcile_concurrency: 1,
            operator_token: Some("legacy-memory-token".to_string()),
            internal_api_secrets: HashMap::new(),
            require_signed_internal_requests: false,
            user_service_base_url: "http://127.0.0.1:39190".to_string(),
            user_service_request_timeout_ms: 300,
            local_connector_service_base_url: "http://127.0.0.1:39230".to_string(),
            local_connector_service_request_timeout_ms: 300,
            local_connector_internal_api_secret: None,
        }
    }
}
