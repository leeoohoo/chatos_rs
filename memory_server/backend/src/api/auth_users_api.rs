use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::repositories::{auth as auth_repo, configs};

use super::{ensure_admin, require_auth, SharedState};

#[derive(Debug, Deserialize)]
pub(super) struct ListUsersQuery {
    limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(super) struct CreateUserRequest {
    username: String,
    password: String,
    role: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct UpdateUserRequest {
    password: Option<String>,
    role: Option<String>,
}

fn auth_user_json(user: &auth_repo::AuthUser) -> Value {
    json!({
        "username": user.user_id,
        "role": user.role,
        "created_at": user.created_at,
        "updated_at": user.updated_at,
    })
}

pub(super) async fn list_users(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(q): Query<ListUsersQuery>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };

    if auth.is_admin() {
        let limit = q.limit.unwrap_or(500).max(1);
        return match auth_repo::list_users(&state.pool, limit).await {
            Ok(items) => (
                StatusCode::OK,
                Json(json!({
                    "items": items
                        .into_iter()
                        .map(|u| auth_user_json(&u))
                        .collect::<Vec<Value>>()
                })),
            ),
            Err(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "list users failed", "detail": err})),
            ),
        };
    }

    match auth_repo::get_user_by_id(&state.pool, auth.user_id.as_str()).await {
        Ok(Some(user)) => (
            StatusCode::OK,
            Json(json!({ "items": [auth_user_json(&user)] })),
        ),
        Ok(None) => (StatusCode::OK, Json(json!({"items": []}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "load user failed", "detail": err})),
        ),
    }
}

fn normalize_role_input(role: Option<&str>) -> Result<String, String> {
    let role = role
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or(auth_repo::USER_ROLE)
        .to_lowercase();

    if role == auth_repo::ADMIN_ROLE || role == auth_repo::USER_ROLE {
        Ok(role)
    } else {
        Err("role only supports admin/user".to_string())
    }
}

pub(super) async fn create_user(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<CreateUserRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if let Err(err) = ensure_admin(&auth) {
        return err;
    }

    let username = req.username.trim().to_string();
    let password = req.password.trim().to_string();
    if username.is_empty() || password.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "username/password required"})),
        );
    }

    let mut role = match normalize_role_input(req.role.as_deref()) {
        Ok(v) => v,
        Err(err) => return (StatusCode::BAD_REQUEST, Json(json!({"error": err}))),
    };
    if username == auth_repo::ADMIN_USER_ID {
        role = auth_repo::ADMIN_ROLE.to_string();
    }

    match auth_repo::get_user_by_id(&state.pool, username.as_str()).await {
        Ok(Some(_)) => {
            return (
                StatusCode::CONFLICT,
                Json(json!({"error": "user already exists"})),
            )
        }
        Ok(None) => {}
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "load user failed", "detail": err})),
            )
        }
    }

    match auth_repo::create_user(
        &state.pool,
        username.as_str(),
        password.as_str(),
        role.as_str(),
    )
    .await
    {
        Ok(user) => (StatusCode::OK, Json(auth_user_json(&user))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "create user failed", "detail": err})),
        ),
    }
}

pub(super) async fn update_user(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(username): Path<String>,
    Json(req): Json<UpdateUserRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if let Err(err) = ensure_admin(&auth) {
        return err;
    }

    let target_username = username.trim().to_string();
    if target_username.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "username required"})),
        );
    }

    let password = req
        .password
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let mut role = match req.role.as_ref() {
        Some(v) => match normalize_role_input(Some(v.as_str())) {
            Ok(role) => Some(role),
            Err(err) => return (StatusCode::BAD_REQUEST, Json(json!({"error": err}))),
        },
        None => None,
    };

    if password.is_none() && role.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "nothing to update"})),
        );
    }

    if target_username == auth_repo::ADMIN_USER_ID
        && role.as_deref() != Some(auth_repo::ADMIN_ROLE)
        && role.is_some()
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "admin role cannot be changed"})),
        );
    }
    if target_username == auth_repo::ADMIN_USER_ID {
        role = Some(auth_repo::ADMIN_ROLE.to_string());
    }

    match auth_repo::update_user(
        &state.pool,
        target_username.as_str(),
        password.as_deref(),
        role.as_deref(),
    )
    .await
    {
        Ok(Some(user)) => (StatusCode::OK, Json(auth_user_json(&user))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "user not found"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "update user failed", "detail": err})),
        ),
    }
}

pub(super) async fn delete_user(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(username): Path<String>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if let Err(err) = ensure_admin(&auth) {
        return err;
    }

    let target_username = username.trim().to_string();
    if target_username.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "username required"})),
        );
    }
    if target_username == auth_repo::ADMIN_USER_ID {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "admin user cannot be deleted"})),
        );
    }
    if target_username == auth.user_id {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "cannot delete current login user"})),
        );
    }

    if let Err(err) = configs::delete_user_configs(&state.pool, target_username.as_str()).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "delete user configs failed", "detail": err})),
        );
    }

    match auth_repo::delete_user(&state.pool, target_username.as_str()).await {
        Ok(true) => (StatusCode::OK, Json(json!({"success": true}))),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "user not found"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "delete user failed", "detail": err})),
        ),
    }
}
