// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::State;
use axum::http::{Method, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::auth::{bearer_token_from_headers, verify_token_via_user_service};
use crate::models::ErrorResponse;
use crate::state::AppState;

use super::internal_auth::internal_service_user_from_request;

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    message: String,
    code: Option<String>,
}

impl ApiError {
    pub fn message(&self) -> &str {
        self.message.as_str()
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
            code: None,
        }
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: message.into(),
            code: None,
        }
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            message: message.into(),
            code: None,
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
            code: None,
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
            code: None,
        }
    }

    pub fn bad_gateway(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            message: message.into(),
            code: None,
        }
    }

    pub fn service_unavailable(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            message: message.into(),
            code: None,
        }
    }

    pub fn gateway_timeout(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::GATEWAY_TIMEOUT,
            message: message.into(),
            code: None,
        }
    }

    pub fn not_implemented(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_IMPLEMENTED,
            message: message.into(),
            code: None,
        }
    }

    pub fn conflict(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            message: message.into(),
            code: Some(code.into()),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorResponse {
                error: self.message,
                code: self.code,
            }),
        )
            .into_response()
    }
}

pub(super) async fn require_auth(
    State(state): State<AppState>,
    mut request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, ApiError> {
    if request.method() == Method::OPTIONS {
        return Ok(next.run(request).await);
    }
    if let Some(user) = internal_service_user_from_request(
        &state.config,
        request.headers(),
        request.method(),
        request.uri().path(),
    )? {
        request.extensions_mut().insert(user);
        return Ok(next.run(request).await);
    }
    let token = bearer_token_from_request(&request, state.config.allow_device_connect_query_token)
        .map_err(ApiError::unauthorized)?;
    let user = verify_token_via_user_service(&state.config, token.as_str())
        .await
        .map_err(ApiError::unauthorized)?;
    request.extensions_mut().insert(user);
    Ok(next.run(request).await)
}

fn bearer_token_from_request(
    request: &Request<axum::body::Body>,
    allow_device_connect_query_token: bool,
) -> Result<String, String> {
    if let Ok(token) = bearer_token_from_headers(request.headers()) {
        return Ok(token.to_string());
    }

    let path = request.uri().path();
    let query = request.uri().query();
    if !has_legacy_query_token(query) {
        return Err("缺少登录令牌".to_string());
    }

    if !is_device_connect_path(path) {
        return Err(
            "URL query access tokens are not supported; use Authorization header".to_string(),
        );
    }

    if !allow_device_connect_query_token {
        return Err(
            "Local Connector device websocket auth must use Authorization header".to_string(),
        );
    }

    token_from_query(query).ok_or_else(|| "缺少登录令牌".to_string())
}

fn is_device_connect_path(path: &str) -> bool {
    let parts = path.trim_matches('/').split('/').collect::<Vec<_>>();
    matches!(
        parts.as_slice(),
        ["api", "local-connectors", "devices", _, "connect"]
    )
}

fn token_from_query(query: Option<&str>) -> Option<String> {
    query?.split('&').find_map(|pair| {
        let mut parts = pair.splitn(2, '=');
        let key = parts.next()?;
        let value = parts.next()?.trim();
        ((key == "access_token" || key == "token") && !value.is_empty()).then(|| value.to_string())
    })
}

fn has_legacy_query_token(query: Option<&str>) -> bool {
    query
        .into_iter()
        .flat_map(|query| query.split('&'))
        .any(|pair| {
            let key = pair.split_once('=').map_or(pair, |(key, _)| key);
            key == "access_token" || key == "token"
        })
}

#[cfg(test)]
mod tests {
    use axum::http::header::AUTHORIZATION;

    use super::*;

    fn request(uri: &str) -> Request<axum::body::Body> {
        Request::builder()
            .uri(uri)
            .body(axum::body::Body::empty())
            .expect("test request should be valid")
    }

    #[test]
    fn header_token_is_preferred_over_device_query_token() {
        let mut request =
            request("/api/local-connectors/devices/device-1/connect?access_token=query-token");
        request
            .headers_mut()
            .insert(AUTHORIZATION, "Bearer header-token".parse().unwrap());

        let token = bearer_token_from_request(&request, false).expect("header token should pass");

        assert_eq!(token, "header-token");
    }

    #[test]
    fn non_device_query_token_is_rejected() {
        let request = request("/api/local-connectors/devices?access_token=query-token");

        let error = bearer_token_from_request(&request, true).expect_err("query token must fail");

        assert_eq!(
            error,
            "URL query access tokens are not supported; use Authorization header"
        );
    }

    #[test]
    fn device_query_token_requires_compatibility_flag() {
        let request =
            request("/api/local-connectors/devices/device-1/connect?access_token=query-token");

        let error = bearer_token_from_request(&request, false).expect_err("query token must fail");

        assert_eq!(
            error,
            "Local Connector device websocket auth must use Authorization header"
        );
    }

    #[test]
    fn device_query_token_is_allowed_when_compatibility_flag_is_enabled() {
        let request = request("/api/local-connectors/devices/device-1/connect?token=query-token");

        let token = bearer_token_from_request(&request, true).expect("query token should pass");

        assert_eq!(token, "query-token");
    }
}
