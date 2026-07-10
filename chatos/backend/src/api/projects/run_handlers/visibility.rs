// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;
use std::collections::HashMap;

use crate::core::user_visible_path::display_path;
use crate::models::project_run::ProjectRunCatalog;
use crate::models::project_run_environment::{
    ProjectRunEnvironmentSnapshot, ProjectRunValidationIssue,
};
use crate::models::terminal::Terminal;

pub(super) fn serialize_project_run_terminal(terminal: &Terminal, busy: bool) -> Value {
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

pub(super) fn visible_project_run_catalog(mut catalog: ProjectRunCatalog) -> ProjectRunCatalog {
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

pub(super) fn visible_validation_issues(
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

pub(super) fn visible_project_run_environment(
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

pub(super) fn visible_env_overrides(
    env_overrides: HashMap<String, String>,
) -> HashMap<String, String> {
    env_overrides
        .into_iter()
        .map(|(key, value)| (key, visible_env_value(value.as_str())))
        .collect()
}
