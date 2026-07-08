// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::{extract::Path, http::StatusCode, Json};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path as FsPath;

use crate::api::fs::policy::{FsPathPolicy, FsPolicyError};
use crate::api::local_connectors::{
    create_local_terminal_session, parse_local_connector_root_path, send_local_terminal_input,
    LocalConnectorRootRef,
};
use crate::core::auth::AuthUser;
use crate::core::project_access::{ensure_owned_project, map_project_access_error};
use crate::core::user_visible_path::display_path;
use crate::models::project_run::ProjectRunCatalog;
use crate::models::project_run_environment::{
    ProjectRunCustomToolchain, ProjectRunEnvironmentSnapshot, ProjectRunValidationIssue,
};
use crate::models::terminal::{Terminal, TerminalService, TERMINAL_KIND_PROJECT_RUN};
use crate::models::terminal_log::{TerminalLog, TerminalLogService};
use crate::repositories::project_run_catalogs;
use crate::repositories::terminals;
use crate::services::project_local_cache::is_local_connector_project_root;
use crate::services::project_run::{
    analyze_project, apply_default_target, clear_cached_environment_snapshot, dispatch_command,
    env_overrides_for_target, load_environment_selection, load_environment_snapshot,
    read_cached_catalog, refresh_environment_snapshot, resolve_command_with_toolchains,
    resolve_execution, save_environment_selection, validate_project_run_target,
    write_cached_catalog, RunExecutionInput,
};
use crate::services::realtime::publish_project_run_catalog_updated;
use crate::services::realtime::{
    publish_project_run_instance_changed, publish_project_run_state_changed,
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
        let display_cwd = display_path(terminal.cwd.as_str());
        map.insert("cwd".to_string(), Value::String(display_cwd.clone()));
        map.insert("display_cwd".to_string(), Value::String(display_cwd));
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
    policy: Option<&FsPathPolicy>,
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
            let path = policy
                .and_then(|policy| policy.expand_user_visible_path(path.as_str()).ok())
                .filter(|expanded| FsPath::new(expanded.as_str()).exists())
                .unwrap_or(path);
            let label = toolchain.label.unwrap_or_default().trim().to_string();
            Some((
                kind.clone(),
                ProjectRunCustomToolchain { kind, label, path },
            ))
        })
        .collect()
}

fn fs_policy_error_tuple(err: FsPolicyError) -> (StatusCode, Json<Value>) {
    (
        err.status_code(),
        Json(serde_json::json!({ "error": err.message() })),
    )
}

async fn authorize_project_run_cwd(
    auth: &AuthUser,
    raw: &str,
) -> Result<String, (StatusCode, Json<Value>)> {
    let policy = FsPathPolicy::for_user(auth)
        .await
        .map_err(fs_policy_error_tuple)?;
    let authorized = policy
        .authorize_existing_dir(raw, "运行目录不存在或不是目录", "运行目录不存在或不是目录")
        .map_err(fs_policy_error_tuple)?;
    policy
        .require_write(&authorized)
        .map_err(fs_policy_error_tuple)?;
    Ok(authorized.path.to_string_lossy().to_string())
}

fn local_connector_refs_match(
    project_root: &LocalConnectorRootRef,
    cwd: &LocalConnectorRootRef,
) -> bool {
    if project_root.device_id != cwd.device_id || project_root.workspace_id != cwd.workspace_id {
        return false;
    }
    let project_relative = project_root.relative_path.as_deref().unwrap_or("");
    let cwd_relative = cwd.relative_path.as_deref().unwrap_or("");
    project_relative.is_empty()
        || cwd_relative == project_relative
        || cwd_relative.starts_with(format!("{project_relative}/").as_str())
}

