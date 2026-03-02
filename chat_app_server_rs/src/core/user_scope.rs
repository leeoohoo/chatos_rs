use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use crate::core::auth::AuthUser;

pub fn ensure_user_id_matches(
    user_id: Option<&str>,
    auth: &AuthUser,
) -> Result<(), (StatusCode, Json<Value>)> {
    if user_id.is_some_and(|uid| uid != auth.user_id.as_str()) {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({"error": "user_id 与登录用户不一致"})),
        ));
    }
    Ok(())
}

pub fn ensure_and_set_user_id(
    user_id: &mut Option<String>,
    auth: &AuthUser,
) -> Result<(), (StatusCode, Json<Value>)> {
    ensure_user_id_matches(user_id.as_deref(), auth)?;
    *user_id = Some(auth.user_id.clone());
    Ok(())
}

pub fn resolve_user_id(
    user_id: Option<String>,
    auth: &AuthUser,
) -> Result<String, (StatusCode, Json<Value>)> {
    ensure_user_id_matches(user_id.as_deref(), auth)?;
    Ok(user_id.unwrap_or_else(|| auth.user_id.clone()))
}
