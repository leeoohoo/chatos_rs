mod analyzer;
mod models;
#[cfg(test)]
mod tests;

use analyzer::analyze_project;
use models::{
    ProjectRunAnalysisSnapshot, ProjectRunCatalog, ProjectRunEnvironment, ProjectRunValidationIssue,
};
use std::collections::{BTreeMap, HashSet};
use tauri::{AppHandle, Manager};

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
