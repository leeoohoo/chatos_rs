use axum::extract::{Path, Query, State};
use axum::{Extension, Json};
use serde::Deserialize;
use serde_json::{json, Value};

use super::access::require_project_access;
use super::ApiError;
use crate::auth::CurrentUser;
use crate::services::project_plan;
use crate::state::AppState;

#[derive(Debug, Default, Deserialize)]
pub(in crate::api) struct ProjectPlanQuery {
    include_archived: Option<bool>,
}

pub(in crate::api) async fn get_project_plan(
    Path(project_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Query(query): Query<ProjectPlanQuery>,
) -> Result<Json<Value>, ApiError> {
    require_project_access(&state, &project_id, &user).await?;
    let snapshot = project_plan::project_plan_snapshot(
        &state.store,
        &project_id,
        query.include_archived.unwrap_or(false),
    )
    .await
    .map_err(ApiError::bad_request)?;
    let dependency_graph = json!(snapshot.dependency_graph);
    let work_items = json!(snapshot.work_items);
    Ok(Json(json!({
        "project_id": snapshot.project_id,
        "projectId": snapshot.project_id,
        "requirements": snapshot.requirements,
        "work_items": work_items.clone(),
        "workItems": work_items,
        "dependency_graph": dependency_graph.clone(),
        "dependencyGraph": dependency_graph,
    })))
}
