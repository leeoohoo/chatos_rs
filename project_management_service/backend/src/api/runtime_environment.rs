// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, State};
use axum::{Extension, Json};
use uuid::Uuid;

use super::access::{ensure_project_writable, require_project_access};
use super::ApiError;
use crate::auth::{AccessToken, CurrentUser};
use crate::models::*;
use crate::services::environment_agent::{
    analyze_project_runtime_environment, generate_project_runtime_environment_image,
    get_project_runtime_environment_deployment, get_project_runtime_environment_progress,
    reconcile_stale_analysis, refresh_project_runtime_compose_config,
    restart_project_runtime_environment, start_project_runtime_environment,
    stop_project_runtime_environment,
};
use crate::services::runtime_environment::{
    apply_environment_variable_overrides, default_runtime_environment_for_project,
    enforce_project_runtime_boundary, refresh_environment_variable_values,
    replace_legacy_internal_routing_summary, runtime_environment_requires_managed_images,
};
use crate::state::AppState;

const MAX_ANALYSIS_REQUIREMENT_LENGTH: usize = 4_000;
const MAX_SELECTED_DEPENDENCIES: usize = 64;
const MAX_SELECTED_DEPENDENCY_LENGTH: usize = 80;

pub(in crate::api) async fn get_project_runtime_environment(
    Path(project_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<ProjectRuntimeEnvironmentResponse>, ApiError> {
    let project = require_project_access(&state, &project_id, &user).await?;
    let mut environment = state
        .store
        .get_project_runtime_environment(&project_id)
        .await
        .map_err(ApiError::bad_request)?
        .unwrap_or_else(|| default_runtime_environment_for_project(&project, None));
    let mut environment_changed = reconcile_stale_analysis(
        &mut environment,
        state.config.environment_analysis_stale_after,
    );
    refresh_environment_variable_values(&mut environment);
    let mut images = state
        .store
        .list_project_runtime_environment_images(&project_id)
        .await
        .map_err(ApiError::bad_request)?;
    environment_changed |=
        replace_legacy_internal_routing_summary(&mut environment, images.as_slice());
    if enforce_project_runtime_boundary(&project, &mut environment, &mut images) {
        environment_changed = true;
    }
    if refresh_project_runtime_compose_config(&project_id, &mut environment, images.as_slice())
        .map_err(ApiError::bad_request)?
    {
        environment.analysis_summary = Some(
            crate::services::runtime_environment::program_generated_runtime_analysis_summary(
                &environment,
                images.as_slice(),
            ),
        );
        environment.updated_at = now_rfc3339();
        environment_changed = true;
    }
    if environment_changed {
        environment = state
            .store
            .upsert_project_runtime_environment(&environment)
            .await
            .map_err(ApiError::bad_request)?;
        images = state
            .store
            .replace_project_runtime_environment_images(&project_id, images.as_slice())
            .await
            .map_err(ApiError::bad_request)?;
    }
    Ok(Json(ProjectRuntimeEnvironmentResponse {
        environment,
        images,
    }))
}

pub(in crate::api) async fn update_project_runtime_environment_variables(
    Path(project_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(input): Json<UpdateProjectRuntimeEnvironmentVariablesRequest>,
) -> Result<Json<ProjectRuntimeEnvironmentResponse>, ApiError> {
    let project = require_project_access(&state, &project_id, &user).await?;
    ensure_project_writable(&project)?;
    let mut environment = state
        .store
        .get_project_runtime_environment(&project_id)
        .await
        .map_err(ApiError::bad_request)?
        .unwrap_or_else(|| default_runtime_environment_for_project(&project, None));
    apply_environment_variable_overrides(&mut environment, input.variables)
        .map_err(ApiError::bad_request)?;
    let images = state
        .store
        .list_project_runtime_environment_images(&project_id)
        .await
        .map_err(ApiError::bad_request)?;
    if environment.status == ProjectRuntimeEnvironmentStatus::PendingConfiguration
        && crate::services::runtime_environment::required_environment_variables_are_complete(
            &environment.environment_variables,
        )
        && (!runtime_environment_requires_managed_images(&environment)
            || images
                .iter()
                .filter(|image| {
                    crate::services::runtime_environment::runtime_image_is_execution_required(image)
                })
                .all(|image| {
                    matches!(
                        image.status.trim().to_ascii_lowercase().as_str(),
                        "ready" | "available" | "local" | "succeeded" | "running"
                    )
                }))
    {
        environment.status = ProjectRuntimeEnvironmentStatus::Ready;
        environment.last_error = None;
    }
    environment.analysis_summary = Some(
        crate::services::runtime_environment::program_generated_runtime_analysis_summary(
            &environment,
            images.as_slice(),
        ),
    );
    environment.updated_at = now_rfc3339();
    let environment = state
        .store
        .upsert_project_runtime_environment(&environment)
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(ProjectRuntimeEnvironmentResponse {
        environment,
        images,
    }))
}

pub(in crate::api) async fn get_project_runtime_environment_progress_handler(
    Path(project_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Extension(access_token): Extension<AccessToken>,
) -> Result<Json<ProjectRuntimeEnvironmentProgressResponse>, ApiError> {
    let project = require_project_access(&state, &project_id, &user).await?;
    get_project_runtime_environment_progress(&state, &project, Some(access_token.0.as_str()))
        .await
        .map(Json)
        .map_err(ApiError::bad_gateway)
}

pub(in crate::api) async fn generate_project_runtime_environment_image_handler(
    Path((project_id, image_record_id)): Path<(String, String)>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Extension(access_token): Extension<AccessToken>,
) -> Result<Json<ProjectRuntimeEnvironmentResponse>, ApiError> {
    let project = require_project_access(&state, &project_id, &user).await?;
    ensure_project_writable(&project)?;
    generate_project_runtime_environment_image(
        &state,
        &project,
        Some(access_token.0.as_str()),
        image_record_id.as_str(),
    )
    .await
    .map(Json)
    .map_err(ApiError::bad_gateway)
}

pub(in crate::api) async fn start_project_runtime_environment_handler(
    Path(project_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Extension(access_token): Extension<AccessToken>,
) -> Result<Json<ProjectRuntimeEnvironmentResponse>, ApiError> {
    let project = require_project_access(&state, &project_id, &user).await?;
    ensure_project_writable(&project)?;
    start_project_runtime_environment(&state, &project, Some(access_token.0.as_str()))
        .await
        .map(Json)
        .map_err(ApiError::bad_gateway)
}

pub(in crate::api) async fn get_project_runtime_environment_deployment_handler(
    Path(project_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Extension(access_token): Extension<AccessToken>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let project = require_project_access(&state, &project_id, &user).await?;
    get_project_runtime_environment_deployment(&state, &project, Some(access_token.0.as_str()))
        .await
        .map(Json)
        .map_err(ApiError::bad_gateway)
}

pub(in crate::api) async fn stop_project_runtime_environment_handler(
    Path(project_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Extension(access_token): Extension<AccessToken>,
) -> Result<Json<ProjectRuntimeEnvironmentResponse>, ApiError> {
    let project = require_project_access(&state, &project_id, &user).await?;
    ensure_project_writable(&project)?;
    stop_project_runtime_environment(&state, &project, Some(access_token.0.as_str()))
        .await
        .map(Json)
        .map_err(ApiError::bad_gateway)
}

pub(in crate::api) async fn restart_project_runtime_environment_handler(
    Path(project_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Extension(access_token): Extension<AccessToken>,
) -> Result<Json<ProjectRuntimeEnvironmentResponse>, ApiError> {
    let project = require_project_access(&state, &project_id, &user).await?;
    ensure_project_writable(&project)?;
    restart_project_runtime_environment(&state, &project, Some(access_token.0.as_str()))
        .await
        .map(Json)
        .map_err(ApiError::bad_gateway)
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
    refresh_environment_variable_values(&mut environment);

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
    payload: Option<Json<AnalyzeProjectRuntimeEnvironmentRequest>>,
) -> Result<Json<ProjectRuntimeEnvironmentResponse>, ApiError> {
    let project = require_project_access(&state, &project_id, &user).await?;
    ensure_project_writable(&project)?;
    let payload = payload.map(|Json(payload)| payload).unwrap_or_default();
    let analysis_requirement = normalize_analysis_requirement(payload.analysis_requirement)?;
    let selected_dependencies = normalize_selected_dependencies(payload.selected_dependencies)?;
    let prefer_china_mirrors = payload.prefer_china_mirrors;

    if let Some(environment) = state
        .store
        .get_project_runtime_environment(&project_id)
        .await
        .map_err(ApiError::bad_request)?
        .filter(|environment| environment.status == ProjectRuntimeEnvironmentStatus::Analyzing)
    {
        let images = state
            .store
            .list_project_runtime_environment_images(&project_id)
            .await
            .map_err(ApiError::bad_request)?;
        return Ok(Json(ProjectRuntimeEnvironmentResponse {
            environment,
            images,
        }));
    }

    let run_id = format!("project_env_agent_{}", Uuid::new_v4());
    analyze_project_runtime_environment(
        &state,
        &project,
        Some(access_token.0.as_str()),
        run_id.as_str(),
        analysis_requirement.as_deref(),
        selected_dependencies.as_slice(),
        prefer_china_mirrors,
    )
    .await
    .map(Json)
    .map_err(ApiError::bad_gateway)
}

fn normalize_analysis_requirement(value: Option<String>) -> Result<Option<String>, ApiError> {
    let requirement = value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if requirement
        .as_deref()
        .is_some_and(|value| value.chars().count() > MAX_ANALYSIS_REQUIREMENT_LENGTH)
    {
        return Err(ApiError::bad_request(format!(
            "analysis_requirement must not exceed {MAX_ANALYSIS_REQUIREMENT_LENGTH} characters"
        )));
    }
    Ok(requirement)
}

fn normalize_selected_dependencies(values: Vec<String>) -> Result<Vec<String>, ApiError> {
    let mut normalized = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for value in values {
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        if value.chars().count() > MAX_SELECTED_DEPENDENCY_LENGTH {
            return Err(ApiError::bad_request(format!(
                "selected_dependencies entries must not exceed {MAX_SELECTED_DEPENDENCY_LENGTH} characters"
            )));
        }
        if seen.insert(value.to_lowercase()) {
            normalized.push(value.to_string());
        }
    }
    if normalized.len() > MAX_SELECTED_DEPENDENCIES {
        return Err(ApiError::bad_request(format!(
            "selected_dependencies must not contain more than {MAX_SELECTED_DEPENDENCIES} entries"
        )));
    }
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::{normalize_analysis_requirement, normalize_selected_dependencies};

    #[test]
    fn analysis_requirement_is_trimmed_and_length_limited() {
        assert_eq!(
            normalize_analysis_requirement(Some("  Use Node.js 22  ".to_string()))
                .expect("valid requirement")
                .as_deref(),
            Some("Use Node.js 22")
        );
        assert!(normalize_analysis_requirement(Some("x".repeat(4_001))).is_err());
    }

    #[test]
    fn selected_dependencies_are_trimmed_deduplicated_and_limited() {
        assert_eq!(
            normalize_selected_dependencies(vec![
                " PostgreSQL ".to_string(),
                "redis".to_string(),
                "REDIS".to_string(),
                " ".to_string(),
            ])
            .expect("valid dependencies"),
            vec!["PostgreSQL".to_string(), "redis".to_string()]
        );
        assert!(normalize_selected_dependencies(vec!["x".repeat(81)]).is_err());
        assert!(normalize_selected_dependencies(
            (0..65).map(|index| format!("service-{index}")).collect()
        )
        .is_err());
    }
}
