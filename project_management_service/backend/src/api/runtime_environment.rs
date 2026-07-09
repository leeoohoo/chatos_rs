// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, State};
use axum::{Extension, Json};

use super::access::{ensure_project_writable, require_project_access};
use super::ApiError;
use crate::auth::{AccessToken, CurrentUser};
use crate::models::*;
use crate::services::environment_agent::analyze_project_runtime_environment;
use crate::services::runtime_environment::default_runtime_environment_for_project;
use crate::state::AppState;

pub(in crate::api) async fn get_project_runtime_environment(
    Path(project_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<ProjectRuntimeEnvironmentResponse>, ApiError> {
    let project = require_project_access(&state, &project_id, &user).await?;
    let environment = state
        .store
        .get_project_runtime_environment(&project_id)
        .await
        .map_err(ApiError::bad_request)?
        .unwrap_or_else(|| default_runtime_environment_for_project(&project, None));
    let images = state
        .store
        .list_project_runtime_environment_images(&project_id)
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(ProjectRuntimeEnvironmentResponse {
        environment,
        images,
    }))
}

pub(in crate::api) async fn update_project_runtime_environment_settings(
    Path(project_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(input): Json<UpdateProjectRuntimeEnvironmentSettingsRequest>,
) -> Result<Json<ProjectRuntimeEnvironmentResponse>, ApiError> {
    let project = require_project_access(&state, &project_id, &user).await?;
    ensure_project_writable(&project)?;
    let mut environment = state
        .store
        .get_project_runtime_environment(&project_id)
        .await
        .map_err(ApiError::bad_request)?
        .unwrap_or_else(|| default_runtime_environment_for_project(&project, None));

    if let Some(sandbox_enabled) = input.sandbox_enabled {
        environment.sandbox_enabled = sandbox_enabled;
        if sandbox_enabled {
            if environment.status == ProjectRuntimeEnvironmentStatus::Disabled {
                environment.status = ProjectRuntimeEnvironmentStatus::Pending;
            }
        } else {
            environment.status = ProjectRuntimeEnvironmentStatus::Disabled;
            environment.sandbox_provider = RuntimeEnvironmentProvider::None;
            environment.file_provider = RuntimeEnvironmentProvider::None;
            environment.last_error = None;
        }
    }
    environment.updated_at = now_rfc3339();
    let environment = state
        .store
        .upsert_project_runtime_environment(&environment)
        .await
        .map_err(ApiError::bad_request)?;
    if !environment.sandbox_enabled {
        state
            .store
            .replace_project_runtime_environment_images(&project_id, &[])
            .await
            .map_err(ApiError::bad_request)?;
    }
    let images = state
        .store
        .list_project_runtime_environment_images(&project_id)
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(ProjectRuntimeEnvironmentResponse {
        environment,
        images,
    }))
}

pub(in crate::api) async fn analyze_project_runtime_environment_handler(
    Path(project_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Extension(access_token): Extension<AccessToken>,
) -> Result<Json<ProjectRuntimeEnvironmentResponse>, ApiError> {
    let project = require_project_access(&state, &project_id, &user).await?;
    ensure_project_writable(&project)?;
    analyze_project_runtime_environment(&state, &project, Some(access_token.0.as_str()))
        .await
        .map(Json)
        .map_err(ApiError::bad_request)
}
