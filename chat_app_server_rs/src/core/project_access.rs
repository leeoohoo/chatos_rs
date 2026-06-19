use crate::core::auth::AuthUser;
use crate::models::project::{Project, ProjectService};
use axum::Json;
use axum::http::StatusCode;
use serde_json::{Value, json};

#[derive(Debug)]
pub enum ProjectAccessError {
    NotFound,
    Forbidden,
    Internal(String),
}

pub fn is_owned_project(project: &Project, auth: &AuthUser) -> bool {
    project.user_id.as_deref() == Some(auth.user_id.as_str())
}

pub async fn ensure_owned_project(
    project_id: &str,
    auth: &AuthUser,
) -> Result<Project, ProjectAccessError> {
    match ProjectService::get_by_id(project_id).await {
        Ok(Some(project)) => {
            if is_owned_project(&project, auth) {
                Ok(project)
            } else {
                Err(ProjectAccessError::Forbidden)
            }
        }
        Ok(None) => Err(ProjectAccessError::NotFound),
        Err(err) => Err(ProjectAccessError::Internal(err)),
    }
}

pub fn map_project_access_error(err: ProjectAccessError) -> (StatusCode, Json<Value>) {
    match err {
        ProjectAccessError::NotFound => {
            (StatusCode::NOT_FOUND, Json(json!({"error": "项目不存在"})))
        }
        ProjectAccessError::Forbidden => (
            StatusCode::FORBIDDEN,
            Json(json!({"error": "无权访问该项目"})),
        ),
        ProjectAccessError::Internal(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": err})),
        ),
    }
}
