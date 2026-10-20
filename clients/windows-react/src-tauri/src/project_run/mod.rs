mod analyzer;
mod models;
mod runtime;
#[cfg(test)]
mod tests;

use analyzer::{analyze_project, resolve_project_run_target};
use models::{
    ProjectRunAnalysisSnapshot, ProjectRunCatalog, ProjectRunChangeSnapshot, ProjectRunEnvironment,
    ProjectRunState, ProjectRunValidationIssue,
};
pub(crate) use runtime::ProjectRunManager;
use runtime::ProjectRunScope;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, State};

use crate::{host_lifecycle::validate_owner, platform_execution_context::resolve_project_root};

#[tauri::command]
pub(crate) async fn analyze_project_run(
    app: AppHandle,
    owner_user_id: String,
    project_id: String,
) -> Result<ProjectRunAnalysisSnapshot, String> {
    validate_owner(&owner_user_id)?;
    validate_identifier(&project_id)?;
    let app_data_directory = app
        .path()
        .app_data_dir()
        .map_err(|_| "无法读取项目运行环境。".to_owned())?;
    tauri::async_runtime::spawn_blocking(move || {
        analyze_resolved_project(&app_data_directory, &owner_user_id, &project_id)
    })
    .await
    .map_err(|_| "项目运行分析任务异常退出。".to_owned())?
}

#[tauri::command]
pub(crate) async fn load_project_run_state(
    app: AppHandle,
    manager: State<'_, ProjectRunManager>,
    owner_user_id: String,
    project_id: String,
) -> Result<ProjectRunState, String> {
    let scope = resolve_scope(&app, owner_user_id, project_id).await?;
    manager.snapshot(&scope).await
}

#[tauri::command]
pub(crate) async fn poll_project_run_changes(
    app: AppHandle,
    manager: State<'_, ProjectRunManager>,
    owner_user_id: String,
    project_id: String,
    after_revision: u64,
    timeout_ms: u64,
) -> Result<ProjectRunChangeSnapshot, String> {
    let scope = resolve_scope(&app, owner_user_id, project_id).await?;
    manager
        .poll_changes(&scope, after_revision, timeout_ms)
        .await
}

#[tauri::command]
pub(crate) async fn start_project_run(
    app: AppHandle,
    manager: State<'_, ProjectRunManager>,
    owner_user_id: String,
    project_id: String,
    target_id: String,
) -> Result<ProjectRunState, String> {
    validate_identifier(&target_id)?;
    let scope = resolve_scope(&app, owner_user_id, project_id).await?;
    let root = scope.project_root.clone();
    let target =
        tauri::async_runtime::spawn_blocking(move || resolve_project_run_target(&root, &target_id))
            .await
            .map_err(|_| "项目运行分析任务异常退出。".to_owned())??;
    manager.start(&scope, target).await
}

#[tauri::command]
pub(crate) async fn stop_project_run(
    app: AppHandle,
    manager: State<'_, ProjectRunManager>,
    owner_user_id: String,
    project_id: String,
    instance_id: String,
) -> Result<ProjectRunState, String> {
    let scope = resolve_scope(&app, owner_user_id, project_id).await?;
    manager.stop(&scope, &instance_id).await
}

#[tauri::command]
pub(crate) async fn delete_project_run_instance(
    app: AppHandle,
    manager: State<'_, ProjectRunManager>,
    owner_user_id: String,
    project_id: String,
    instance_id: String,
) -> Result<ProjectRunState, String> {
    let scope = resolve_scope(&app, owner_user_id, project_id).await?;
    manager.delete(&scope, &instance_id).await
}

fn analyze_resolved_project(
    app_data_directory: &std::path::Path,
    owner_user_id: &str,
    project_id: &str,
) -> Result<ProjectRunAnalysisSnapshot, String> {
    let resolved = resolve_project_root(
        &app_data_directory.join("NativeConnector"),
        owner_user_id,
        project_id,
    )?;
    let analysis = analyze_project(&resolved.root)?;
    let default_target_id = analysis
        .targets
        .iter()
        .find(|target| target.presentation.is_default)
        .map(|target| target.presentation.id.clone());
    let target = default_target_id.as_ref().and_then(|id| {
        analysis
            .targets
            .iter()
            .find(|target| &target.presentation.id == id)
    });
    let required = target
        .map(|target| {
            target
                .presentation
                .required_toolchains
                .iter()
                .cloned()
                .collect::<HashSet<_>>()
        })
        .unwrap_or_default();
    let validation_issues = required
        .into_iter()
        .filter(|kind| analysis.toolchains.get(kind).is_none_or(Vec::is_empty))
        .map(|kind| ProjectRunValidationIssue {
            id: format!(
                "error:{}:{kind}",
                default_target_id.as_deref().unwrap_or_default()
            ),
            kind: "error",
            message: format!("没有发现可用的 {kind} 工具链"),
            target_id: default_target_id.clone(),
            target_label: target.map(|target| target.presentation.label.clone()),
            path: None,
            hint: Some(format!(
                "请安装 {kind}，或稍后在工具链设置中选择可执行文件。"
            )),
        })
        .collect();
    let toolchain_options = analysis
        .toolchains
        .iter()
        .map(|(kind, options)| {
            (
                kind.clone(),
                options
                    .iter()
                    .map(|option| option.presentation.clone())
                    .collect(),
            )
        })
        .collect();
    let targets = analysis
        .targets
        .into_iter()
        .map(|target| target.presentation)
        .collect::<Vec<_>>();
    Ok(ProjectRunAnalysisSnapshot {
        catalog: ProjectRunCatalog {
            project_id: project_id.to_owned(),
            status: if targets.is_empty() { "empty" } else { "ready" },
            default_target_id,
            targets,
            error_message: None,
        },
        environment: ProjectRunEnvironment {
            toolchain_options,
            configuration_files: analysis.configuration_files,
            validation_issues,
            selected_toolchains: BTreeMap::new(),
            custom_toolchains: BTreeMap::new(),
            environment_variables: Vec::new(),
            terminal_ui_enabled: false,
        },
    })
}

async fn resolve_scope(
    app: &AppHandle,
    owner_user_id: String,
    project_id: String,
) -> Result<ProjectRunScope, String> {
    validate_owner(&owner_user_id)?;
    validate_identifier(&project_id)?;
    let app_data_directory = app
        .path()
        .app_data_dir()
        .map_err(|_| "无法读取项目运行环境。".to_owned())?;
    let root = resolved_project_root(&app_data_directory, &owner_user_id, &project_id).await?;
    Ok(ProjectRunScope {
        owner_user_id,
        project_id,
        project_root: root,
    })
}

async fn resolved_project_root(
    app_data_directory: &Path,
    owner_user_id: &str,
    project_id: &str,
) -> Result<PathBuf, String> {
    let connector = app_data_directory.join("NativeConnector");
    let owner = owner_user_id.to_owned();
    let project = project_id.to_owned();
    tauri::async_runtime::spawn_blocking(move || {
        resolve_project_root(&connector, &owner, &project).map(|resolved| resolved.root)
    })
    .await
    .map_err(|_| "项目运行目录解析任务异常退出。".to_owned())?
}

fn validate_identifier(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.trim() != value
        || value.len() > 1_024
        || value.chars().any(char::is_control)
    {
        Err("项目运行请求包含无效字段。".into())
    } else {
        Ok(())
    }
}
