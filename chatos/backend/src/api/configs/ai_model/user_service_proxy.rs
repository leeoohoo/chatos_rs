// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::StatusCode;

use crate::config::Config;
use crate::core::auth::AuthUser;
use crate::services::access_token_scope;

pub(super) fn configured_user_service_base_url() -> Option<String> {
    Config::try_get()
        .ok()
        .and_then(|cfg| cfg.user_service_base_url.clone())
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
}

pub(super) fn user_service_timeout_ms() -> i64 {
    Config::try_get()
        .map(|cfg| cfg.user_service_request_timeout_ms)
        .unwrap_or(5000)
}

pub(super) fn user_service_access_token_for_auth(_auth: &AuthUser) -> Result<String, String> {
    access_token_scope::get_current_access_token()
        .ok_or_else(|| "current user access token is required".to_string())
}

pub(super) fn proxy_status_from_user_service_error(err: &str) -> StatusCode {
    if err.contains(" 400 ") || err.contains(": 400 ") {
        StatusCode::BAD_REQUEST
    } else if err.contains(" 401 ") || err.contains(": 401 ") {
        StatusCode::UNAUTHORIZED
    } else if err.contains(" 403 ") || err.contains(": 403 ") {
        StatusCode::FORBIDDEN
    } else if err.contains(" 404 ") || err.contains(": 404 ") {
        StatusCode::NOT_FOUND
    } else if err.contains(" 409 ") || err.contains(": 409 ") {
        StatusCode::CONFLICT
    } else {
        StatusCode::BAD_GATEWAY
    }
}