fn shell_quote_local_value(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn build_local_connector_project_run_input(
    command: &str,
    env_overrides: &HashMap<String, String>,
) -> String {
    let mut entries = env_overrides.iter().collect::<Vec<_>>();
    entries.sort_by(|(left, _), (right, _)| left.cmp(right));
    let mut input = String::new();
    for (key, value) in entries {
        let key = key.trim();
        if key.is_empty() {
            continue;
        }
        input.push_str(
            format!("export {key}={}\n", shell_quote_local_value(value.as_str())).as_str(),
        );
    }
    input.push_str(command.trim());
    input.push('\n');
    input
}

fn connector_error_response_message(err: (StatusCode, Json<Value>)) -> String {
    let (status, Json(value)) = err;
    value
        .get("error")
        .and_then(Value::as_str)
        .map(|message| format!("{message} ({status})"))
        .unwrap_or_else(|| format!("{value} ({status})"))
}

async fn dispatch_local_connector_project_run(
    user_id: &str,
    project_id: &str,
    project_name: &str,
    project_root: &str,
    cwd: &str,
    command: &str,
    create_if_missing: bool,
    env_overrides: HashMap<String, String>,
    preferred_terminal_id: Option<&str>,
) -> Result<crate::services::project_run::RunDispatchResult, String> {
    let project_ref = parse_local_connector_root_path(project_root)
        .ok_or_else(|| "Local Connector 项目根目录格式错误".to_string())?;
    let cwd_ref = parse_local_connector_root_path(cwd)
        .ok_or_else(|| "Local Connector 运行目录格式错误".to_string())?;
    if !local_connector_refs_match(&project_ref, &cwd_ref) {
        return Err("Local Connector 运行目录必须位于项目目录内".to_string());
    }
    if command.trim().is_empty() {
        return Err("运行命令不能为空".to_string());
    }

    let reusable = if let Some(terminal_id) = preferred_terminal_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let terminal = TerminalService::get_by_id(terminal_id).await?;
        terminal.filter(|item| {
            item.kind == TERMINAL_KIND_PROJECT_RUN
                && item.user_id.as_deref() == Some(user_id)
                && item.project_id.as_deref() == Some(project_id)
                && item.status == "running"
        })
    } else {
        None
    };

    let (terminal, reused) = if let Some(terminal) = reusable {
        (terminal, true)
    } else if create_if_missing {
        let terminal_name = if project_name.trim().is_empty() {
            "Local Connector 运行实例".to_string()
        } else {
            format!("{} 运行实例", project_name.trim())
        };
        let terminal = Terminal::new(
            terminal_name,
            cwd.trim().to_string(),
            TERMINAL_KIND_PROJECT_RUN.to_string(),
            Some(user_id.to_string()),
            Some(project_id.to_string()),
        );
        terminals::create_terminal(&terminal).await?;
        publish_project_run_instance_changed(
            user_id, project_id, &terminal, false, true, "running", "created", None,
        );
        publish_project_run_state_changed(
            user_id,
            project_id,
            Some(&terminal),
            false,
            true,
            "running",
            "created",
            None,
        );
        (terminal, false)
    } else {
        return Err("未找到可复用终端，且未允许自动创建".to_string());
    };

    create_local_terminal_session(
        cwd_ref.device_id.as_str(),
        cwd_ref.workspace_id.as_str(),
        terminal.id.as_str(),
        cwd_ref.relative_path.as_deref(),
        120,
        32,
    )
    .await
    .map_err(connector_error_response_message)?;

    let input = build_local_connector_project_run_input(command, &env_overrides);
    send_local_terminal_input(
        cwd_ref.device_id.as_str(),
        cwd_ref.workspace_id.as_str(),
        terminal.id.as_str(),
        input.as_str(),
    )
    .await
    .map_err(connector_error_response_message)?;

    let _ = TerminalLogService::create(TerminalLog::new(
        terminal.id.clone(),
        "command".to_string(),
        command.trim().to_string(),
    ))
    .await;
    let _ = TerminalLogService::create(TerminalLog::new(
        terminal.id.clone(),
        "input".to_string(),
        input,
    ))
    .await;
    let _ = TerminalService::touch(terminal.id.as_str()).await;
    publish_project_run_instance_changed(
        user_id,
        project_id,
        &terminal,
        true,
        true,
        "running",
        "command_dispatched",
        None,
    );
    publish_project_run_state_changed(
        user_id,
        project_id,
        Some(&terminal),
        true,
        true,
        "running",
        "command_dispatched",
        None,
    );

    Ok(crate::services::project_run::RunDispatchResult {
        terminal_id: terminal.id,
        terminal_name: terminal.name,
        terminal_reused: reused,
        terminal_status: terminal.status,
        cwd: cwd.trim().to_string(),
        executed_command: command.trim().to_string(),
    })
}

fn visible_project_run_catalog(mut catalog: ProjectRunCatalog) -> ProjectRunCatalog {
    if catalog.status != "error" {
        catalog.error_message = None;
    }
    for target in &mut catalog.targets {
        target.cwd = display_path(target.cwd.as_str());
        target.manifest_path = target
            .manifest_path
            .as_ref()
            .map(|path| display_path(path.as_str()));
    }
    catalog
}

fn visible_validation_issues(
    issues: Vec<ProjectRunValidationIssue>,
) -> Vec<ProjectRunValidationIssue> {
    issues
        .into_iter()
        .map(|mut issue| {
            issue.path = issue.path.as_ref().map(|path| display_path(path.as_str()));
            issue
        })
        .collect()
}

fn visible_project_run_environment(
    mut snapshot: ProjectRunEnvironmentSnapshot,
) -> ProjectRunEnvironmentSnapshot {
    for rows in snapshot.options_by_kind.values_mut() {
        for option in rows {
            option.path = display_path(option.path.as_str());
        }
    }
    for config in &mut snapshot.config_files {
        config.path = display_path(config.path.as_str());
    }
    snapshot.validation_issues = visible_validation_issues(snapshot.validation_issues);
    for custom in snapshot.custom_toolchains.values_mut() {
        custom.path = display_path(custom.path.as_str());
    }
    snapshot
}

fn visible_env_value(value: &str) -> String {
    if value.contains(':') {
        return value
            .split(':')
            .map(display_path)
            .collect::<Vec<_>>()
            .join(":");
    }
    display_path(value)
}

