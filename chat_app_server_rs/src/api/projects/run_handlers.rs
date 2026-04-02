use axum::{extract::Path, http::StatusCode, Json};
use serde_json::{json, Value};

use crate::core::auth::AuthUser;
use crate::core::project_access::{ensure_owned_project, map_project_access_error};
use crate::repositories::project_run_catalogs;
use crate::services::project_run::{
    analyze_project, apply_default_target, dispatch_command, resolve_execution, RunExecutionInput,
};

use super::contracts::{ProjectRunDefaultRequest, ProjectRunExecuteRequest};

async fn load_or_analyze_catalog(
    project: &crate::models::project::Project,
) -> Result<crate::models::project_run::ProjectRunCatalog, String> {
    if let Some(cached) =
        project_run_catalogs::get_catalog_by_project_id(project.id.as_str()).await?
    {
        return Ok(cached);
    }
    let analyzed = analyze_project(project).await;
    project_run_catalogs::upsert_catalog(&analyzed).await?;
    Ok(analyzed)
}

pub(super) async fn analyze_project_run(
    auth: AuthUser,
    Path(id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let project = match ensure_owned_project(&id, &auth).await {
        Ok(project) => project,
        Err(err) => return map_project_access_error(err),
    };
    let analyzed = analyze_project(&project).await;
    if let Err(err) = project_run_catalogs::upsert_catalog(&analyzed).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": err})),
        );
    }
    (
        StatusCode::OK,
        Json(serde_json::to_value(analyzed).unwrap_or(Value::Null)),
    )
}

pub(super) async fn get_project_run_catalog(
    auth: AuthUser,
    Path(id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let project = match ensure_owned_project(&id, &auth).await {
        Ok(project) => project,
        Err(err) => return map_project_access_error(err),
    };
    match load_or_analyze_catalog(&project).await {
        Ok(catalog) => (
            StatusCode::OK,
            Json(serde_json::to_value(catalog).unwrap_or(Value::Null)),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": err})),
        ),
    }
}

pub(super) async fn set_project_run_default(
    auth: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<ProjectRunDefaultRequest>,
) -> (StatusCode, Json<Value>) {
    let project = match ensure_owned_project(&id, &auth).await {
        Ok(project) => project,
        Err(err) => return map_project_access_error(err),
    };
    let catalog = match load_or_analyze_catalog(&project).await {
        Ok(catalog) => catalog,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": err})),
            );
        }
    };
    let updated = match apply_default_target(&catalog, req.target_id.as_deref()) {
        Ok(updated) => updated,
        Err(err) => return (StatusCode::BAD_REQUEST, Json(json!({"error": err}))),
    };
    match project_run_catalogs::upsert_catalog(&updated).await {
        Ok(_) => (
            StatusCode::OK,
            Json(serde_json::to_value(updated).unwrap_or(Value::Null)),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": err})),
        ),
    }
}

pub(super) async fn execute_project_run(
    auth: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<ProjectRunExecuteRequest>,
) -> (StatusCode, Json<Value>) {
    let project = match ensure_owned_project(&id, &auth).await {
        Ok(project) => project,
        Err(err) => return map_project_access_error(err),
    };
    let catalog = match load_or_analyze_catalog(&project).await {
        Ok(catalog) => catalog,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": err})),
            );
        }
    };
    let input = RunExecutionInput {
        target_id: req.target_id,
        cwd: req.cwd,
        command: req.command,
        create_if_missing: req.create_if_missing.unwrap_or(true),
    };
    let (cwd, command) = match resolve_execution(&catalog, input.clone()) {
        Ok(v) => v,
        Err(err) => return (StatusCode::BAD_REQUEST, Json(json!({"error": err}))),
    };
    let run = match dispatch_command(
        auth.user_id.as_str(),
        Some(project.id.as_str()),
        cwd.as_str(),
        command.as_str(),
        input.create_if_missing,
    )
    .await
    {
        Ok(result) => result,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": err})),
            );
        }
    };
    (
        StatusCode::OK,
        Json(json!({
            "terminal_id": run.terminal_id,
            "terminal_name": run.terminal_name,
            "terminal_reused": run.terminal_reused,
            "cwd": run.cwd,
            "executed_command": run.executed_command,
            "project_id": project.id,
        })),
    )
}
