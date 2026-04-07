use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::repositories::auth as auth_repo;

use super::shared::{build_auth_token, require_auth};
use super::SharedState;

#[derive(Debug, Deserialize)]
pub(super) struct LoginRequest {
    username: String,
    password: String,
}

pub(super) async fn login(
    State(state): State<SharedState>,
    Json(req): Json<LoginRequest>,
) -> (StatusCode, Json<Value>) {
    let username = req.username.trim().to_string();
    let password = req.password.trim().to_string();

    if username.is_empty() || password.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "username/password required"})),
        );
    }

    let user = match auth_repo::verify_user_password(&state.pool, username.as_str(), password.as_str())
        .await
    {
        Ok(Some(user)) => user,
        Ok(None) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "invalid credentials"})),
            )
        }
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "login failed", "detail": err})),
            )
        }
    };

    let token = build_auth_token(
        user.username.as_str(),
        user.role.as_str(),
        state.config.auth_secret.as_str(),
        state.config.auth_token_ttl_hours,
    );

    (
        StatusCode::OK,
        Json(json!({
            "token": token,
            "username": user.username,
            "display_name": user.display_name,
            "role": user.role,
            "status": user.status
        })),
    )
}

pub(super) async fn me(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };

    let user = match auth_repo::get_user_by_username(&state.pool, auth.user_id.as_str()).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "user not found"})),
            )
        }
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "load user failed", "detail": err})),
            )
        }
    };

    (
        StatusCode::OK,
        Json(json!({
            "id": user.id,
            "username": user.username,
            "display_name": user.display_name,
            "avatar_url": user.avatar_url,
            "role": user.role,
            "status": user.status
        })),
    )
}
