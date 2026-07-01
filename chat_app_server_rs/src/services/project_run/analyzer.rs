// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[path = "analyzer/change_detection.rs"]
mod change_detection;
#[path = "analyzer/go.rs"]
mod go;
#[path = "analyzer/java.rs"]
mod java;
#[path = "analyzer/node.rs"]
mod node;
#[path = "analyzer/python.rs"]
mod python;
#[path = "analyzer/rust.rs"]
mod rust;
#[path = "analyzer/scan.rs"]
mod scan;
#[path = "analyzer/scan_budget.rs"]
mod scan_budget;
#[path = "analyzer/target_model.rs"]
mod target_model;

use std::path::PathBuf;

use crate::core::time::now_rfc3339;
use crate::models::project::Project;
use crate::models::project_run::ProjectRunCatalog;

pub(crate) use change_detection::{classify_project_run_path_change, ProjectRunPathChangeKind};
pub(super) use go::detect_go_entrypoints;
pub(super) use rust::detect_rust_bins;
pub(super) use target_model::{is_same_cwd, normalized_cwd};

use scan::detect_targets_sync;

pub(crate) async fn analyze_project(project: &Project) -> ProjectRunCatalog {
    let project_id = project.id.clone();
    let user_id = project.user_id.clone();
    let now = now_rfc3339();
    let root_path = project.root_path.clone();

    let detected =
        tokio::task::spawn_blocking(move || detect_targets_sync(PathBuf::from(root_path))).await;

    match detected {
        Ok(Ok(mut targets)) => {
            let default_target_id = targets.first().map(|target| target.id.clone());
            if let Some(default_id) = default_target_id.as_deref() {
                for target in &mut targets {
                    target.is_default = target.id == default_id;
                }
            }
            ProjectRunCatalog {
                project_id,
                user_id,
                status: if targets.is_empty() {
                    "empty".to_string()
                } else {
                    "ready".to_string()
                },
                default_target_id,
                targets,
                error_message: None,
                analyzed_at: Some(now.clone()),
                updated_at: now,
            }
        }
        Ok(Err(err)) => build_error_catalog(project_id, user_id, now, err),
        Err(err) => build_error_catalog(
            project_id,
            user_id,
            now,
            format!("project run analysis task failed: {err}"),
        ),
    }
}

fn build_error_catalog(
    project_id: String,
    user_id: Option<String>,
    now: String,
    error_message: String,
) -> ProjectRunCatalog {
    tracing::warn!(error = %error_message, "project run target analysis failed");
    ProjectRunCatalog {
        project_id,
        user_id,
        status: "error".to_string(),
        default_target_id: None,
        targets: Vec::new(),
        error_message: Some(error_message),
        analyzed_at: Some(now.clone()),
        updated_at: now,
    }
}

pub(crate) fn apply_default_target(
    catalog: &ProjectRunCatalog,
    target_id: Option<&str>,
) -> Result<ProjectRunCatalog, String> {
    let Some(target_id) = target_id.map(str::trim).filter(|value| !value.is_empty()) else {
        return Err("target_id 不能为空".to_string());
    };
    let mut updated = catalog.clone();
    let mut found = false;
    for target in &mut updated.targets {
        let is_default = target.id == target_id;
        target.is_default = is_default;
        if is_default {
            found = true;
        }
    }
    if !found {
        return Err("target_id 不存在".to_string());
    }
    updated.default_target_id = Some(target_id.to_string());
    updated.updated_at = now_rfc3339();
    Ok(updated)
}
