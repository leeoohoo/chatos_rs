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

use std::collections::HashSet;
use std::path::PathBuf;

use crate::api::local_connectors::{
    call_local_mcp_tool, parse_local_connector_root_path, LocalConnectorRootRef,
    LOCAL_CONNECTOR_BUILTIN_CODE_READ,
};
use crate::core::time::now_rfc3339;
use crate::models::project::Project;
use crate::models::project_run::{ProjectRunCatalog, ProjectRunTarget};
use serde_json::{json, Value};

pub(crate) use change_detection::{classify_project_run_path_change, ProjectRunPathChangeKind};
pub(super) use go::detect_go_entrypoints;
pub(super) use rust::detect_rust_bins;
pub(super) use target_model::{is_same_cwd, normalized_cwd};

use scan::detect_targets_sync;

pub(crate) async fn analyze_project(project: &Project) -> ProjectRunCatalog {
    if let Some(root_ref) = parse_local_connector_root_path(project.root_path.as_str()) {
        return analyze_local_connector_project(project, root_ref).await;
    }

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

async fn analyze_local_connector_project(
    project: &Project,
    root_ref: LocalConnectorRootRef,
) -> ProjectRunCatalog {
    let project_id = project.id.clone();
    let user_id = project.user_id.clone();
    let now = now_rfc3339();

    let root_listing = match call_local_mcp_tool(
        root_ref.device_id.as_str(),
        root_ref.workspace_id.as_str(),
        root_ref.relative_path.as_deref(),
        &[LOCAL_CONNECTOR_BUILTIN_CODE_READ],
        "list_dir",
        json!({ "path": ".", "max_entries": 1000 }),
    )
    .await
    {
        Ok(value) => value,
        Err(err) => {
            return build_error_catalog(
                project_id,
                user_id,
                now,
                format!(
                    "Local Connector 项目分析失败: {}",
                    connector_error_message(err)
                ),
            );
        }
    };

    let mut targets = detect_local_connector_node_targets(project, &root_ref, &root_listing).await;
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

async fn detect_local_connector_node_targets(
    project: &Project,
    root_ref: &LocalConnectorRootRef,
    root_listing: &Value,
) -> Vec<ProjectRunTarget> {
    let root_entries = local_listing_entry_names(root_listing);
    if !root_entries.contains("package.json") {
        return Vec::new();
    }
    let package_json = match call_local_mcp_tool(
        root_ref.device_id.as_str(),
        root_ref.workspace_id.as_str(),
        root_ref.relative_path.as_deref(),
        &[LOCAL_CONNECTOR_BUILTIN_CODE_READ],
        "read_file_raw",
        json!({ "path": "package.json", "with_line_numbers": false }),
    )
    .await
    {
        Ok(value) => value,
        Err(err) => {
            tracing::warn!(
                error = %connector_error_message(err),
                project_id = project.id.as_str(),
                "Local Connector package.json read failed"
            );
            return Vec::new();
        }
    };
    let Some(content) = package_json.get("content").and_then(Value::as_str) else {
        return Vec::new();
    };
    let Ok(package) = serde_json::from_str::<Value>(content) else {
        return Vec::new();
    };
    let Some(scripts) = package.get("scripts").and_then(Value::as_object) else {
        return Vec::new();
    };
    let package_manager = detect_local_node_package_manager(root_entries, &package);
    let manifest_path = format!("{}/package.json", project.root_path.trim_end_matches('/'));
    let mut script_names = scripts
        .iter()
        .filter_map(|(name, value)| {
            let script = value.as_str()?.trim();
            (!name.trim().is_empty() && !script.is_empty()).then(|| name.clone())
        })
        .collect::<Vec<_>>();
    script_names.sort_by(|left, right| {
        local_node_script_priority(left.as_str())
            .cmp(&local_node_script_priority(right.as_str()))
            .then_with(|| left.cmp(right))
    });

    script_names
        .into_iter()
        .map(|script| {
            let command = format!("{package_manager} run {script}");
            ProjectRunTarget {
                id: format!(
                    "local_connector_node_{}",
                    local_target_id_suffix(script.as_str())
                ),
                label: format!("{package_manager} run {script}"),
                kind: "node".to_string(),
                language: Some("JavaScript".to_string()),
                cwd: project.root_path.clone(),
                command,
                source: "local_connector_package_json".to_string(),
                confidence: 0.82,
                is_default: false,
                entrypoint: None,
                manifest_path: Some(manifest_path.clone()),
                required_toolchains: Vec::new(),
            }
        })
        .collect()
}

fn local_listing_entry_names(value: &Value) -> HashSet<String> {
    value
        .get("entries")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.get("name").and_then(Value::as_str))
        .map(ToOwned::to_owned)
        .collect()
}

fn detect_local_node_package_manager(entries: HashSet<String>, package: &Value) -> String {
    if entries.contains("pnpm-lock.yaml") {
        return "pnpm".to_string();
    }
    if entries.contains("yarn.lock") {
        return "yarn".to_string();
    }
    if entries.contains("bun.lockb") || entries.contains("bun.lock") {
        return "bun".to_string();
    }
    let package_manager = package
        .get("packageManager")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if package_manager.starts_with("pnpm@") {
        return "pnpm".to_string();
    }
    if package_manager.starts_with("yarn@") {
        return "yarn".to_string();
    }
    if package_manager.starts_with("bun@") {
        return "bun".to_string();
    }
    "npm".to_string()
}

fn local_node_script_priority(script: &str) -> i32 {
    match script {
        "dev" => 0,
        "start" => 1,
        "serve" => 2,
        "preview" => 3,
        "build" => 4,
        "test" => 5,
        _ => 20,
    }
}

fn local_target_id_suffix(value: &str) -> String {
    let suffix = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string();
    if suffix.is_empty() {
        "script".to_string()
    } else {
        suffix
    }
}

fn connector_error_message(err: (axum::http::StatusCode, axum::Json<Value>)) -> String {
    let (status, axum::Json(value)) = err;
    value
        .get("error")
        .and_then(Value::as_str)
        .map(|message| format!("{message} ({status})"))
        .unwrap_or_else(|| format!("{value} ({status})"))
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
