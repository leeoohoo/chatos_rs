use crate::core::auth::AuthUser;
use crate::models::session::{Session, SessionService};
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

#[derive(Debug)]
pub enum SessionAccessError {
    NotFound,
    Forbidden,
    Internal(String),
}

pub fn is_owned_session(session: &Session, auth: &AuthUser) -> bool {
    session.user_id.as_deref() == Some(auth.user_id.as_str())
}

pub async fn ensure_owned_session(
    session_id: &str,
    auth: &AuthUser,
) -> Result<Session, SessionAccessError> {
    match SessionService::get_by_id(session_id).await {
        Ok(Some(session)) => {
            if is_owned_session(&session, auth) {
                Ok(session)
            } else {
                Err(SessionAccessError::Forbidden)
            }
        }
        Ok(None) => Err(SessionAccessError::NotFound),
        Err(err) => Err(SessionAccessError::Internal(err)),
    }
}

pub fn map_session_access_error(err: SessionAccessError) -> (StatusCode, Json<Value>) {
    match err {
        SessionAccessError::NotFound => {
            (StatusCode::NOT_FOUND, Json(json!({"error": "会话不存在"})))
        }
        SessionAccessError::Forbidden => (
            StatusCode::FORBIDDEN,
            Json(json!({"error": "无权访问该会话"})),
        ),
        SessionAccessError::Internal(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": err})),
        ),
    }
}

pub fn map_session_access_error_with_success(err: SessionAccessError) -> (StatusCode, Json<Value>) {
    match err {
        SessionAccessError::NotFound => (
            StatusCode::NOT_FOUND,
            Json(json!({"success": false, "error": "会话不存在"})),
        ),
        SessionAccessError::Forbidden => (
            StatusCode::FORBIDDEN,
            Json(json!({"success": false, "error": "无权访问该会话"})),
        ),
        SessionAccessError::Internal(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"success": false, "error": err})),
        ),
    }
}
