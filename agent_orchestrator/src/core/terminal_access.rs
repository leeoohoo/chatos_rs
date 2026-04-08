use crate::core::auth::AuthUser;
use crate::models::terminal::{Terminal, TerminalService};
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

#[derive(Debug)]
pub enum TerminalAccessError {
    NotFound,
    Forbidden,
    Internal(String),
}

pub fn is_owned_terminal(terminal: &Terminal, auth: &AuthUser) -> bool {
    terminal.user_id.as_deref() == Some(auth.user_id.as_str())
}

pub async fn ensure_owned_terminal(
    terminal_id: &str,
    auth: &AuthUser,
) -> Result<Terminal, TerminalAccessError> {
    match TerminalService::get_by_id(terminal_id).await {
        Ok(Some(terminal)) => {
            if is_owned_terminal(&terminal, auth) {
                Ok(terminal)
            } else {
                Err(TerminalAccessError::Forbidden)
            }
        }
        Ok(None) => Err(TerminalAccessError::NotFound),
        Err(err) => Err(TerminalAccessError::Internal(err)),
    }
}

pub fn map_terminal_access_error(err: TerminalAccessError) -> (StatusCode, Json<Value>) {
    match err {
        TerminalAccessError::NotFound => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "终端不存在" })),
        ),
        TerminalAccessError::Forbidden => (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "无权访问该终端" })),
        ),
        TerminalAccessError::Internal(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": err })),
        ),
    }
}
