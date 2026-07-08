// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::{HeaderMap, StatusCode};
use axum::{routing::get, routing::post, Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::warn;

use crate::config::Config;
use crate::core::auth::{access_token_from_headers, AuthUser};
use crate::core::websocket_ticket::issue_websocket_ticket;
use crate::services::{new_user_bootstrap, user_service_api_client};

#[derive(Debug, Deserialize)]
struct LoginRequest {
    username: Option<String>,
    #[serde(alias = "email")]
    email: Option<String>,
    password: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RegisterRequest {
    username: Option<String>,
    #[serde(alias = "email")]
    email: Option<String>,
    password: Option<String>,
}

pub fn router() -> Router {
    Router::new()
        .route("/api/auth/register", post(register))
        .route("/api/auth/login", post(login))
        .route("/api/auth/me", axum::routing::get(me))
}

pub fn protected_router() -> Router {
    Router::new()
        .route("/api/auth/ws-ticket", post(issue_ws_ticket))
        .route("/api/auth/bootstrap-defaults", post(bootstrap_defaults))
        .route("/api/auth/agent-accounts", get(list_agent_accounts))
}

async fn register(Json(req): Json<RegisterRequest>) -> (StatusCode, Json<Value>) {
    match required_user_service_base_url() {
        Ok(base_url) => register_via_user_service(base_url.as_str(), req).await,
        Err(response) => response,
    }
}

async fn login(Json(req): Json<LoginRequest>) -> (StatusCode, Json<Value>) {
    match required_user_service_base_url() {
        Ok(base_url) => login_via_user_service(base_url.as_str(), req).await,
        Err(response) => response,
    }
}

async fn me(headers: HeaderMap) -> (StatusCode, Json<Value>) {
    let base_url = match required_user_service_base_url() {
        Ok(value) => value,
        Err(response) => return response,
    };
    let access_token = match access_token_from_headers(&headers) {
        Ok(token) => token,
        Err(err) => return err.into_response(),
    };
    match user_service_api_client::get_me(
        base_url.as_str(),
        access_token.as_str(),
        Config::get().user_service_request_timeout_ms,
    )
    .await
    {
        Ok(payload) => (
            StatusCode::OK,
            Json(json!({
                "user": user_public_value_from_user_service(payload.user)
            })),
        ),
        Err(err) => (
            proxy_status_from_user_service_error(err.as_str()),
            Json(json!({
                "error": "fetch user profile via user_service failed",
                "detail": err,
            })),
        ),
    }
}

async fn issue_ws_ticket(auth: AuthUser, headers: HeaderMap) -> (StatusCode, Json<Value>) {
    let access_token = match access_token_from_headers(&headers) {
        Ok(token) => token,
        Err(err) => return err.into_response(),
    };
    match issue_websocket_ticket(access_token.as_str(), &auth) {
        Ok(ticket) => (
            StatusCode::OK,
            Json(json!({
                "ticket": ticket.ticket,
                "expires_in": ticket.expires_in,
                "expires_at": ticket.expires_at,
            })),
        ),
        Err(err) => err.into_response(),
    }
}

async fn list_agent_accounts(_auth: AuthUser, headers: HeaderMap) -> (StatusCode, Json<Value>) {
    let base_url = match required_user_service_base_url() {
        Ok(value) => value,
        Err(response) => return response,
    };
    let access_token = match access_token_from_headers(&headers) {
        Ok(token) => token,
        Err(err) => return err.into_response(),
    };
    match user_service_api_client::list_agent_accounts(
        base_url.as_str(),
        access_token.as_str(),
        Config::get().user_service_request_timeout_ms,
    )
    .await
    {
        Ok(items) => (StatusCode::OK, Json(json!(items))),
        Err(err) => (
            proxy_status_from_user_service_error(err.as_str()),
            Json(json!({
                "error": "load agent accounts via user_service failed",
                "detail": err
            })),
        ),
    }
}

async fn bootstrap_defaults(auth: AuthUser, headers: HeaderMap) -> (StatusCode, Json<Value>) {
    let access_token = match access_token_from_headers(&headers) {
        Ok(token) => token,
        Err(err) => return err.into_response(),
    };
    match new_user_bootstrap::bootstrap_new_user_defaults(
        new_user_bootstrap::NewUserBootstrapInput {
            access_token,
            user_id: auth.user_id,
            username: None,
            display_name: None,
        },
    )
    .await
    {
        Ok(report) => (
            StatusCode::OK,
            Json(json!({
                "ok": true,
                "report": report,
            })),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "error": "bootstrap default workspace failed",
                "detail": err,
            })),
        ),
    }
}

