use crate::core::auth::AuthUser;
use crate::models::application::Application;
use crate::repositories::applications;
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

#[derive(Debug)]
pub enum ApplicationAccessError {
    NotFound,
    Forbidden,
    Internal(String),
}

pub fn is_owned_application(application: &Application, auth: &AuthUser) -> bool {
    application.user_id.as_deref() == Some(auth.user_id.as_str())
}

pub async fn ensure_owned_application(
    application_id: &str,
    auth: &AuthUser,
) -> Result<Application, ApplicationAccessError> {
    match applications::get_application_by_id(application_id).await {
        Ok(Some(application)) => {
            if is_owned_application(&application, auth) {
                Ok(application)
            } else {
                Err(ApplicationAccessError::Forbidden)
            }
        }
        Ok(None) => Err(ApplicationAccessError::NotFound),
        Err(err) => Err(ApplicationAccessError::Internal(err)),
    }
}

pub fn map_application_access_error(err: ApplicationAccessError) -> (StatusCode, Json<Value>) {
    match err {
        ApplicationAccessError::NotFound => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Application 不存在"})),
        ),
        ApplicationAccessError::Forbidden => (
            StatusCode::FORBIDDEN,
            Json(json!({"error": "无权访问该应用"})),
        ),
        ApplicationAccessError::Internal(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": err})),
        ),
    }
}
