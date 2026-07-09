// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Multipart, Path, Query, State};
use axum::http::StatusCode;
use axum::{Extension, Json};
use serde::Deserialize;

use super::access::{ensure_project_writable, require_project_access};
use super::ApiError;
use crate::auth::{AccessToken, CurrentUser};
use crate::models::{
    normalized_optional, now_rfc3339, CloudImportSource, CreateProjectRequest, ProjectImportStatus,
    ProjectProfileRecord, ProjectRecord, ProjectSourceType, ProjectStatus, UpdateProjectRequest,
    UpsertProjectProfileRequest,
};
use crate::services::cloud_import::{
    create_harness_repo_for_project, import_git_url_to_harness, import_zip_to_harness,
    HarnessProjectRepoResponse,
};
use crate::services::runtime_environment::ensure_runtime_environment_for_project;
use crate::state::AppState;

#[derive(Debug, Default, Deserialize)]
pub(in crate::api) struct ProjectListQuery {
    status: Option<ProjectStatus>,
}

pub(in crate::api) async fn list_projects(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Query(query): Query<ProjectListQuery>,
) -> Result<Json<Vec<ProjectRecord>>, ApiError> {
    state
        .store
        .list_projects(&user, query.status)
        .await
        .map(Json)
        .map_err(ApiError::bad_request)
}

pub(in crate::api) async fn create_project(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(input): Json<CreateProjectRequest>,
) -> Result<(StatusCode, Json<ProjectRecord>), ApiError> {
    let sandbox_enabled = input.sandbox_enabled;
    let project = state
        .store
        .create_project(input, &user)
        .await
        .map_err(ApiError::bad_request)?;
    ensure_runtime_environment_for_project(&state.store, &project, sandbox_enabled)
        .await
        .map_err(ApiError::bad_request)?;
    Ok((StatusCode::CREATED, Json(project)))
}

pub(in crate::api) async fn create_cloud_project(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Extension(access_token): Extension<AccessToken>,
    multipart: Multipart,
) -> Result<(StatusCode, Json<ProjectRecord>), ApiError> {
    if !state.config.cloud_project_import_enabled {
        return Err(ApiError::bad_request("cloud project import is disabled"));
    }
    let input =
        parse_cloud_project_multipart(multipart, state.config.cloud_project_max_zip_bytes).await?;
    let import_source = input.import_source()?;
    let mut project = state
        .store
        .create_project(
            CreateProjectRequest {
                name: input.name,
                root_path: None,
                git_url: None,
                description: input.description,
                source_type: Some(ProjectSourceType::Cloud),
                cloud_import_source: Some(import_source),
                import_status: Some(ProjectImportStatus::Pending),
                source_git_url: input.git_url.clone(),
                sandbox_enabled: Some(true),
            },
            &user,
        )
        .await
        .map_err(ApiError::bad_request)?;
    ensure_runtime_environment_for_project(&state.store, &project, Some(true))
        .await
        .map_err(ApiError::bad_request)?;

    project.import_status = ProjectImportStatus::Importing;
    project.import_started_at = Some(now_rfc3339());
    project.updated_at = now_rfc3339();
    state
        .store
        .save_project_record(&project)
        .await
        .map_err(ApiError::bad_request)?;

    let repo =
        match create_harness_repo_for_project(&state.config, access_token.0.as_str(), &project)
            .await
        {
            Ok(repo) => repo,
            Err(err) => {
                let failed = mark_cloud_import_failed(state.clone(), project, err).await?;
                return Ok((StatusCode::BAD_GATEWAY, Json(failed)));
            }
        };
    apply_harness_repo(&mut project, &repo);
    state
        .store
        .save_project_record(&project)
        .await
        .map_err(ApiError::bad_request)?;

    let import_result = match import_source {
        CloudImportSource::Empty => Ok(()),
        CloudImportSource::Git => {
            let git_url = input.git_url.as_deref().unwrap_or_default();
            import_git_url_to_harness(&state.config, git_url, &repo, project.id.as_str()).await
        }
        CloudImportSource::Zip => {
            let zip_bytes = input.zip_bytes.unwrap_or_default();
            import_zip_to_harness(&state.config, zip_bytes, &repo, project.id.as_str()).await
        }
        CloudImportSource::None => Ok(()),
    };

    match import_result {
        Ok(()) => {
            project.import_status = ProjectImportStatus::Ready;
            project.import_error = None;
            project.import_finished_at = Some(now_rfc3339());
            project.updated_at = now_rfc3339();
            state
                .store
                .save_project_record(&project)
                .await
                .map_err(ApiError::bad_request)?;
            Ok((StatusCode::CREATED, Json(project)))
        }
        Err(err) => {
            let failed = mark_cloud_import_failed(state, project, err).await?;
            Ok((StatusCode::BAD_GATEWAY, Json(failed)))
        }
    }
}

