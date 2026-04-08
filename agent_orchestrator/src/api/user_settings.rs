use axum::http::StatusCode;
use axum::{routing::get, Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::core::auth::AuthUser;
use crate::core::user_scope::resolve_user_id;
use crate::services::user_settings::{
    get_effective_user_settings, patch_user_settings, save_user_settings,
};

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

async fn get_settings(auth: AuthUser) -> (StatusCode, Json<Value>) {
    let uid = auth.user_id;
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

async fn put_settings(
    auth: AuthUser,
    Json(req): Json<UserSettingsRequest>,
) -> (StatusCode, Json<Value>) {
    let uid = match resolve_user_id(req.user_id, &auth) {
        Ok(uid) => uid,
        Err(err) => return err,
    };
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

async fn patch_settings(
    auth: AuthUser,
    Json(req): Json<UserSettingsRequest>,
) -> (StatusCode, Json<Value>) {
    let uid = match resolve_user_id(req.user_id, &auth) {
        Ok(uid) => uid,
        Err(err) => return err,
    };
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
