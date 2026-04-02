use std::time::Duration;

use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use once_cell::sync::Lazy;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::config::AppConfig;
use crate::models::LoginRequest;

pub const ADMIN_USER_ID: &str = "admin";
pub const ADMIN_ROLE: &str = "admin";

static AUTH_HTTP: Lazy<reqwest::Client> = Lazy::new(reqwest::Client::new);

#[derive(Debug, Clone)]
pub struct AuthIdentity {
    pub user_id: String,
    pub role: String,
}

impl AuthIdentity {
    pub fn is_admin(&self) -> bool {
        self.role == ADMIN_ROLE || self.user_id == ADMIN_USER_ID
    }
}

#[derive(Debug, Deserialize)]
struct MemoryAuthLoginResponse {
    token: String,
    #[serde(alias = "username")]
    user_id: String,
    role: String,
}

#[derive(Debug, Deserialize)]
struct MemoryAuthMeResponse {
    #[serde(alias = "username")]
    user_id: String,
    role: String,
}

fn timeout_duration(config: &AppConfig) -> Duration {
    Duration::from_millis(config.memory_server_request_timeout_ms.max(300))
}

fn build_memory_url(config: &AppConfig, path: &str) -> String {
    format!(
        "{}{}",
        config.memory_server_base_url.trim_end_matches('/'),
        path
    )
}

pub async fn login_via_memory(
    config: &AppConfig,
    req: &LoginRequest,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    let username = req.username.trim();
    let password = req.password.trim();

    if username.is_empty() || password.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "username/password required"})),
        ));
    }

    let response = AUTH_HTTP
        .post(build_memory_url(config, "/auth/login"))
        .timeout(timeout_duration(config))
        .json(&json!({
            "username": username,
            "password": password,
        }))
        .send()
        .await
        .map_err(|err| {
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": "memory auth login failed", "detail": err.to_string()})),
            )
        })?;

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    let payload = serde_json::from_str::<Value>(&body).unwrap_or_else(|_| json!({ "raw": body }));

    if !status.is_success() {
        return Err((status, Json(payload)));
    }

    let parsed = serde_json::from_value::<MemoryAuthLoginResponse>(payload.clone()).map_err(|err| {
        (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": "invalid memory auth login response", "detail": err.to_string()})),
        )
    })?;

    Ok((
        StatusCode::OK,
        Json(json!({
            "token": parsed.token,
            "username": parsed.user_id,
            "role": parsed.role,
        })),
    ))
}

pub async fn require_auth(
    headers: &HeaderMap,
    config: &AppConfig,
) -> Result<AuthIdentity, (StatusCode, Json<Value>)> {
    if let Some(expected) = config.service_token.as_ref() {
        let ok = headers
            .get("x-service-token")
            .and_then(|v| v.to_str().ok())
            .map(|v| v.trim())
            == Some(expected.as_str());
        if ok {
            return Ok(AuthIdentity {
                user_id: ADMIN_USER_ID.to_string(),
                role: ADMIN_ROLE.to_string(),
            });
        }
    }

    let Some(token) = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| {
            v.strip_prefix("Bearer ")
                .or_else(|| v.strip_prefix("bearer "))
        })
        .map(str::trim)
        .filter(|v| !v.is_empty())
    else {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "unauthorized"})),
        ));
    };

    let response = AUTH_HTTP
        .get(build_memory_url(config, "/auth/me"))
        .timeout(timeout_duration(config))
        .bearer_auth(token)
        .send()
        .await
        .map_err(|err| {
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": "memory auth me failed", "detail": err.to_string()})),
            )
        })?;

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    let payload = serde_json::from_str::<Value>(&body).unwrap_or_else(|_| json!({ "raw": body }));

    if !status.is_success() {
        return Err((status, Json(payload)));
    }

    let parsed = serde_json::from_value::<MemoryAuthMeResponse>(payload).map_err(|err| {
        (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": "invalid memory auth me response", "detail": err.to_string()})),
        )
    })?;

    Ok(AuthIdentity {
        user_id: parsed.user_id,
        role: parsed.role,
    })
}

pub fn resolve_scope_user_id(auth: &AuthIdentity, requested: Option<String>) -> String {
    if auth.is_admin() {
        requested
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| auth.user_id.clone())
    } else {
        auth.user_id.clone()
    }
}