pub(in crate::api) async fn get_project(
    Path(project_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<ProjectRecord>, ApiError> {
    let project = require_project_access(&state, &project_id, &user).await?;
    Ok(Json(project))
}

pub(in crate::api) async fn update_project(
    Path(project_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(input): Json<UpdateProjectRequest>,
) -> Result<Json<ProjectRecord>, ApiError> {
    let project = require_project_access(&state, &project_id, &user).await?;
    ensure_project_writable(&project)?;
    let project = state
        .store
        .update_project(&project_id, input)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("项目不存在: {project_id}")))?;
    Ok(Json(project))
}

pub(in crate::api) async fn delete_project(
    Path(project_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<ProjectRecord>, ApiError> {
    let project = require_project_access(&state, &project_id, &user).await?;
    ensure_project_writable(&project)?;
    let project = state
        .store
        .archive_project(&project_id)
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("项目不存在: {project_id}")))?;
    Ok(Json(project))
}

pub(in crate::api) async fn get_project_profile(
    Path(project_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<ProjectProfileRecord>, ApiError> {
    require_project_access(&state, &project_id, &user).await?;
    let profile = state
        .store
        .get_project_profile(&project_id)
        .await
        .map_err(ApiError::bad_request)?
        .unwrap_or_else(|| {
            let now = now_rfc3339();
            ProjectProfileRecord {
                project_id,
                creator_user_id: None,
                creator_username: None,
                creator_display_name: None,
                owner_user_id: None,
                owner_username: None,
                owner_display_name: None,
                background: None,
                introduction: None,
                created_at: now.clone(),
                updated_at: now,
            }
        });
    Ok(Json(profile))
}

struct CloudProjectMultipartInput {
    name: String,
    git_url: Option<String>,
    zip_bytes: Option<Vec<u8>>,
    description: Option<String>,
}

impl CloudProjectMultipartInput {
    fn import_source(&self) -> Result<CloudImportSource, ApiError> {
        let has_git = self
            .git_url
            .as_deref()
            .map(str::trim)
            .is_some_and(|value| !value.is_empty());
        let has_zip = self
            .zip_bytes
            .as_ref()
            .is_some_and(|value| !value.is_empty());
        match (has_git, has_zip) {
            (true, true) => Err(ApiError::bad_request(
                "git_url and zip cannot both be provided",
            )),
            (true, false) => Ok(CloudImportSource::Git),
            (false, true) => Ok(CloudImportSource::Zip),
            (false, false) => Ok(CloudImportSource::Empty),
        }
    }
}

async fn parse_cloud_project_multipart(
    mut multipart: Multipart,
    max_zip_bytes: usize,
) -> Result<CloudProjectMultipartInput, ApiError> {
    let mut name = None;
    let mut git_url = None;
    let mut description = None;
    let mut zip_bytes = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|err| ApiError::bad_request(format!("invalid multipart form: {err}")))?
    {
        let field_name = field.name().unwrap_or_default().to_string();
        match field_name.as_str() {
            "name" | "project_name" => {
                name = Some(read_multipart_text(field).await?);
            }
            "git_url" | "source_git_url" => {
                git_url = normalized_optional(Some(read_multipart_text(field).await?));
            }
            "description" => {
                description = normalized_optional(Some(read_multipart_text(field).await?));
            }
            "zip" | "archive" | "file" => {
                let bytes = field.bytes().await.map_err(|err| {
                    ApiError::bad_request(format!("read zip upload failed: {err}"))
                })?;
                if bytes.len() > max_zip_bytes {
                    return Err(ApiError::bad_request(format!(
                        "zip file is too large: {} bytes > {} bytes",
                        bytes.len(),
                        max_zip_bytes
                    )));
                }
                if !bytes.is_empty() {
                    zip_bytes = Some(bytes.to_vec());
                }
            }
            _ => {
                let _ = field.bytes().await;
            }
        }
    }

    let name = normalized_optional(name)
        .ok_or_else(|| ApiError::bad_request("project name is required"))?;
    Ok(CloudProjectMultipartInput {
        name,
        git_url,
        zip_bytes,
        description,
    })
}

async fn read_multipart_text(
    field: axum::extract::multipart::Field<'_>,
) -> Result<String, ApiError> {
    field
        .text()
        .await
        .map_err(|err| ApiError::bad_request(format!("read multipart text field failed: {err}")))
}

fn apply_harness_repo(project: &mut ProjectRecord, repo: &HarnessProjectRepoResponse) {
    project.harness_space_identifier = Some(repo.space_identifier.clone());
    project.harness_repo_identifier = Some(repo.repo_identifier.clone());
    project.harness_repo_path = Some(repo.repo_path.clone());
    project.harness_git_url = Some(repo.git_url.clone());
    project.harness_git_ssh_url = repo.git_ssh_url.clone();
    project.git_url = Some(repo.git_url.clone());
    project.updated_at = now_rfc3339();
}

async fn mark_cloud_import_failed(
    state: AppState,
    mut project: ProjectRecord,
    error: String,
) -> Result<ProjectRecord, ApiError> {
    project.import_status = ProjectImportStatus::Failed;
    project.import_error = Some(error);
    project.import_finished_at = Some(now_rfc3339());
    project.updated_at = now_rfc3339();
    state
        .store
        .save_project_record(&project)
        .await
        .map_err(ApiError::bad_request)?;
    Ok(project)
}

pub(in crate::api) async fn upsert_project_profile(
    Path(project_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(input): Json<UpsertProjectProfileRequest>,
) -> Result<Json<ProjectProfileRecord>, ApiError> {
    let project = require_project_access(&state, &project_id, &user).await?;
    ensure_project_writable(&project)?;
    state
        .store
        .upsert_project_profile(&project_id, input, &user)
        .await
        .map(Json)
        .map_err(ApiError::bad_request)
}
