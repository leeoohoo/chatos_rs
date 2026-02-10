use axum::http::StatusCode;
use axum::{extract::Query, routing::get, Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::services::user_settings::{
    get_default_user_settings, get_effective_user_settings, patch_user_settings, save_user_settings,
};

#[derive(Debug, Deserialize)]
struct UserQuery {
    #[serde(alias = "userId")]
    user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UserSettingsRequest {
    user_id: Option<String>,
    settings: Option<Value>,
}

pub fn router() -> Router {
    Router::new().route(
        "/api/user-settings",
        get(get_settings).put(put_settings).patch(patch_settings),
    )
}

async fn get_settings(Query(query): Query<UserQuery>) -> (StatusCode, Json<Value>) {
    let user_id = query.user_id;
    if user_id.is_none() {
        let defaults = get_default_user_settings();
        return (
            StatusCode::OK,
            Json(json!({ "user_id": Value::Null, "settings": defaults, "effective": defaults })),
        );
    }
    let uid = user_id.unwrap();
    match get_effective_user_settings(Some(uid.clone())).await {
        Ok(effective) => (
            StatusCode::OK,
            Json(json!({ "user_id": uid, "settings": effective, "effective": effective })),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "获取用户设置失败", "detail": err })),
        ),
    }
}

async fn put_settings(Json(req): Json<UserSettingsRequest>) -> (StatusCode, Json<Value>) {
    if req.user_id.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "user_id 为必填项" })),
        );
    }
    let uid = req.user_id.unwrap();
    match save_user_settings(&uid, req.settings.as_ref().unwrap_or(&json!({}))).await {
        Ok(effective) => (
            StatusCode::OK,
            Json(
                json!({ "ok": true, "user_id": uid, "settings": effective, "effective": effective }),
            ),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "保存用户设置失败", "detail": err })),
        ),
    }
}

async fn patch_settings(Json(req): Json<UserSettingsRequest>) -> (StatusCode, Json<Value>) {
    if req.user_id.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "user_id 为必填项" })),
        );
    }
    let uid = req.user_id.unwrap();
    match patch_user_settings(&uid, req.settings.as_ref().unwrap_or(&json!({}))).await {
        Ok(effective) => (
            StatusCode::OK,
            Json(
                json!({ "ok": true, "user_id": uid, "settings": effective, "effective": effective }),
            ),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "更新用户设置失败", "detail": err })),
        ),
    }
}
