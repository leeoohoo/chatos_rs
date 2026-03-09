use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use crate::core::auth::AuthUser;
use crate::models::remote_connection::{RemoteConnection, RemoteConnectionService};

#[derive(Debug)]
pub enum RemoteConnectionAccessError {
    NotFound,
    Forbidden,
    Internal(String),
}

pub fn is_owned_remote_connection(connection: &RemoteConnection, auth: &AuthUser) -> bool {
    connection.user_id.as_deref() == Some(auth.user_id.as_str())
}

pub async fn ensure_owned_remote_connection(
    connection_id: &str,
    auth: &AuthUser,
) -> Result<RemoteConnection, RemoteConnectionAccessError> {
    match RemoteConnectionService::get_by_id(connection_id).await {
        Ok(Some(connection)) => {
            if is_owned_remote_connection(&connection, auth) {
                Ok(connection)
            } else {
                Err(RemoteConnectionAccessError::Forbidden)
            }
        }
        Ok(None) => Err(RemoteConnectionAccessError::NotFound),
        Err(err) => Err(RemoteConnectionAccessError::Internal(err)),
    }
}

pub fn map_remote_connection_access_error(
    err: RemoteConnectionAccessError,
) -> (StatusCode, Json<Value>) {
    match err {
        RemoteConnectionAccessError::NotFound => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "远端连接不存在", "code": "remote_connection_not_found" })),
        ),
        RemoteConnectionAccessError::Forbidden => (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "无权访问该远端连接", "code": "remote_connection_forbidden" })),
        ),
        RemoteConnectionAccessError::Internal(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": err, "code": "remote_connection_access_internal" })),
        ),
    }
}
