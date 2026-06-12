use axum::{Json, extract::Path, http::StatusCode};
use serde_json::{Value, json};
use std::collections::HashMap;

use crate::core::auth::AuthUser;
use crate::core::project_access::{ensure_owned_project, map_project_access_error};
use crate::core::time::now_rfc3339;
use crate::models::project_run::ProjectRunCatalog;
use crate::models::project_run_environment::ProjectRunCustomToolchain;
use crate::models::terminal::TerminalService;
use crate::repositories::project_run_catalogs;
use crate::services::project_run::{
    RunExecutionInput, analyze_project, apply_default_target, clear_cached_environment_snapshot,
    dispatch_command, env_overrides_for_target, load_environment_selection,
    load_environment_snapshot, read_cached_catalog, refresh_environment_snapshot,
    resolve_command_with_toolchains, resolve_execution, save_environment_selection,
    validate_project_run_target, write_cached_catalog,
};
use crate::services::terminal_manager::get_terminal_manager;

use super::contracts::{
    ProjectRunDefaultRequest, ProjectRunEnvironmentUpdateRequest, ProjectRunExecuteRequest,
};

fn serialize_project_run_terminal(
    terminal: &crate::models::terminal::Terminal,
    busy: bool,
) -> Value {
    let mut serialized = serde_json::to_value(terminal).unwrap_or(Value::Null);
    if let Value::Object(ref mut map) = serialized {
        map.insert("busy".to_string(), Value::Bool(busy));
        map.insert(
            "running".to_string(),
            Value::Bool(terminal.status == "running"),
        );
    }
    serialized
}

fn normalize_custom_toolchains(
    raw: Option<HashMap<String, super::contracts::ProjectRunCustomToolchainRequest>>,
) -> HashMap<String, ProjectRunCustomToolchain> {
    raw.unwrap_or_default()
        .into_iter()
        .filter_map(|(map_kind, toolchain)| {
            let kind = toolchain
                .kind
                .as_deref()
                .unwrap_or(map_kind.as_str())
                .trim()
                .to_string();
            let path = toolchain.path.unwrap_or_default().trim().to_string();
            if kind.is_empty() || path.is_empty() {
                return None;
            }
            let label = toolchain.label.unwrap_or_default().trim().to_string();
            Some((
                kind.clone(),
                ProjectRunCustomToolchain { kind, label, path },
            ))
        })
        .collect()
}

async fn load_or_analyze_catalog(
    project: &crate::models::project::Project,
) -> Result<crate::models::project_run::ProjectRunCatalog, String> {
    if let Some(cached) =
        project_run_catalogs::get_catalog_by_project_id(project.id.as_str()).await?
    {
        let _ = write_cached_catalog(project.root_path.as_str(), &cached);
        return Ok(cached);
    }
    if let Some(cached) = read_cached_catalog(project.root_path.as_str())? {
        let _ = project_run_catalogs::upsert_catalog(&cached).await;
        return Ok(cached);
    }
    Ok(ProjectRunCatalog {
        project_id: project.id.clone(),
        user_id: project.user_id.clone(),
        status: "empty".to_string(),
        default_target_id: None,
        targets: vec![],
        error_message: None,
        analyzed_at: None,
        updated_at: now_rfc3339(),
    })
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
    let _ = write_cached_catalog(project.root_path.as_str(), &analyzed);
    let _ = clear_cached_environment_snapshot(project.root_path.as_str());
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
    let target = input
        .target_id
        .as_deref()
        .and_then(|target_id| catalog.targets.iter().find(|item| item.id == target_id))
        .cloned();
    let environment_snapshot = match load_environment_snapshot(&project).await {
        Ok(snapshot) => snapshot,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": err})),
            );
        }
    };
    let saved_selection = match load_environment_selection(project.id.as_str()).await {
        Ok(selection) => selection,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": err})),
            );
        }
    };
    let env_overrides = target
        .as_ref()
        .map(|value| {
            env_overrides_for_target(
                value,
                saved_selection.as_ref(),
                &environment_snapshot.options_by_kind,
            )
        })
        .unwrap_or_default();
    let resolved_command = target
        .as_ref()
        .map(|value| {
            resolve_command_with_toolchains(
                value,
                saved_selection.as_ref(),
                &environment_snapshot.options_by_kind,
            )
        })
        .unwrap_or(command.clone());
    if let Some(target) = target.as_ref() {
        let issues = validate_project_run_target(
            std::path::Path::new(project.root_path.as_str()),
            target,
            saved_selection.as_ref(),
            &environment_snapshot.options_by_kind,
        );
        if let Some(issue) = issues.first() {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": issue.message,
                    "validation_issue": issue,
                    "validation_issues": issues,
                })),
            );
        }
    }
    let run = match dispatch_command(
        auth.user_id.as_str(),
        Some(project.id.as_str()),
        project.root_path.as_str(),
        cwd.as_str(),
        resolved_command.as_str(),
        input.create_if_missing,
        env_overrides.clone(),
        req.terminal_id.as_deref(),
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
            "status": run.terminal_status,
            "cwd": run.cwd,
            "executed_command": run.executed_command,
            "project_id": project.id,
            "env_overrides": env_overrides,
        })),
    )
}