fn user_public_value_from_user_service(
    user: user_service_api_client::UserServiceAuthUser,
) -> Value {
    let username = user
        .username
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(user.id.as_str())
        .to_string();
    let role = user
        .role
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("user")
        .to_string();
    json!({
        "id": user.id,
        "username": username.clone(),
        "email": username,
        "display_name": user.display_name,
        "role": role,
        "status": "active",
        "last_login_at": Value::Null,
        "created_at": Value::Null,
        "updated_at": Value::Null,
    })
}

fn configured_user_service_base_url() -> Option<String> {
    Config::try_get()
        .ok()
        .and_then(|cfg| cfg.user_service_base_url.clone())
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
}

fn required_user_service_base_url() -> Result<String, (StatusCode, Json<Value>)> {
    configured_user_service_base_url().ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "CHATOS_USER_SERVICE_BASE_URL is required"})),
        )
    })
}

async fn register_via_user_service(
    base_url: &str,
    req: RegisterRequest,
) -> (StatusCode, Json<Value>) {
    let username = req
        .username
        .or(req.email)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let password = req
        .password
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let Some(username) = username else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "username is required"})),
        );
    };
    let Some(password) = password else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "password is required"})),
        );
    };
    match user_service_api_client::register(
        base_url,
        username.as_str(),
        password.as_str(),
        Config::get().user_service_request_timeout_ms,
    )
    .await
    {
        Ok(payload) => {
            if let Err(err) = new_user_bootstrap::bootstrap_new_user_defaults(
                new_user_bootstrap::NewUserBootstrapInput {
                    access_token: payload.token.clone(),
                    user_id: payload.user.id.clone(),
                    username: payload.user.username.clone(),
                    display_name: payload.user.display_name.clone(),
                },
            )
            .await
            {
                warn!(
                    user_id = payload.user.id.as_str(),
                    username = payload.user.username.as_deref().unwrap_or_default(),
                    error = err.as_str(),
                    "bootstrap new user defaults failed"
                );
            }
            proxy_login_success_response(payload)
        }
        Err(err) => (
            proxy_status_from_user_service_error(err.as_str()),
            Json(json!({
                "error": "register via user_service failed",
                "detail": err
            })),
        ),
    }
}

async fn login_via_user_service(base_url: &str, req: LoginRequest) -> (StatusCode, Json<Value>) {
    let username = req
        .username
        .or(req.email)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let password = req
        .password
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let Some(username) = username else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "username is required"})),
        );
    };
    let Some(password) = password else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "password is required"})),
        );
    };
    match user_service_api_client::login(
        base_url,
        username.as_str(),
        password.as_str(),
        Config::get().user_service_request_timeout_ms,
    )
    .await
    {
        Ok(payload) => proxy_login_success_response(payload),
        Err(err) => (
            proxy_status_from_user_service_error(err.as_str()),
            Json(json!({
                "error": "login via user_service failed",
                "detail": err
            })),
        ),
    }
}

fn proxy_login_success_response(
    payload: user_service_api_client::UserServiceLoginResponse,
) -> (StatusCode, Json<Value>) {
    (
        StatusCode::OK,
        Json(json!({
            "access_token": payload.token,
            "token_type": "Bearer",
            "user": user_public_value_from_user_service(payload.user),
        })),
    )
}

fn proxy_status_from_user_service_error(err: &str) -> StatusCode {
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
