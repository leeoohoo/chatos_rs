use axum::extract::FromRequestParts;
use axum::http::{header::AUTHORIZATION, request::Parts, HeaderMap, StatusCode};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::services::memory_server_client;

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

#[axum::async_trait]
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
        match memory_server_client::auth_me(access_token.as_str()).await {
            Ok(me) => Ok(AuthUser {
                user_id: me.user_id,
                role: me.role,
            }),
            Err(err) => {
                if err.contains("status=401") || err.contains("status=403") {
                    Err(AuthHeaderError::InvalidOrExpiredToken.into_response())
                } else {
                    Err((
                        StatusCode::BAD_GATEWAY,
                        Json(json!({
                            "error": "认证服务不可用",
                            "detail": err
                        })),
                    ))
                }
            }
        }
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

fn unauthorized(message: &str) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({
            "error": message
        })),
    )
}
