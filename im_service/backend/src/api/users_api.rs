use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde_json::{json, Value};

use crate::models::{CreateImUserRequest, UpdateImUserRequest};
use crate::repositories::auth as auth_repo;

use super::shared::{ensure_admin, require_auth};
use super::SharedState;

pub(super) async fn list_users(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if let Err(err) = ensure_admin(&auth) {
        return err;
    }

    match auth_repo::list_users(&state.pool, 200).await {
        Ok(items) => (
            StatusCode::OK,
            Json(json!(items
                .into_iter()
                .map(|item| json!({
                    "id": item.id,
                    "username": item.username,
                    "display_name": item.display_name,
                    "avatar_url": item.avatar_url,
                    "role": item.role,
                    "status": item.status,
                    "created_at": item.created_at,
                    "updated_at": item.updated_at,
                }))
                .collect::<Vec<_>>())),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list users failed", "detail": err})),
        ),
    }
}

pub(super) async fn create_user(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<CreateImUserRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if let Err(err) = ensure_admin(&auth) {
        return err;
    }

    match auth_repo::create_user(&state.pool, req).await {
        Ok(user) => (
            StatusCode::CREATED,
            Json(json!({
                "id": user.id,
                "username": user.username,
                "display_name": user.display_name,
                "avatar_url": user.avatar_url,
                "role": user.role,
                "status": user.status,
                "created_at": user.created_at,
                "updated_at": user.updated_at,
            })),
        ),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "create user failed", "detail": err})),
        ),
    }
}

pub(super) async fn update_user(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(username): Path<String>,
    Json(req): Json<UpdateImUserRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if let Err(err) = ensure_admin(&auth) {
        return err;
    }

    match auth_repo::update_user(&state.pool, username.as_str(), req).await {
        Ok(Some(user)) => (
            StatusCode::OK,
            Json(json!({
                "id": user.id,
                "username": user.username,
                "display_name": user.display_name,
                "avatar_url": user.avatar_url,
                "role": user.role,
                "status": user.status,
                "created_at": user.created_at,
                "updated_at": user.updated_at,
            })),
        ),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "user not found"})),
        ),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "update user failed", "detail": err})),
        ),
    }
}
