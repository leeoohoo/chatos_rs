// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, State};
use axum::Json;
use serde_json::Value;
use uuid::Uuid;

use crate::sandbox::compose::start_project_compose_environment;
use crate::LocalRuntime;

use super::super::context::owner_context;
use super::super::error::LocalRuntimeApiError;
use super::compose_plan::{build_local_compose_plan, LocalComposeBuildPlan};
use super::response_for;

pub(super) async fn start_environment(
    Path(project_id): Path<String>,
    State(runtime): State<LocalRuntime>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let database = runtime.local_database()?;
    let project = database
        .get_project(project_id.as_str(), owner.owner_user_id.as_str())
        .await?
        .ok_or_else(|| {
            LocalRuntimeApiError::not_found(
                "local_runtime_project_not_found",
                "Local project was not found",
            )
        })?;
    let environment = database
        .ensure_local_runtime_environment(owner.owner_user_id.as_str(), project_id.as_str())
        .await?;
    if !environment.sandbox_enabled {
        return Err(LocalRuntimeApiError::conflict(
            "local_environment_disabled",
            "Local project sandbox is disabled",
        ));
    }
    if environment.status == "pending_configuration" {
        return Err(LocalRuntimeApiError::conflict(
            "local_environment_configuration_required",
            "Complete the required local environment configuration before building",
        ));
    }
    if matches!(
        environment.status.as_str(),
        "analyzing" | "pending_image_build"
    ) {
        return response_for(&runtime, owner.owner_user_id.as_str(), &environment).await;
    }
    let images = database
        .list_local_runtime_environment_images(owner.owner_user_id.as_str(), project_id.as_str())
        .await?;
    let plan =
        build_local_compose_plan(&project, &environment, images.as_slice()).map_err(|error| {
            LocalRuntimeApiError::conflict("local_environment_build_plan_invalid", error)
        })?;
    if !runtime.environment_jobs.register(project_id.as_str()).await {
        return response_for(&runtime, owner.owner_user_id.as_str(), &environment).await;
    }

    let run_id = format!("lc_environment_build_{}", Uuid::new_v4());
    let environment = match database
        .start_local_environment_build(
            owner.owner_user_id.as_str(),
            project_id.as_str(),
            run_id.as_str(),
        )
        .await
    {
        Ok(environment) => environment,
        Err(error) => {
            runtime.environment_jobs.remove(project_id.as_str()).await;
            return Err(error.into());
        }
    };
    let task_runtime = runtime.clone();
    let task_owner = owner.owner_user_id.clone();
    let task_project = project.clone();
    tokio::spawn(async move {
        run_environment_build(task_runtime.clone(), task_owner, task_project, run_id, plan).await;
        task_runtime
            .environment_jobs
            .remove(project_id.as_str())
            .await;
    });
    response_for(&runtime, owner.owner_user_id.as_str(), &environment).await
}

async fn run_environment_build(
    runtime: LocalRuntime,
    owner_user_id: String,
    project: crate::local_runtime::storage::LocalProjectRecord,
    run_id: String,
    plan: LocalComposeBuildPlan,
) {
    let database = match runtime.local_database() {
        Ok(database) => database,
        Err(error) => {
            crate::tracing_stdout(
                format!("open local database for environment build failed: {error}").as_str(),
            );
            return;
        }
    };
    let _ = database
        .update_local_environment_progress(
            owner_user_id.as_str(),
            project.project_id.as_str(),
            Some(run_id.as_str()),
            "building_image",
            "running",
            Some(35),
            "Building and starting the managed local Docker Compose environment",
            None,
            false,
        )
        .await;
    let state = runtime.state.read().await.clone();
    let result =
        start_project_compose_environment(&state, project.workspace_id.as_str(), plan.request)
            .await;
    match result {
        Ok(result) => {
            let logs = compose_result_logs(&result);
            if let Err(error) = database
                .finish_local_environment_build(
                    owner_user_id.as_str(),
                    project.project_id.as_str(),
                    run_id.as_str(),
                    plan.image_refs.as_slice(),
                    logs.as_str(),
                )
                .await
            {
                let message = format!("Persist local environment build result failed: {error}");
                let _ = database
                    .fail_local_environment_build(
                        owner_user_id.as_str(),
                        project.project_id.as_str(),
                        run_id.as_str(),
                        message.as_str(),
                    )
                    .await;
            }
        }
        Err(error) => {
            let message = error.to_string();
            let _ = database
                .fail_local_environment_build(
                    owner_user_id.as_str(),
                    project.project_id.as_str(),
                    run_id.as_str(),
                    message.as_str(),
                )
                .await;
        }
    }
}

fn compose_result_logs(result: &Value) -> String {
    let output = result
        .get("output")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let services = result
        .get("services")
        .cloned()
        .unwrap_or_else(|| Value::Array(Vec::new()));
    let services = serde_json::to_string_pretty(&services).unwrap_or_default();
    if output.trim().is_empty() {
        services
    } else if services.trim().is_empty() || services == "[]" {
        output.to_string()
    } else {
        format!("{output}\n\nServices:\n{services}")
    }
}
