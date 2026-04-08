use axum::http::StatusCode;
use axum::{routing::post, Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::core::auth::AuthUser;
use crate::services::im_service_client;

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
    #[allow(dead_code)]
    display_name: Option<String>,
}

pub fn router() -> Router {
    Router::new()
        .route("/api/auth/register", post(register))
        .route("/api/auth/login", post(login))
        .route("/api/auth/me", axum::routing::get(me))
}

async fn register(Json(req): Json<RegisterRequest>) -> (StatusCode, Json<Value>) {
    // 统一账号体系后，register 先沿用 login 语义，后续由 IM 服务接管真正注册流程。
    login_inner(req.username, req.email, req.password).await
}

async fn login(Json(req): Json<LoginRequest>) -> (StatusCode, Json<Value>) {
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

    match im_service_client::auth_login(username.as_str(), password.as_str()).await {
        Ok(resp) => (
            StatusCode::OK,
            Json(json!({
                "access_token": resp.token,
                "token_type": "Bearer",
                "expires_in": crate::config::Config::get().auth_access_token_ttl_seconds,
                "user": user_public_value(
                    resp.username.as_str(),
                    Some(resp.display_name.as_str()),
                    resp.role.as_str(),
                    resp.status.as_str(),
                ),
            })),
        ),
        Err(err) => map_im_auth_error("登录失败", err),
    }
}

async fn me(auth: AuthUser) -> (StatusCode, Json<Value>) {
    (
        StatusCode::OK,
        Json(json!({
            "user": user_public_value(auth.user_id.as_str(), None, auth.role.as_str(), "active")
        })),
    )
}

fn user_public_value(
    user_id: &str,
    display_name: Option<&str>,
    role: &str,
    status: &str,
) -> Value {
    json!({
        "id": user_id,
        "username": user_id,
        "email": user_id,
        "display_name": display_name,
        "role": role,
        "status": status,
        "last_login_at": Value::Null,
        "created_at": Value::Null,
        "updated_at": Value::Null,
    })
}

fn map_im_auth_error(scene: &str, err: String) -> (StatusCode, Json<Value>) {
    if err.contains("status=401") {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "用户名或密码错误"})),
        );
    }
    if err.contains("status=400") {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": scene, "detail": err})),
        );
    }
    (
        StatusCode::BAD_GATEWAY,
        Json(json!({"error": "认证服务不可用", "detail": err})),
    )
}
