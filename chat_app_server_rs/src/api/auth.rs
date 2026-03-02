use axum::http::StatusCode;
use axum::{routing::post, Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::core::auth::{
    hash_password, normalize_email, sign_access_token, validate_password, verify_password, AuthUser,
};
use crate::models::user::{User, UserService};

#[derive(Debug, Deserialize)]
struct RegisterRequest {
    email: Option<String>,
    password: Option<String>,
    display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LoginRequest {
    email: Option<String>,
    password: Option<String>,
}

pub fn router() -> Router {
    Router::new()
        .route("/api/auth/register", post(register))
        .route("/api/auth/login", post(login))
        .route("/api/auth/me", axum::routing::get(me))
}

async fn register(Json(req): Json<RegisterRequest>) -> (StatusCode, Json<Value>) {
    let Some(raw_email) = req.email else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "email 为必填项"})),
        );
    };
    let Some(password) = req.password else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "password 为必填项"})),
        );
    };

    let Some(email) = normalize_email(&raw_email) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "email 格式不合法"})),
        );
    };
    if let Err(err) = validate_password(&password) {
        return (StatusCode::BAD_REQUEST, Json(json!({"error": err})));
    }

    match UserService::get_by_email(&email).await {
        Ok(Some(_)) => return (StatusCode::CONFLICT, Json(json!({"error": "该邮箱已注册"}))),
        Ok(None) => {}
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "注册失败", "detail": err})),
            )
        }
    }

    let password_hash = match hash_password(&password) {
        Ok(hash) => hash,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "注册失败", "detail": err})),
            )
        }
    };

    let display_name = req
        .display_name
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let user = User::new(email.clone(), password_hash, display_name);
    if let Err(err) = UserService::create(&user).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "注册失败", "detail": err})),
        );
    }

    let token = match sign_access_token(&user.id, &user.email) {
        Ok(token) => token,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "注册成功但签发 token 失败", "detail": err})),
            )
        }
    };

    (
        StatusCode::CREATED,
        Json(json!({
            "access_token": token,
            "token_type": "Bearer",
            "expires_in": crate::config::Config::get().auth_access_token_ttl_seconds,
            "user": user_public_value(&user),
        })),
    )
}

async fn login(Json(req): Json<LoginRequest>) -> (StatusCode, Json<Value>) {
    let Some(raw_email) = req.email else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "email 为必填项"})),
        );
    };
    let Some(password) = req.password else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "password 为必填项"})),
        );
    };

    let Some(email) = normalize_email(&raw_email) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "email 格式不合法"})),
        );
    };

    let user = match UserService::get_by_email(&email).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "邮箱或密码错误"})),
            )
        }
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "登录失败", "detail": err})),
            )
        }
    };

    if user.status != "active" {
        return (StatusCode::FORBIDDEN, Json(json!({"error": "账号已禁用"})));
    }

    let verified = match verify_password(&password, &user.password_hash) {
        Ok(ok) => ok,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "登录失败", "detail": err})),
            )
        }
    };
    if !verified {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "邮箱或密码错误"})),
        );
    }

    let _ = UserService::update_last_login_at(&user.id).await;
    let refreshed = UserService::get_by_id(&user.id)
        .await
        .ok()
        .flatten()
        .unwrap_or(user.clone());

    let token = match sign_access_token(&user.id, &user.email) {
        Ok(token) => token,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "登录失败", "detail": err})),
            )
        }
    };

    (
        StatusCode::OK,
        Json(json!({
            "access_token": token,
            "token_type": "Bearer",
            "expires_in": crate::config::Config::get().auth_access_token_ttl_seconds,
            "user": user_public_value(&refreshed),
        })),
    )
}

async fn me(auth: AuthUser) -> (StatusCode, Json<Value>) {
    match UserService::get_by_id(&auth.user_id).await {
        Ok(Some(user)) => (
            StatusCode::OK,
            Json(json!({ "user": user_public_value(&user) })),
        ),
        Ok(None) => (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "用户不存在"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "获取用户信息失败", "detail": err})),
        ),
    }
}

fn user_public_value(user: &User) -> Value {
    json!({
        "id": user.id,
        "email": user.email,
        "display_name": user.display_name,
        "status": user.status,
        "last_login_at": user.last_login_at,
        "created_at": user.created_at,
        "updated_at": user.updated_at,
    })
}
