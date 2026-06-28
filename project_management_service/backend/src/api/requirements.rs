use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::{Extension, Json};
use serde::Deserialize;

use super::access::{ensure_project_writable, require_project_access, require_requirement_access};
use super::ApiError;
use crate::auth::CurrentUser;
use crate::domain::visibility::{non_archived_requirements, should_include_archived};
use crate::models::{
    now_rfc3339, CreateRequirementRequest, RequirementDocumentRecord, RequirementRecord,
    RequirementStatus, UpdateRequirementRequest, UpsertRequirementDocumentRequest,
};
use crate::state::AppState;

#[derive(Debug, Default, Deserialize)]
pub(in crate::api) struct RequirementListQuery {
    status: Option<RequirementStatus>,
    keyword: Option<String>,
    include_archived: Option<bool>,
}

pub(in crate::api) async fn list_project_requirements(
    Path(project_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Query(query): Query<RequirementListQuery>,
) -> Result<Json<Vec<RequirementRecord>>, ApiError> {
    require_project_access(&state, &project_id, &user).await?;
    let include_archived = should_include_archived(
        query.include_archived,
        matches!(query.status, Some(RequirementStatus::Archived)),
    );
    let mut requirements = state
        .store
        .list_requirements(&project_id, query.status, query.keyword)
        .await
        .map_err(ApiError::bad_request)?;
    if !include_archived {
        requirements = non_archived_requirements(requirements);
    }
    Ok(Json(requirements))
}

pub(in crate::api) async fn create_requirement(
    Path(project_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(input): Json<CreateRequirementRequest>,
) -> Result<(StatusCode, Json<RequirementRecord>), ApiError> {
    let project = require_project_access(&state, &project_id, &user).await?;
    ensure_project_writable(&project)?;
    let requirement = state
        .store
        .create_requirement(&project_id, input, &user)
        .await
        .map_err(ApiError::bad_request)?;
    Ok((StatusCode::CREATED, Json(requirement)))
}

pub(in crate::api) async fn get_requirement(
    Path(requirement_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<RequirementRecord>, ApiError> {
    let requirement = require_requirement_access(&state, &requirement_id, &user).await?;
    Ok(Json(requirement))
}

pub(in crate::api) async fn update_requirement(
    Path(requirement_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(input): Json<UpdateRequirementRequest>,
) -> Result<Json<RequirementRecord>, ApiError> {
    let requirement = require_requirement_access(&state, &requirement_id, &user).await?;
    let project = require_project_access(&state, &requirement.project_id, &user).await?;
    ensure_project_writable(&project)?;
    state
        .store
        .update_requirement(&requirement_id, input)
        .await
        .map_err(ApiError::bad_request)?
        .map(Json)
        .ok_or_else(|| ApiError::not_found(format!("需求不存在: {requirement_id}")))
}

pub(in crate::api) async fn delete_requirement(
    Path(requirement_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<RequirementRecord>, ApiError> {
    let requirement = require_requirement_access(&state, &requirement_id, &user).await?;
    let project = require_project_access(&state, &requirement.project_id, &user).await?;
    ensure_project_writable(&project)?;
    state
        .store
        .archive_requirement(&requirement_id)
        .await
        .map_err(ApiError::bad_request)?
        .map(Json)
        .ok_or_else(|| ApiError::not_found(format!("需求不存在: {requirement_id}")))
}

pub(in crate::api) async fn get_requirement_technical_overview(
    Path(requirement_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<RequirementDocumentRecord>, ApiError> {
    require_requirement_access(&state, &requirement_id, &user).await?;
    let doc = state
        .store
        .get_requirement_document(&requirement_id)
        .await
        .map_err(ApiError::bad_request)?
        .unwrap_or_else(|| {
            let now = now_rfc3339();
            RequirementDocumentRecord {
                id: String::new(),
                requirement_id,
                doc_type: "technical_overview".to_string(),
                creator_user_id: None,
                creator_username: None,
                creator_display_name: None,
                owner_user_id: None,
                owner_username: None,
                owner_display_name: None,
                title: "实现技术总体文档".to_string(),
                format: "markdown".to_string(),
                content: String::new(),
                version: 0,
                created_at: now.clone(),
                updated_at: now,
            }
        });
    Ok(Json(doc))
}

pub(in crate::api) async fn upsert_requirement_technical_overview(
    Path(requirement_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(input): Json<UpsertRequirementDocumentRequest>,
) -> Result<Json<RequirementDocumentRecord>, ApiError> {
    let requirement = require_requirement_access(&state, &requirement_id, &user).await?;
    let project = require_project_access(&state, &requirement.project_id, &user).await?;
    ensure_project_writable(&project)?;
    state
        .store
        .upsert_requirement_document(&requirement_id, input, &user)
        .await
        .map(Json)
        .map_err(ApiError::bad_request)
}
