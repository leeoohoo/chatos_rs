use axum::{
    extract::{Path, Query},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::config::Config;
use crate::core::auth::AuthUser;
use crate::core::project_access::{ensure_owned_project, map_project_access_error};
use crate::services::{access_token_scope, project_management_api_client};

#[derive(Debug, Default, Deserialize)]
pub(super) struct ProjectPlanQuery {
    include_archived: Option<bool>,
    include_work_items: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct RequirementWorkItemsQuery {
    include_archived: Option<bool>,
    include_dependency_graph: Option<bool>,
}

pub(super) async fn get_project_plan(
    auth: AuthUser,
    Path(id): Path<String>,
    Query(query): Query<ProjectPlanQuery>,
) -> (StatusCode, Json<Value>) {
    let project = match ensure_owned_project(&id, &auth).await {
        Ok(project) => project,
        Err(err) => return map_project_access_error(err),
    };
    let cfg = match Config::try_get() {
        Ok(cfg) => cfg,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": err })),
            );
        }
    };
    let Some(access_token) = access_token_scope::get_current_access_token() else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "current user access token is required" })),
        );
    };

    let plan = match project_management_api_client::get_project_service_plan_with_options(
        cfg.project_service_base_url.as_str(),
        access_token.as_str(),
        project.id.as_str(),
        project_management_api_client::ProjectServicePlanOptions {
            include_archived: query.include_archived.unwrap_or(false),
            include_work_items: query.include_work_items,
        },
    )
    .await
    {
        Ok(plan) => plan,
        Err(err) => {
            return (StatusCode::BAD_GATEWAY, Json(json!({ "error": err })));
        }
    };
    (StatusCode::OK, Json(plan))
}

pub(super) async fn list_requirement_work_items(
    auth: AuthUser,
    Path((id, requirement_id)): Path<(String, String)>,
    Query(query): Query<RequirementWorkItemsQuery>,
) -> (StatusCode, Json<Value>) {
    let project = match ensure_owned_project(&id, &auth).await {
        Ok(project) => project,
        Err(err) => return map_project_access_error(err),
    };
    let cfg = match Config::try_get() {
        Ok(cfg) => cfg,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": err })),
            );
        }
    };
    let Some(access_token) = access_token_scope::get_current_access_token() else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "current user access token is required" })),
        );
    };

    let response = match project_management_api_client::list_project_service_requirement_work_items(
        cfg.project_service_base_url.as_str(),
        access_token.as_str(),
        project.id.as_str(),
        requirement_id.as_str(),
        query.include_archived.unwrap_or(false),
        query.include_dependency_graph.unwrap_or(false),
    )
    .await
    {
        Ok(response) => response,
        Err(err) => {
            return (StatusCode::BAD_GATEWAY, Json(json!({ "error": err })));
        }
    };
    (StatusCode::OK, Json(response))
}
