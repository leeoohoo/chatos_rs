// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::State;
use axum::http::{HeaderMap, Method, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::auth::{bearer_token_from_headers, verify_token_via_user_service};
use crate::models::{CurrentUser, ErrorResponse};
use crate::state::AppState;

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: message.into(),
        }
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            message: message.into(),
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
        }
    }

    pub fn bad_gateway(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            message: message.into(),
        }
    }

    pub fn service_unavailable(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            message: message.into(),
        }
    }

    pub fn gateway_timeout(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::GATEWAY_TIMEOUT,
            message: message.into(),
        }
    }

    pub fn not_implemented(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_IMPLEMENTED,
            message: message.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorResponse {
                error: self.message,
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
    if let Some(user) = internal_service_user_from_headers(&state, request.headers())? {
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

fn internal_service_user_from_headers(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<Option<CurrentUser>, ApiError> {
    let Some(secret) = header_text(headers, "x-local-connector-internal-secret") else {
        return Ok(None);
    };
    let expected = state
        .config
        .internal_api_secret
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::unauthorized("Local Connector internal auth is not configured"))?;
    if secret != expected {
        return Err(ApiError::unauthorized(
            "Local Connector internal auth secret is invalid",
        ));
    }
    let owner_user_id = header_text(headers, "x-local-connector-owner-user-id")
        .or_else(|| header_text(headers, "x-chatos-owner-user-id"))
        .ok_or_else(|| ApiError::unauthorized("Local Connector owner user id is required"))?;
    Ok(Some(CurrentUser {
        principal_type: "service".to_string(),
        user_id: format!("task_runner:{owner_user_id}"),
        username: Some("task_runner".to_string()),
        display_name: Some("Task Runner".to_string()),
        role: "service".to_string(),
        owner_user_id: Some(owner_user_id),
    }))
}

fn bearer_token_from_request(
    request: &Request<axum::body::Body>,
    allow_device_connect_query_token: bool,
) -> Result<String, String> {
    if bearer_token_from_headers(request.headers()).is_err()
        && is_device_connect_path(request.uri().path())
        && !allow_device_connect_query_token
    {
        return Err(
            "Local Connector device websocket auth must use Authorization header".to_string(),
        );
    }
    bearer_token_from_headers(request.headers())
        .map(ToOwned::to_owned)
        .or_else(|_| {
            token_from_query(request.uri().query()).ok_or_else(|| "缺少登录令牌".to_string())
        })
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

fn header_text(headers: &HeaderMap, key: &'static str) -> Option<String> {
    headers
        .get(key)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}
