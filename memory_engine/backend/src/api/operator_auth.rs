// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use axum::{
    body::Body,
    extract::State,
    http::{header::AUTHORIZATION, HeaderMap, Request, StatusCode},
    middleware::Next,
    response::Response,
};
use subtle::ConstantTimeEq;

use crate::state::AppState;

use super::internal_auth::{require_internal_request, OPERATOR_SCOPE};

const BEARER_PREFIX: &str = "Bearer ";
const OPERATOR_HEADER: &str = "x-memory-operator-token";

pub async fn require_operator_auth(
    State(state): State<Arc<AppState>>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, (StatusCode, String)> {
    if require_internal_request(
        &state.config,
        request.headers(),
        OPERATOR_SCOPE,
        &[
            "chatos-backend",
            "task-runner",
            "project-service",
            "local-connector-service",
        ],
    )? {
        return Ok(next.run(request).await);
    }
    if state.config.require_signed_internal_requests {
        return Err((
            StatusCode::UNAUTHORIZED,
            "signed Memory Engine internal API token is required".to_string(),
        ));
    }
    let Some(expected_token) = state.config.operator_token.as_deref() else {
        return Err((
            StatusCode::UNAUTHORIZED,
            "Memory Engine operator auth is not configured".to_string(),
        ));
    };

    let provided = extract_operator_token(request.headers()).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            "missing operator token".to_string(),
        )
    })?;

    if !constant_time_equal(expected_token, provided) {
        return Err((
            StatusCode::UNAUTHORIZED,
            "invalid operator token".to_string(),
        ));
    }

    Ok(next.run(request).await)
}

pub(crate) fn extract_operator_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(OPERATOR_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            headers
                .get(AUTHORIZATION)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.strip_prefix(BEARER_PREFIX))
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })
}

pub(crate) fn constant_time_equal(expected: &str, provided: &str) -> bool {
    expected.as_bytes().ct_eq(provided.as_bytes()).into()
}