pub(super) async fn get_project_run_state(
    auth: AuthUser,
    Path(id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let project = match ensure_owned_project(&id, &auth).await {
        Ok(project) => project,
        Err(err) => return map_project_access_error(err),
    };
    let terminal = match TerminalService::get_project_run_by_project_id(
        Some(auth.user_id.clone()),
        project.id.as_str(),
    )
    .await
    {
        Ok(value) => value,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": err})),
            );
        }
    };
    let manager = get_terminal_manager();
    let terminals = match TerminalService::list_project_runs_by_project_id(
        Some(auth.user_id.clone()),
        project.id.as_str(),
    )
    .await
    {
        Ok(value) => value,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": err})),
            );
        }
    };
    let terminal_entries = terminals
        .iter()
        .map(|value| {
            let busy = manager.get_busy(value.id.as_str()).unwrap_or(false);
            json!({
                "terminal_id": value.id,
                "terminal_name": value.name,
                "cwd": value.cwd,
                "status": value.status,
                "busy": busy,
                "running": value.status == "running",
                "terminal": serialize_project_run_terminal(value, busy),
            })
        })
        .collect::<Vec<_>>();
    let busy = terminals
        .iter()
        .any(|value| manager.get_busy(value.id.as_str()).unwrap_or(false));
    let running = terminals.iter().any(|value| value.status == "running");
    let aggregate_status = if running {
        "running".to_string()
    } else {
        terminals
            .first()
            .map(|value| value.status.clone())
            .unwrap_or_else(|| "idle".to_string())
    };
    let terminal_value = terminal.as_ref().map(|value| {
        serialize_project_run_terminal(value, manager.get_busy(value.id.as_str()).unwrap_or(false))
    });
    (
        StatusCode::OK,
        Json(json!({
            "project_id": project.id,
            "running": running,
            "busy": busy,
            "status": aggregate_status,
            "terminal_id": terminal.as_ref().map(|value| value.id.clone()),
            "terminal_name": terminal.as_ref().map(|value| value.name.clone()),
            "cwd": terminal.as_ref().map(|value| value.cwd.clone()),
            "terminal": terminal_value,
            "instances": terminal_entries,
        })),
    )
}

pub(super) async fn get_project_run_environment(
    auth: AuthUser,
    Path(id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let project = match ensure_owned_project(&id, &auth).await {
        Ok(project) => project,
        Err(err) => return map_project_access_error(err),
    };
    match load_environment_snapshot(&project).await {
        Ok(snapshot) => (
            StatusCode::OK,
            Json(serde_json::to_value(snapshot).unwrap_or(Value::Null)),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": err})),
        ),
    }
}

pub(super) async fn update_project_run_environment(
    auth: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<ProjectRunEnvironmentUpdateRequest>,
) -> (StatusCode, Json<Value>) {
    let project = match ensure_owned_project(&id, &auth).await {
        Ok(project) => project,
        Err(err) => return map_project_access_error(err),
    };

    match save_environment_selection(
        &project,
        req.selected_toolchains.unwrap_or_default(),
        normalize_custom_toolchains(req.custom_toolchains),
        req.env_vars.unwrap_or_default(),
    )
    .await
    {
        Ok(_) => match refresh_environment_snapshot(&project).await {
            Ok(snapshot) => (
                StatusCode::OK,
                Json(serde_json::to_value(snapshot).unwrap_or(Value::Null)),
            ),
            Err(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": err})),
            ),
        },
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": err})),
        ),
    }
}
