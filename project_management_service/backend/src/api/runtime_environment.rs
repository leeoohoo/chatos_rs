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
    analyze_project_runtime_environment, get_project_runtime_environment_progress,
};
use crate::services::runtime_environment::{
    default_runtime_environment_for_project, ensure_runtime_environment_for_project,
};
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

    {
        let mut active = state.runtime_environment_analysis_jobs.lock().await;
        if !active.insert(project_id.clone()) {
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
            return Ok(Json(ProjectRuntimeEnvironmentResponse {
                environment,
                images,
            }));
        }
    }

    let run_id = format!("project_env_agent_{}", Uuid::new_v4());
    let queued = async {
        let mut environment =
            ensure_runtime_environment_for_project(&state.store, &project, None).await?;
        reset_environment_for_analysis(&mut environment, run_id.as_str());
        let environment = state
            .store
            .upsert_project_runtime_environment(&environment)
            .await?;
        state
            .store
            .replace_project_runtime_environment_images(&project_id, &[])
            .await?;
        let images = state
            .store
            .list_project_runtime_environment_images(&project_id)
            .await?;
        Ok::<ProjectRuntimeEnvironmentResponse, String>(ProjectRuntimeEnvironmentResponse {
            environment,
            images,
        })
    }
    .await;
    let response = match queued {
        Ok(response) => response,
        Err(err) => {
            state
                .runtime_environment_analysis_jobs
                .lock()
                .await
                .remove(&project_id);
            return Err(ApiError::bad_request(err));
        }
    };

    let worker_state = state.clone();
    let worker_project = project.clone();
    let worker_project_id = project_id.clone();
    let worker_run_id = run_id.clone();
    let worker_access_token = access_token.0.clone();
    tokio::spawn(async move {
        let task_state = worker_state.clone();
        let task_project = worker_project.clone();
        let task_run_id = worker_run_id.clone();
        let task = tokio::spawn(async move {
            analyze_project_runtime_environment(
                &task_state,
                &task_project,
                Some(worker_access_token.as_str()),
                task_run_id.as_str(),
            )
            .await
        });
        let failure = match task.await {
            Ok(Ok(_)) => None,
            Ok(Err(err)) => Some(err),
            Err(err) => Some(format!("project environment analysis task failed: {err}")),
        };
        if let Some(err) = failure {
            persist_background_analysis_failure(
                &worker_state,
                worker_project_id.as_str(),
                worker_run_id.as_str(),
                err.as_str(),
            )
            .await;
        }
        worker_state
            .runtime_environment_analysis_jobs
            .lock()
            .await
            .remove(worker_project_id.as_str());
    });

    Ok(Json(response))
}

fn reset_environment_for_analysis(environment: &mut ProjectRuntimeEnvironmentRecord, run_id: &str) {
    environment.status = ProjectRuntimeEnvironmentStatus::Analyzing;
    environment.sandbox_provider = RuntimeEnvironmentProvider::None;
    environment.file_provider = RuntimeEnvironmentProvider::None;
    environment.analysis_summary = Some("正在重新分析项目并准备沙箱运行环境。".to_string());
    environment.not_runnable_reason = None;
    environment.detected_stack = empty_object();
    environment.required_services = empty_array();
    environment.env_vars = empty_object();
    environment.last_agent_run_id = Some(run_id.to_string());
    environment.last_error = None;
    environment.updated_at = now_rfc3339();
}

async fn persist_background_analysis_failure(
    state: &AppState,
    project_id: &str,
    run_id: &str,
    error: &str,
) {
    let Ok(Some(mut environment)) = state
        .store
        .get_project_runtime_environment(project_id)
        .await
    else {
        tracing::error!(
            project_id,
            run_id,
            error,
            "load failed project environment analysis"
        );
        return;
    };
    if environment.last_agent_run_id.as_deref() != Some(run_id)
        || environment.status != ProjectRuntimeEnvironmentStatus::Analyzing
    {
        return;
    }
    environment.status = ProjectRuntimeEnvironmentStatus::Failed;
    environment.analysis_summary = Some("项目运行环境后台分析失败。".to_string());
    environment.last_error = Some(error.to_string());
    environment.updated_at = now_rfc3339();
    if let Err(persist_error) = state
        .store
        .upsert_project_runtime_environment(&environment)
        .await
    {
        tracing::error!(
            project_id,
            run_id,
            error = persist_error.as_str(),
            "persist failed project environment analysis"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::reset_environment_for_analysis;
    use crate::models::{
        ProjectRuntimeEnvironmentRecord, ProjectRuntimeEnvironmentStatus,
        RuntimeEnvironmentProvider,
    };
    use serde_json::json;

    #[test]
    fn reanalysis_clears_stale_provisioning_failure_state() {
        let mut environment = ProjectRuntimeEnvironmentRecord {
            project_id: "project-1".to_string(),
            status: ProjectRuntimeEnvironmentStatus::Failed,
            sandbox_enabled: true,
            sandbox_provider: RuntimeEnvironmentProvider::LocalConnector,
            file_provider: RuntimeEnvironmentProvider::Harness,
            analysis_summary: Some("old summary".to_string()),
            not_runnable_reason: Some("old reason".to_string()),
            detected_stack: json!({"stale": true}),
            required_services: json!([{"stale": true}]),
            env_vars: json!({"STALE": "1"}),
            last_agent_run_id: Some("run-old".to_string()),
            last_error: Some("Docker is not installed".to_string()),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };

        reset_environment_for_analysis(&mut environment, "run-new");

        assert_eq!(
            environment.status,
            ProjectRuntimeEnvironmentStatus::Analyzing
        );
        assert_eq!(
            environment.sandbox_provider,
            RuntimeEnvironmentProvider::None
        );
        assert_eq!(environment.file_provider, RuntimeEnvironmentProvider::None);
        assert_eq!(environment.last_agent_run_id.as_deref(), Some("run-new"));
        assert!(environment.last_error.is_none());
        assert!(environment.not_runnable_reason.is_none());
        assert_eq!(environment.detected_stack, json!({}));
        assert_eq!(environment.required_services, json!([]));
        assert_eq!(environment.env_vars, json!({}));
    }
}
