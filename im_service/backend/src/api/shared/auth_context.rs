use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde_json::{json, Value};

use auth_core::parse_auth_token;

use crate::api::SharedState;
use crate::repositories::auth as auth_repo;
use crate::state::AppState;

#[derive(Debug, Clone)]
pub(crate) struct AuthIdentity {
    pub(crate) user_id: String,
    pub(crate) role: String,
}

impl AuthIdentity {
    pub(crate) fn is_admin(&self) -> bool {
        self.role == auth_repo::ADMIN_ROLE || self.user_id == auth_repo::ADMIN_USER_ID
    }
}

pub(crate) fn ensure_admin(auth: &AuthIdentity) -> Result<(), (StatusCode, Json<Value>)> {
    if auth.is_admin() {
        Ok(())
    } else {
        Err((StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"}))))
    }
}

pub(crate) fn require_auth(
    state: &SharedState,
    headers: &HeaderMap,
) -> Result<AuthIdentity, (StatusCode, Json<Value>)> {
    resolve_identity(headers, state.as_ref())
}

pub(crate) fn require_auth_from_access_token(
    state: &SharedState,
    access_token: &str,
) -> Result<AuthIdentity, (StatusCode, Json<Value>)> {
    let token = access_token.trim();
    if token.is_empty() {
        return Err((StatusCode::UNAUTHORIZED, Json(json!({"error": "unauthorized"}))));
    }

    let parsed =
        parse_auth_token(token, state.config.auth_secret.as_str()).ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "unauthorized"})),
            )
        })?;

    Ok(AuthIdentity {
        user_id: parsed.0,
        role: parsed.1,
    })
}

fn resolve_identity(
    headers: &HeaderMap,
    state: &AppState,
) -> Result<AuthIdentity, (StatusCode, Json<Value>)> {
    if is_valid_service_token(headers, state) {
        return Ok(AuthIdentity {
            user_id: auth_repo::ADMIN_USER_ID.to_string(),
            role: auth_repo::ADMIN_ROLE.to_string(),
        });
    }

    let token_from_header = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.trim().to_string())
        .and_then(|v| {
            v.strip_prefix("Bearer ")
                .or_else(|| v.strip_prefix("bearer "))
                .map(|s| s.trim().to_string())
        });

    let Some(token) = token_from_header else {
        return Err((StatusCode::UNAUTHORIZED, Json(json!({"error": "unauthorized"}))));
    };

    let parsed =
        parse_auth_token(token.as_str(), state.config.auth_secret.as_str()).ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "unauthorized"})),
            )
        })?;

    Ok(AuthIdentity {
        user_id: parsed.0,
        role: parsed.1,
    })
}

fn is_valid_service_token(headers: &HeaderMap, state: &AppState) -> bool {
    let Some(expected) = state.config.service_token.as_ref() else {
        return false;
    };

    headers
        .get("x-service-token")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .as_deref()
        == Some(expected.as_str())
}
