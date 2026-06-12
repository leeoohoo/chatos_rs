use axum::http::StatusCode;
use axum::{
    extract::{Path, Query},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::core::application_access::{ensure_owned_application, map_application_access_error};
use crate::core::auth::AuthUser;
use crate::core::user_scope::resolve_user_id;
use crate::core::validation::normalize_non_empty;
use crate::models::application::Application;
use crate::repositories::applications as repo;

#[derive(Debug, Deserialize)]
struct AppQuery {
    user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CreateAppRequest {
    name: Option<String>,
    url: Option<String>,
    description: Option<String>,
    user_id: Option<String>,
    enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct UpdateAppRequest {
    name: Option<String>,
    url: Option<String>,
    description: Option<String>,
    enabled: Option<bool>,
}

pub fn router() -> Router {
    Router::new()
        .route("/", get(list_apps).post(create_app))
        .route(
            "/:application_id",
            get(get_app).put(update_app).delete(delete_app),
        )
}

async fn list_apps(
    auth: AuthUser,
    Query(query): Query<AppQuery>,
) -> (StatusCode, Json<serde_json::Value>) {
    let user_id = match resolve_user_id(query.user_id, &auth) {
        Ok(user_id) => user_id,
        Err(err) => return err,
    };
    match repo::list_applications(Some(user_id)).await {
        Ok(apps) => (
            StatusCode::OK,
            Json(serde_json::to_value(apps).unwrap_or(serde_json::Value::Null)),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "获取应用列表失败", "detail": err})),
        ),
    }
}

async fn create_app(
    auth: AuthUser,
    Json(req): Json<CreateAppRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let CreateAppRequest {
        name,
        url,
        description,
        user_id,
        enabled,
    } = req;
    let user_id = match resolve_user_id(user_id, &auth) {
        Ok(user_id) => user_id,
        Err(err) => return err,
    };
    let Some(name) = normalize_non_empty(name) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "name 和 url 为必填项"})),
        );
    };
    let Some(url) = normalize_non_empty(url) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "name 和 url 为必填项"})),
        );
    };

    let id = Uuid::new_v4().to_string();
    let app = Application {
        id,
        name,
        url,
        description,
        user_id: Some(user_id),
        enabled: enabled.unwrap_or(true),
        created_at: crate::core::time::now_rfc3339(),
        updated_at: crate::core::time::now_rfc3339(),
    };
    if let Err(err) = repo::create_application(&app).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "创建应用失败", "detail": err})),
        );
    }
    (
        StatusCode::CREATED,
        Json(serde_json::to_value(app).unwrap_or(serde_json::Value::Null)),
    )
}

async fn get_app(
    auth: AuthUser,
    Path(application_id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    match ensure_owned_application(&application_id, &auth).await {
        Ok(app) => (
            StatusCode::OK,
            Json(serde_json::to_value(app).unwrap_or(serde_json::Value::Null)),
        ),
        Err(err) => map_application_access_error(err),
    }
}

async fn update_app(
    auth: AuthUser,
    Path(application_id): Path<String>,
    Json(req): Json<UpdateAppRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    match ensure_owned_application(&application_id, &auth).await {
        Ok(mut existing) => {
            let mut update_requested = false;
            if let Some(name) = req.name {
                existing.name = name;
                update_requested = true;
            }
            if let Some(url) = req.url {
                existing.url = url;
                update_requested = true;
            }
            if let Some(desc) = req.description {
                existing.description = Some(desc);
                update_requested = true;
            }
            if let Some(enabled) = req.enabled {
                existing.enabled = enabled;
                update_requested = true;
            }
            if update_requested {
                existing.updated_at = crate::core::time::now_rfc3339();
                if let Err(err) = repo::update_application(&application_id, &existing).await {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({"error": "更新应用失败", "detail": err})),
                    );
                }
            }
            match repo::get_application_by_id(&application_id).await {
                Ok(Some(app)) => (
                    StatusCode::OK,
                    Json(serde_json::to_value(app).unwrap_or(serde_json::Value::Null)),
                ),
                Ok(None) => (StatusCode::OK, Json(serde_json::Value::Null)),
                Err(err) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "更新应用失败", "detail": err})),
                ),
            }
        }
        Err(err) => map_application_access_error(err),
    }
}

async fn delete_app(
    auth: AuthUser,
    Path(application_id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(err) = ensure_owned_application(&application_id, &auth).await {
        return map_application_access_error(err);
    }
    if let Err(err) = repo::delete_application(&application_id).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "删除应用失败", "detail": err})),
        );
    }
    (StatusCode::OK, Json(serde_json::json!({"ok": true})))
}
