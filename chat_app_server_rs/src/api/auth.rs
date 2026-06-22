use axum::http::{HeaderMap, StatusCode};
use axum::{routing::get, routing::post, Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::config::Config;
use crate::core::auth::{access_token_from_headers, build_auth_token, AuthUser};
use crate::core::time::now_rfc3339;
use crate::core::websocket_ticket::issue_websocket_ticket;
use crate::repositories::auth_users;
use crate::services::user_service_api_client;

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
        .route("/api/auth/agent-accounts", get(list_agent_accounts))
}

async fn register(Json(req): Json<RegisterRequest>) -> (StatusCode, Json<Value>) {
    if let Some(base_url) = configured_user_service_base_url() {
        return register_via_user_service(base_url.as_str(), req).await;
    }

    let username = req
        .username
        .or(req.email)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let password = req
        .password
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());

    let Some(username) = username else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "username 为必填项"})),
        );
    };
    let Some(password) = password else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "password 为必填项"})),
        );
    };

    if username.chars().count() < 3 {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "用户名至少需要 3 个字符"})),
        );
    }

    if password.chars().count() < 6 {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "密码至少需要 6 个字符"})),
        );
    }

    let now = now_rfc3339();
    let user = auth_users::AuthUserRecord {
        user_id: username,
        password_hash: auth_users::hash_password(password.as_str()),
        role: "user".to_string(),
        created_at: now.clone(),
        updated_at: now,
    };

    match auth_users::create_user(&user).await {
        Ok(auth_users::CreateUserResult::Created) => build_login_success_response(&user),
        Ok(auth_users::CreateUserResult::AlreadyExists) => {
            (StatusCode::CONFLICT, Json(json!({"error": "用户名已存在"})))
        }
        Err(err) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({
                "error": "注册失败",
                "detail": err
            })),
        ),
    }
}

async fn login(Json(req): Json<LoginRequest>) -> (StatusCode, Json<Value>) {
    if let Some(base_url) = configured_user_service_base_url() {
        return login_via_user_service(base_url.as_str(), req).await;
    }
    login_inner(req.username, req.email, req.password).await
}

async fn login_inner(
    username: Option<String>,
    email: Option<String>,
    password: Option<String>,
) -> (StatusCode, Json<Value>) {
    let username = username
        .or(email)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let password = password
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());

    let Some(username) = username else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "username 为必填项"})),
        );
    };
    let Some(password) = password else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "password 为必填项"})),
        );
    };

    match auth_users::verify_user_password(username.as_str(), password.as_str()).await {
        Ok(Some(user)) => build_login_success_response(&user),
        Ok(None) => (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "用户名或密码错误"})),
        ),
        Err(err) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({
                "error": "登录失败",
                "detail": err
            })),
        ),
    }
}

async fn me(auth: AuthUser, headers: HeaderMap) -> (StatusCode, Json<Value>) {
    if let Some(base_url) = configured_user_service_base_url() {
        if let Ok(access_token) = access_token_from_headers(&headers) {
            match user_service_api_client::get_me(
                base_url.as_str(),
                access_token.as_str(),
                Config::get().user_service_request_timeout_ms,
            )
            .await
            {
                Ok(payload) => {
                    return (
                        StatusCode::OK,
                        Json(json!({
                            "user": user_public_value_from_user_service(payload.user)
                        })),
                    );
                }
                Err(err) => {
                    return (
                        StatusCode::BAD_GATEWAY,
                        Json(json!({
                            "error": "fetch user profile via user_service failed",
                            "detail": err,
                        })),
                    );
                }
            }
        }
    }

    (
        StatusCode::OK,
        Json(json!({
            "user": user_public_value(auth.user_id.as_str(), auth.role.as_str())
        })),
    )
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
    let Some(base_url) = configured_user_service_base_url() else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "user_service is not configured"})),
        );
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

fn user_public_value(user_id: &str, role: &str) -> Value {
    json!({
        "id": user_id,
        "username": user_id,
        "email": user_id,
        "display_name": Value::Null,
        "role": role,
        "status": "active",
        "last_login_at": Value::Null,
        "created_at": Value::Null,
        "updated_at": Value::Null,
    })
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

fn build_login_success_response(user: &auth_users::AuthUserRecord) -> (StatusCode, Json<Value>) {
    match Config::try_get() {
        Ok(cfg) => match build_auth_token(user.user_id.as_str(), user.role.as_str()) {
            Ok(token) => (
                StatusCode::OK,
                Json(json!({
                    "access_token": token,
                    "token_type": "Bearer",
                    "expires_in": cfg.auth_access_token_ttl_seconds,
                    "user": user_public_value(user.user_id.as_str(), user.role.as_str()),
                })),
            ),
            Err(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "生成登录令牌失败",
                    "detail": err
                })),
            ),
        },
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "error": "服务配置未初始化",
                "detail": err
            })),
        ),
    }
}

fn configured_user_service_base_url() -> Option<String> {
    Config::try_get()
        .ok()
        .and_then(|cfg| cfg.user_service_base_url.clone())
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
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
        Ok(payload) => proxy_login_success_response(payload),
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
