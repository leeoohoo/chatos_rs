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

pub(crate) fn resolve_identity(
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
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "unauthorized"})),
        ));
    };

    let parsed = parse_trusted_auth_token(token.as_str(), state).ok_or_else(|| {
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

fn parse_trusted_auth_token(token: &str, state: &AppState) -> Option<(String, String, i64)> {
    parse_auth_token(token, state.config.auth_secret.as_str()).or_else(|| {
        state
            .config
            .trusted_im_auth_secret
            .as_deref()
            .and_then(|secret| parse_auth_token(token, secret))
    })
}

pub(crate) fn require_auth(
    state: &SharedState,
    headers: &HeaderMap,
) -> Result<AuthIdentity, (StatusCode, Json<Value>)> {
    resolve_identity(headers, state.as_ref())
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

pub(crate) fn resolve_scope_user_id(
    auth: &AuthIdentity,
    requested_user_id: Option<String>,
) -> String {
    if auth.is_admin() {
        requested_user_id
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| auth.user_id.clone())
    } else {
        auth.user_id.clone()
    }
}

pub(crate) fn resolve_visible_user_ids(scope_user_id: &str) -> Vec<String> {
    let normalized = scope_user_id.trim();
    if normalized.is_empty() || normalized == auth_repo::ADMIN_USER_ID {
        return vec![auth_repo::ADMIN_USER_ID.to_string()];
    }
    vec![normalized.to_string(), auth_repo::ADMIN_USER_ID.to_string()]
}

pub(crate) fn normalize_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned)
}

pub(crate) fn normalize_project_scope_id(value: Option<&str>) -> String {
    normalize_optional_text(value).unwrap_or_else(|| "0".to_string())
}

pub(crate) fn default_project_name(project_id: &str) -> String {
    if project_id == "0" {
        "未指定项目".to_string()
    } else {
        format!("项目 {}", project_id)
    }
}

pub(crate) fn pick_latest_timestamp(candidates: &[Option<&str>]) -> Option<String> {
    let mut best: Option<&str> = None;
    for candidate in candidates.iter().flatten() {
        let trimmed = candidate.trim();
        if trimmed.is_empty() {
            continue;
        }
        match best {
            Some(current) if current >= trimmed => {}
            _ => best = Some(trimmed),
        }
    }
    best.map(ToOwned::to_owned)
}
