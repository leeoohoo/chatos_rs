use axum::{
    extract::Path,
    http::StatusCode,
    Json,
};
use serde_json::Value;

use crate::core::auth::AuthUser;
use crate::core::project_access::{ensure_owned_project, map_project_access_error};
use crate::repositories::change_logs;

pub(super) async fn get_project_change_summary(
    auth: AuthUser,
    Path(id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let project = match ensure_owned_project(&id, &auth).await {
        Ok(project) => project,
        Err(err) => return map_project_access_error(err),
    };

    match change_logs::list_unconfirmed_project_changes(&project.id, &project.root_path).await {
        Ok(records) => {
            let summary = change_logs::summarize_project_changes(&records);
            (
                StatusCode::OK,
                Json(serde_json::to_value(summary).unwrap_or(Value::Null)),
            )
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": err})),
        ),
    }
}
