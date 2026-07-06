// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::FromRequestParts;
use axum::http::{header::AUTHORIZATION, request::Parts, HeaderMap, StatusCode};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::config::Config;
use crate::services::user_service_api_client;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthUser {
    pub user_id: String,
    pub role: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthHeaderError {
    MissingAuthorization,
    InvalidAuthorization,
    InvalidOrExpiredToken,
}

#[derive(Debug)]
pub enum AuthResolveError {
    InvalidPrincipal,
    ConfigUnavailable(String),
    UserServiceUnavailable(String),
}

impl AuthHeaderError {
    fn message(self) -> &'static str {
        match self {
            Self::MissingAuthorization => "缺少 Authorization",
            Self::InvalidAuthorization => "Authorization 格式错误",
            Self::InvalidOrExpiredToken => "登录状态无效或已过期",
        }
    }

    pub fn into_response(self) -> (StatusCode, Json<serde_json::Value>) {
        unauthorized(self.message())
    }
}

impl AuthResolveError {
    pub fn into_response(self) -> (StatusCode, Json<serde_json::Value>) {
        match self {
            Self::InvalidPrincipal => AuthHeaderError::InvalidOrExpiredToken.into_response(),
            Self::ConfigUnavailable(detail) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "认证配置未初始化",
                    "detail": detail
                })),
            ),
            Self::UserServiceUnavailable(detail) => (
                StatusCode::BAD_GATEWAY,
                Json(json!({
                    "error": "user_service 鉴权失败",
                    "detail": detail
                })),
            ),
        }
    }
}

impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, Json<serde_json::Value>);

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        if let Some(auth_user) = parts.extensions.get::<AuthUser>() {
            return Ok(auth_user.clone());
        }
        let access_token =
            access_token_from_headers(&parts.headers).map_err(AuthHeaderError::into_response)?;
        resolve_auth_user_via_user_service(access_token.as_str())
            .await
            .map_err(AuthResolveError::into_response)
    }
}

pub fn access_token_from_headers(headers: &HeaderMap) -> Result<String, AuthHeaderError> {
    let Some(value) = headers.get(AUTHORIZATION) else {
        return Err(AuthHeaderError::MissingAuthorization);
    };
    let Ok(raw) = value.to_str() else {
        return Err(AuthHeaderError::InvalidAuthorization);
    };
    let Some(token) = raw.strip_prefix("Bearer ").map(str::trim) else {
        return Err(AuthHeaderError::InvalidAuthorization);
    };
    access_token_from_raw(token)
}

pub fn access_token_from_raw(token: &str) -> Result<String, AuthHeaderError> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return Err(AuthHeaderError::InvalidOrExpiredToken);
    }
    Ok(trimmed.to_string())
}

pub async fn resolve_auth_user_via_user_service(
    access_token: &str,
) -> Result<AuthUser, AuthResolveError> {
    let cfg = Config::try_get().map_err(AuthResolveError::ConfigUnavailable)?;
    let base_url = cfg
        .user_service_base_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            AuthResolveError::ConfigUnavailable(
                "CHATOS_USER_SERVICE_BASE_URL is required".to_string(),
            )
        })?;
    let payload = user_service_api_client::verify_token(
        base_url,
        access_token,
        cfg.user_service_request_timeout_ms,
    )
    .await
    .map_err(AuthResolveError::UserServiceUnavailable)?;
    let principal = payload.principal;
    if principal.principal_type != "human_user" {
        return Err(AuthResolveError::InvalidPrincipal);
    }
    let user_id = principal
        .user_id
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or(AuthResolveError::InvalidPrincipal)?;
    let role = principal
        .role
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "user".to_string());
    Ok(AuthUser { user_id, role })
}

fn unauthorized(message: &str) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({
            "error": message
        })),
    )
}