fn visible_env_overrides(env_overrides: HashMap<String, String>) -> HashMap<String, String> {
    env_overrides
        .into_iter()
        .map(|(key, value)| (key, visible_env_value(value.as_str())))
        .collect()
}

async fn load_or_analyze_catalog(
    project: &crate::models::project::Project,
) -> Result<ProjectRunCatalog, String> {
    let is_local_connector = is_local_connector_project_root(project.root_path.as_str());
    let should_reanalyze_local_connector = |catalog: &ProjectRunCatalog| {
        is_local_connector && (catalog.status == "error" || catalog.error_message.is_some())
    };
    if let Some(cached) =
        project_run_catalogs::get_catalog_by_project_id(project.id.as_str()).await?
    {
        if cached.analyzed_at.is_some() && !should_reanalyze_local_connector(&cached) {
            let _ = write_cached_catalog(project.root_path.as_str(), &cached);
            return Ok(cached);
        }
    }
    if let Some(cached) = read_cached_catalog(project.root_path.as_str())? {
        if cached.analyzed_at.is_some() && !should_reanalyze_local_connector(&cached) {
            let _ = project_run_catalogs::upsert_catalog(&cached).await;
            return Ok(cached);
        }
    }

    let analyzed = analyze_project(project).await;
    let _ = project_run_catalogs::upsert_catalog(&analyzed).await;
    let _ = write_cached_catalog(project.root_path.as_str(), &analyzed);
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
    let _ = write_cached_catalog(project.root_path.as_str(), &analyzed);
    let _ = clear_cached_environment_snapshot(project.root_path.as_str());
    (
        StatusCode::OK,
        Json(serde_json::to_value(visible_project_run_catalog(analyzed)).unwrap_or(Value::Null)),
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
            Json(serde_json::to_value(visible_project_run_catalog(catalog)).unwrap_or(Value::Null)),
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
            Json(serde_json::to_value(visible_project_run_catalog(updated)).unwrap_or(Value::Null)),
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

    if is_local_connector_project_root(project.root_path.as_str()) {
        let run = match dispatch_local_connector_project_run(
            auth.user_id.as_str(),
            project.id.as_str(),
            project.name.as_str(),
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
        return (
            StatusCode::OK,
            Json(json!({
                "terminal_id": run.terminal_id,
                "terminal_name": run.terminal_name,
                "terminal_reused": run.terminal_reused,
                "status": run.terminal_status,
                "cwd": display_path(run.cwd.as_str()),
                "display_cwd": display_path(run.cwd.as_str()),
                "executed_command": run.executed_command,
                "project_id": project.id,
                "env_overrides": visible_env_overrides(env_overrides),
            })),
        );
    }

    let cwd = match authorize_project_run_cwd(&auth, cwd.as_str()).await {
        Ok(path) => path,
        Err(err) => return err,
    };
    if let Some(target) = target.as_ref() {
        let issues = validate_project_run_target(
            std::path::Path::new(project.root_path.as_str()),
            target,
            saved_selection.as_ref(),
            &environment_snapshot.options_by_kind,
        );
        if !issues.is_empty() {
            let visible_issues = visible_validation_issues(issues);
            let visible_issue = visible_issues.first().cloned().unwrap_or_default();
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": visible_issue.message,
                    "validation_issue": visible_issue,
                    "validation_issues": visible_issues,
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
            "cwd": display_path(run.cwd.as_str()),
            "display_cwd": display_path(run.cwd.as_str()),
            "executed_command": run.executed_command,
            "project_id": project.id,
            "env_overrides": visible_env_overrides(env_overrides),
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
                "cwd": display_path(value.cwd.as_str()),
                "display_cwd": display_path(value.cwd.as_str()),
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
            "cwd": terminal.as_ref().map(|value| display_path(value.cwd.as_str())),
            "display_cwd": terminal.as_ref().map(|value| display_path(value.cwd.as_str())),
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
            Json(
                serde_json::to_value(visible_project_run_environment(snapshot))
                    .unwrap_or(Value::Null),
            ),
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
    let path_policy = FsPathPolicy::for_user(&auth).await.ok();

    let terminal_ui_enabled = match load_environment_selection(project.id.as_str()).await {
        Ok(selection) => req.terminal_ui_enabled.unwrap_or_else(|| {
            selection
                .map(|value| value.terminal_ui_enabled)
                .unwrap_or(true)
        }),
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": err})),
            );
        }
    };

    match save_environment_selection(
        &project,
        req.selected_toolchains.unwrap_or_default(),
        normalize_custom_toolchains(req.custom_toolchains, path_policy.as_ref()),
        req.env_vars.unwrap_or_default(),
        terminal_ui_enabled,
    )
    .await
    {
        Ok(_) => match refresh_environment_snapshot(&project).await {
            Ok(snapshot) => {
                publish_project_run_catalog_updated(
                    auth.user_id.as_str(),
                    project.id.as_str(),
                    "project_run_environment_changed",
                    None,
                );
                (
                    StatusCode::OK,
                    Json(
                        serde_json::to_value(visible_project_run_environment(snapshot))
                            .unwrap_or(Value::Null),
                    ),
                )
            }
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
