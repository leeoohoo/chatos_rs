// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod compose_plan;
mod lifecycle;
mod response;

use axum::extract::{Path, State};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

use crate::local_runtime::run_local_environment_analysis;
use crate::LocalRuntime;

use super::context::owner_context;
use super::error::LocalRuntimeApiError;
use response::{environment_response, idle_progress_response, progress_response};

const MAX_ANALYSIS_REQUIREMENT_LENGTH: usize = 4_000;
const MAX_SELECTED_DEPENDENCIES: usize = 64;
const MAX_SELECTED_DEPENDENCY_LENGTH: usize = 80;

#[derive(Debug, Default, Deserialize)]
struct EnvironmentSettingsPayload {
    sandbox_enabled: Option<bool>,
    #[serde(rename = "sandboxEnabled")]
    sandbox_enabled_camel: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
struct EnvironmentAnalysisPayload {
    analysis_requirement: Option<String>,
    #[serde(rename = "analysisRequirement")]
    analysis_requirement_camel: Option<String>,
    selected_dependencies: Option<Vec<String>>,
    #[serde(rename = "selectedDependencies")]
    selected_dependencies_camel: Option<Vec<String>>,
}

pub(super) fn router() -> Router<LocalRuntime> {
    Router::new()
        .route(
            "/api/local/runtime/projects/{project_id}/runtime-environment",
            get(get_environment),
        )
        .route(
            "/api/local/runtime/projects/{project_id}/runtime-environment/settings",
            put(update_settings),
        )
        .route(
            "/api/local/runtime/projects/{project_id}/runtime-environment/analyze",
            post(analyze_environment),
        )
        .route(
            "/api/local/runtime/projects/{project_id}/runtime-environment/start",
            post(lifecycle::start_environment),
        )
        .route(
            "/api/local/runtime/projects/{project_id}/runtime-environment/progress",
            get(get_progress),
        )
}

async fn get_environment(
    Path(project_id): Path<String>,
    State(runtime): State<LocalRuntime>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let environment = runtime
        .local_database()?
        .ensure_local_runtime_environment(owner.owner_user_id.as_str(), project_id.as_str())
        .await?;
    response_for(&runtime, owner.owner_user_id.as_str(), &environment).await
}

async fn update_settings(
    Path(project_id): Path<String>,
    State(runtime): State<LocalRuntime>,
    Json(payload): Json<EnvironmentSettingsPayload>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let enabled = payload
        .sandbox_enabled
        .or(payload.sandbox_enabled_camel)
        .ok_or_else(|| {
            LocalRuntimeApiError::bad_request(
                "local_environment_setting_required",
                "sandbox_enabled is required",
            )
        })?;
    let environment = runtime
        .local_database()?
        .set_local_environment_enabled(owner.owner_user_id.as_str(), project_id.as_str(), enabled)
        .await?;
    response_for(&runtime, owner.owner_user_id.as_str(), &environment).await
}

async fn analyze_environment(
    Path(project_id): Path<String>,
    State(runtime): State<LocalRuntime>,
    payload: Option<Json<EnvironmentAnalysisPayload>>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let payload = payload.map(|Json(payload)| payload).unwrap_or_default();
    let analysis_requirement = normalize_analysis_requirement(
        payload
            .analysis_requirement
            .or(payload.analysis_requirement_camel),
    )?;
    let selected_dependencies = normalize_selected_dependencies(
        payload
            .selected_dependencies
            .or(payload.selected_dependencies_camel)
            .unwrap_or_default(),
    )?;
    let model_config_id = {
        let state = runtime.state.read().await;
        state
            .model_configs
            .settings
            .environment_initialization_model_config_id
            .clone()
            .ok_or_else(|| {
                LocalRuntimeApiError::conflict(
                    "local_environment_model_required",
                    "Configure the Environment Initialization model in Local Connector first",
                )
            })?
    };
    if !runtime.environment_jobs.register(project_id.as_str()).await {
        let environment = runtime
            .local_database()?
            .ensure_local_runtime_environment(owner.owner_user_id.as_str(), project_id.as_str())
            .await?;
        return response_for(&runtime, owner.owner_user_id.as_str(), &environment).await;
    }
    let run_id = format!("lc_environment_run_{}", Uuid::new_v4());
    let environment = runtime
        .local_database()?
        .start_local_environment_analysis(
            owner.owner_user_id.as_str(),
            project_id.as_str(),
            run_id.as_str(),
        )
        .await?;
    let task_runtime = runtime.clone();
    let task_owner = owner.owner_user_id.clone();
    let task_project = project_id.clone();
    let task_analysis_requirement = analysis_requirement;
    let task_selected_dependencies = selected_dependencies;
    tokio::spawn(async move {
        if let Err(error) = run_local_environment_analysis(
            task_runtime.clone(),
            task_owner.clone(),
            task_project.clone(),
            model_config_id,
            run_id.clone(),
            task_analysis_requirement,
            task_selected_dependencies,
        )
        .await
        {
            if let Ok(database) = task_runtime.local_database() {
                let _ = database
                    .fail_local_environment_analysis(
                        task_owner.as_str(),
                        task_project.as_str(),
                        run_id.as_str(),
                        error.as_str(),
                    )
                    .await;
            }
        }
        task_runtime
            .environment_jobs
            .remove(task_project.as_str())
            .await;
    });
    response_for(&runtime, owner.owner_user_id.as_str(), &environment).await
}

fn normalize_analysis_requirement(
    value: Option<String>,
) -> Result<Option<String>, LocalRuntimeApiError> {
    let requirement = value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if requirement
        .as_deref()
        .is_some_and(|value| value.chars().count() > MAX_ANALYSIS_REQUIREMENT_LENGTH)
    {
        return Err(LocalRuntimeApiError::bad_request(
            "local_environment_analysis_requirement_too_long",
            format!(
                "analysis_requirement must not exceed {MAX_ANALYSIS_REQUIREMENT_LENGTH} characters"
            ),
        ));
    }
    Ok(requirement)
}

fn normalize_selected_dependencies(
    values: Vec<String>,
) -> Result<Vec<String>, LocalRuntimeApiError> {
    let mut normalized = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for value in values {
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        if value.chars().count() > MAX_SELECTED_DEPENDENCY_LENGTH {
            return Err(LocalRuntimeApiError::bad_request(
                "local_environment_selected_dependency_too_long",
                format!(
                    "selected_dependencies entries must not exceed {MAX_SELECTED_DEPENDENCY_LENGTH} characters"
                ),
            ));
        }
        if seen.insert(value.to_lowercase()) {
            normalized.push(value.to_string());
        }
    }
    if normalized.len() > MAX_SELECTED_DEPENDENCIES {
        return Err(LocalRuntimeApiError::bad_request(
            "local_environment_selected_dependencies_too_many",
            format!(
                "selected_dependencies must not contain more than {MAX_SELECTED_DEPENDENCIES} entries"
            ),
        ));
    }
    Ok(normalized)
}

async fn get_progress(
    Path(project_id): Path<String>,
    State(runtime): State<LocalRuntime>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let progress = runtime
        .local_database()?
        .get_local_environment_progress(owner.owner_user_id.as_str(), project_id.as_str())
        .await?;
    Ok(Json(progress.map_or_else(
        || idle_progress_response(project_id.as_str()),
        |record| progress_response(&record),
    )))
}

pub(super) async fn response_for(
    runtime: &LocalRuntime,
    owner_user_id: &str,
    environment: &crate::local_runtime::LocalRuntimeEnvironmentRecord,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let images = runtime
        .local_database()?
        .list_local_runtime_environment_images(owner_user_id, environment.project_id.as_str())
        .await?;
    Ok(Json(environment_response(environment, images.as_slice())))
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
