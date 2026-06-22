use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::core::time::now_rfc3339;
use crate::models::project::Project;
use crate::models::project_run::ProjectRunCatalog;
use crate::models::project_run_environment::{
    ProjectRunCustomToolchain, ProjectRunEnvironmentSelection, ProjectRunEnvironmentSnapshot,
};
use crate::repositories::project_run_catalogs;
use crate::repositories::project_run_environment_settings;

use super::cache::{
    read_cached_catalog, read_cached_environment_selection, read_cached_environment_snapshot,
    write_cached_environment_selection, write_cached_environment_snapshot,
};
use super::environment_discovery::{collect_project_config_files, discover_toolchain_options};
use super::environment_support::{
    infer_version_suffix, normalize_string, normalized_selected_toolchain_id, resolve_user_path,
};
use super::environment_validation::validate_project_run_target;

fn build_environment_snapshot(
    project: &Project,
    selection: Option<ProjectRunEnvironmentSelection>,
    analyzed: ProjectRunCatalog,
) -> Result<ProjectRunEnvironmentSnapshot, String> {
    let options_by_kind = discover_toolchain_options(project, selection.as_ref());
    let project_root = PathBuf::from(resolve_user_path(project.root_path.as_str()));
    let config_files =
        collect_project_config_files(project_root.as_path(), analyzed.targets.as_slice());
    let validation_issues = analyzed
        .targets
        .iter()
        .flat_map(|target| {
            validate_project_run_target(
                project_root.as_path(),
                target,
                selection.as_ref(),
                &options_by_kind,
            )
        })
        .collect();

    Ok(ProjectRunEnvironmentSnapshot {
        project_id: project.id.clone(),
        user_id: project.user_id.clone(),
        options_by_kind,
        config_files,
        validation_issues,
        selected_toolchains: selection
            .as_ref()
            .map(|value| {
                value
                    .selected_toolchains
                    .iter()
                    .map(|(kind, id)| {
                        (
                            kind.clone(),
                            normalized_selected_toolchain_id(
                                kind.as_str(),
                                id.as_str(),
                                &value.custom_toolchains,
                            ),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default(),
        custom_toolchains: selection
            .as_ref()
            .map(|value| value.custom_toolchains.clone())
            .unwrap_or_default(),
        env_vars: selection
            .as_ref()
            .map(|value| value.env_vars.clone())
            .unwrap_or_default(),
        terminal_ui_enabled: selection
            .as_ref()
            .map(|value| value.terminal_ui_enabled)
            .unwrap_or(true),
        updated_at: selection.map(|value| value.updated_at),
    })
}

pub(crate) async fn refresh_environment_snapshot(
    project: &Project,
) -> Result<ProjectRunEnvironmentSnapshot, String> {
    let selection =
        match project_run_environment_settings::get_by_project_id(project.id.as_str()).await? {
            Some(selection) => {
                let _ = write_cached_environment_selection(project.root_path.as_str(), &selection);
                Some(selection)
            }
            None => read_cached_environment_selection(project.root_path.as_str())?,
        };
    let analyzed =
        match project_run_catalogs::get_catalog_by_project_id(project.id.as_str()).await? {
            Some(cached) => cached,
            None => read_cached_catalog(project.root_path.as_str())?.unwrap_or(ProjectRunCatalog {
                project_id: project.id.clone(),
                user_id: project.user_id.clone(),
                status: "empty".to_string(),
                default_target_id: None,
                targets: vec![],
                error_message: None,
                analyzed_at: None,
                updated_at: now_rfc3339(),
            }),
        };
    let snapshot = build_environment_snapshot(project, selection, analyzed)?;
    let _ = write_cached_environment_snapshot(project.root_path.as_str(), &snapshot);
    Ok(snapshot)
}

pub(crate) async fn load_environment_snapshot(
    project: &Project,
) -> Result<ProjectRunEnvironmentSnapshot, String> {
    if let Some(cached) = read_cached_environment_snapshot(project.root_path.as_str())? {
        return Ok(cached);
    }
    refresh_environment_snapshot(project).await
}

pub(crate) async fn save_environment_selection(
    project: &Project,
    selected_toolchains: HashMap<String, String>,
    custom_toolchains: HashMap<String, ProjectRunCustomToolchain>,
    env_vars: HashMap<String, String>,
    terminal_ui_enabled: bool,
) -> Result<ProjectRunEnvironmentSelection, String> {
    let normalized_custom_toolchains = custom_toolchains
        .into_iter()
        .filter_map(|(map_kind, custom)| {
            let kind = normalize_string(custom.kind.as_str());
            let normalized_kind = if kind.is_empty() {
                normalize_string(map_kind.as_str())
            } else {
                kind
            };
            let path = resolve_user_path(custom.path.as_str());
            if normalized_kind.is_empty() || path.is_empty() {
                return None;
            }
            let label = normalize_string(custom.label.as_str());
            Some((
                normalized_kind.clone(),
                ProjectRunCustomToolchain {
                    kind: normalized_kind,
                    label: if label.is_empty() {
                        format!(
                            "手动指定: {}",
                            infer_version_suffix(Path::new(path.as_str()))
                        )
                    } else {
                        label
                    },
                    path,
                },
            ))
        })
        .collect::<HashMap<_, _>>();

    let normalized_selected_toolchains = selected_toolchains
        .into_iter()
        .map(|(kind, id)| {
            let normalized_kind = normalize_string(kind.as_str());
            let normalized_id = normalized_selected_toolchain_id(
                normalized_kind.as_str(),
                id.as_str(),
                &normalized_custom_toolchains,
            );
            (normalized_kind, normalized_id)
        })
        .filter(|(kind, id)| !kind.is_empty() && !id.is_empty())
        .collect();

    let normalized_env_vars = env_vars
        .into_iter()
        .map(|(key, value)| (normalize_string(key.as_str()), value))
        .filter(|(key, _)| !key.is_empty())
        .collect();

    let selection = ProjectRunEnvironmentSelection {
        project_id: project.id.clone(),
        user_id: project.user_id.clone(),
        selected_toolchains: normalized_selected_toolchains,
        custom_toolchains: normalized_custom_toolchains,
        env_vars: normalized_env_vars,
        terminal_ui_enabled,
        updated_at: now_rfc3339(),
    };
    let saved = project_run_environment_settings::upsert(&selection).await?;
    let _ = write_cached_environment_selection(project.root_path.as_str(), &saved);
    Ok(saved)
}

pub(crate) async fn load_environment_selection(
    project_id: &str,
) -> Result<Option<ProjectRunEnvironmentSelection>, String> {
    project_run_environment_settings::get_by_project_id(project_id).await
}
