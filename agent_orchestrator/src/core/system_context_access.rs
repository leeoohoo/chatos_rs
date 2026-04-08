use crate::core::auth::AuthUser;
use crate::models::system_context::SystemContext;
use crate::repositories::system_contexts;
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

#[derive(Debug)]
pub enum SystemContextAccessError {
    NotFound,
    Forbidden,
    Internal(String),
}

pub fn is_owned_system_context(context: &SystemContext, auth: &AuthUser) -> bool {
    context.user_id == auth.user_id
}

pub async fn ensure_owned_system_context(
    context_id: &str,
    auth: &AuthUser,
) -> Result<SystemContext, SystemContextAccessError> {
    match system_contexts::get_system_context_by_id(context_id).await {
        Ok(Some(context)) => {
            if is_owned_system_context(&context, auth) {
                Ok(context)
            } else {
                Err(SystemContextAccessError::Forbidden)
            }
        }
        Ok(None) => Err(SystemContextAccessError::NotFound),
        Err(err) => Err(SystemContextAccessError::Internal(err)),
    }
}

pub fn map_system_context_access_error(err: SystemContextAccessError) -> (StatusCode, Json<Value>) {
    match err {
        SystemContextAccessError::NotFound => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "系统上下文不存在"})),
        ),
        SystemContextAccessError::Forbidden => (
            StatusCode::FORBIDDEN,
            Json(json!({"error": "无权访问该系统上下文"})),
        ),
        SystemContextAccessError::Internal(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": err})),
        ),
    }
}
