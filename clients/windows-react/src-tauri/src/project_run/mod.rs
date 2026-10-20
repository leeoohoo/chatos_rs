mod analyzer;
mod models;
mod preferences;
mod runtime;
#[cfg(test)]
mod tests;

use analyzer::{analyze_project, resolve_project_run_target};
use models::{
    ProjectRunAnalysis, ProjectRunAnalysisSnapshot, ProjectRunCatalog, ProjectRunChangeSnapshot,
    ProjectRunEnvironment, ProjectRunEnvironmentVariable, ProjectRunState,
    ProjectRunValidationIssue,
};
pub(crate) use preferences::ProjectRunPreferencesStore;
use preferences::{ProjectRunEnvironmentMutation, ProjectRunSelection};
pub(crate) use runtime::ProjectRunManager;
use runtime::ProjectRunScope;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, State};

use crate::{host_lifecycle::validate_owner, platform_execution_context::resolve_project_root};

#[tauri::command]
pub(crate) async fn analyze_project_run(
    app: AppHandle,
    preferences: State<'_, ProjectRunPreferencesStore>,
    owner_user_id: String,
    project_id: String,
) -> Result<ProjectRunAnalysisSnapshot, String> {
    validate_owner(&owner_user_id)?;
    validate_identifier(&project_id)?;
    let app_data_directory = app
        .path()
        .app_data_dir()
        .map_err(|_| "无法读取项目运行环境。".to_owned())?;
    let preferences = preferences.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        analyze_resolved_project_with_preferences(
            &app_data_directory,
            &preferences,
            &owner_user_id,
            &project_id,
        )
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
    preferences: State<'_, ProjectRunPreferencesStore>,
    owner_user_id: String,
    project_id: String,
    target_id: String,
) -> Result<ProjectRunState, String> {
    validate_identifier(&target_id)?;
    let scope = resolve_scope(&app, owner_user_id, project_id).await?;
    let root = scope.project_root.clone();
    let owner = scope.owner_user_id.clone();
    let project = scope.project_id.clone();
    let preferences = preferences.inner().clone();
    let target = tauri::async_runtime::spawn_blocking(move || {
        let launch = preferences.launch_preferences(&owner, &project)?;
        let mut target =
            resolve_project_run_target(&root, &target_id, &launch.selection.selected_toolchains)?;
        target.environment = launch
            .environment
            .into_iter()
            .map(|(key, value)| (key.into(), value.into()))
            .collect();
        Ok::<_, String>(target)
    })
    .await
    .map_err(|_| "项目运行分析任务异常退出。".to_owned())??;
    manager.start(&scope, target).await
}

#[tauri::command]
pub(crate) async fn set_project_run_default_target(
    app: AppHandle,
    preferences: State<'_, ProjectRunPreferencesStore>,
    owner_user_id: String,
    project_id: String,
    target_id: String,
) -> Result<ProjectRunAnalysisSnapshot, String> {
    validate_identifier(&target_id)?;
    let scope = resolve_scope(&app, owner_user_id, project_id).await?;
    let preferences = preferences.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let analysis = analyze_project(&scope.project_root)?;
        if !analysis
            .targets
            .iter()
            .any(|target| target.presentation.id == target_id)
        {
            return Err("运行目标不存在，请重新分析项目。".into());
        }
        preferences.set_default_target(&scope.owner_user_id, &scope.project_id, target_id)?;
        let (selection, environment) =
            preferences.projection(&scope.owner_user_id, &scope.project_id)?;
        Ok(project_run_snapshot(
            &scope.project_id,
            analysis,
            selection,
            environment,
        ))
    })
    .await
    .map_err(|_| "项目运行偏好保存任务异常退出。".to_owned())?
}

#[tauri::command]
pub(crate) async fn update_project_run_environment(
    app: AppHandle,
    preferences: State<'_, ProjectRunPreferencesStore>,
    owner_user_id: String,
    project_id: String,
    selected_toolchains: BTreeMap<String, String>,
    environment_variables: Vec<ProjectRunEnvironmentMutation>,
) -> Result<ProjectRunAnalysisSnapshot, String> {
    let scope = resolve_scope(&app, owner_user_id, project_id).await?;
    let preferences = preferences.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let analysis = analyze_project(&scope.project_root)?;
        validate_toolchain_selection(&analysis, &selected_toolchains)?;
        preferences.update_environment(
            &scope.owner_user_id,
            &scope.project_id,
            selected_toolchains,
            environment_variables,
        )?;
        let (selection, environment) =
            preferences.projection(&scope.owner_user_id, &scope.project_id)?;
        Ok(project_run_snapshot(
            &scope.project_id,
            analysis,
            selection,
            environment,
        ))
    })
    .await
    .map_err(|_| "项目运行偏好保存任务异常退出。".to_owned())?
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

#[cfg(test)]
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
    Ok(project_run_snapshot(
        project_id,
        analysis,
        ProjectRunSelection::default(),
        Vec::new(),
    ))
}

fn analyze_resolved_project_with_preferences(
    app_data_directory: &Path,
    preferences: &ProjectRunPreferencesStore,
    owner_user_id: &str,
    project_id: &str,
) -> Result<ProjectRunAnalysisSnapshot, String> {
    let resolved = resolve_project_root(
        &app_data_directory.join("NativeConnector"),
        owner_user_id,
        project_id,
    )?;
    let analysis = analyze_project(&resolved.root)?;
    let (selection, environment) = preferences.projection(owner_user_id, project_id)?;
    Ok(project_run_snapshot(
        project_id,
        analysis,
        selection,
        environment,
    ))
}

fn project_run_snapshot(
    project_id: &str,
    analysis: ProjectRunAnalysis,
    selection: ProjectRunSelection,
    environment_variables: Vec<ProjectRunEnvironmentVariable>,
) -> ProjectRunAnalysisSnapshot {
    let analyzed_default = analysis
        .targets
        .iter()
        .find(|target| target.presentation.is_default)
        .map(|target| target.presentation.id.clone());
    let default_target_id = selection
        .default_target_id
        .filter(|selected| {
            analysis
                .targets
                .iter()
                .any(|target| target.presentation.id == *selected)
        })
        .or(analyzed_default);
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
    let mut validation_issues = required
        .into_iter()
        .filter(|kind| {
            let options = analysis.toolchains.get(kind);
            if let Some(selected) = selection.selected_toolchains.get(kind) {
                options.is_none_or(|options| {
                    !options
                        .iter()
                        .any(|option| option.presentation.id == *selected)
                })
            } else {
                options.is_none_or(Vec::is_empty)
            }
        })
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
        .collect::<Vec<_>>();
    validation_issues.extend(
        environment_variables
            .iter()
            .filter(|variable| !variable.has_value)
            .map(|variable| ProjectRunValidationIssue {
                id: format!("error:environment:{}", variable.key),
                kind: "error",
                message: format!("环境变量 {} 的安全值不可用", variable.key),
                target_id: None,
                target_label: None,
                path: Some(variable.key.clone()),
                hint: Some("请重新输入该环境变量的值。".into()),
            }),
    );
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
        .map(|target| {
            let mut target = target.presentation;
            target.is_default = Some(&target.id) == default_target_id.as_ref();
            target
        })
        .collect::<Vec<_>>();
    ProjectRunAnalysisSnapshot {
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
            selected_toolchains: selection.selected_toolchains,
            custom_toolchains: BTreeMap::new(),
            environment_variables,
            terminal_ui_enabled: false,
        },
    }
}

fn validate_toolchain_selection(
    analysis: &ProjectRunAnalysis,
    selected: &BTreeMap<String, String>,
) -> Result<(), String> {
    for (kind, selected_id) in selected {
        let exists = analysis.toolchains.get(kind).is_some_and(|options| {
            options
                .iter()
                .any(|option| option.presentation.id == *selected_id)
        });
        if !exists {
            return Err(format!("所选 {kind} 工具链已不可用，请重新选择。"));
        }
    }
    Ok(())
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
