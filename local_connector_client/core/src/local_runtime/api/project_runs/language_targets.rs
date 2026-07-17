// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::path::Path;

use serde_json::Value;

use super::analysis::{push_config_file, push_target};

pub(super) fn add_node_targets(
    root: &Path,
    logical_root: &str,
    targets: &mut Vec<Value>,
    config_files: &mut Vec<Value>,
) {
    let manifest = root.join("package.json");
    let Ok(content) = fs::read_to_string(manifest.as_path()) else {
        return;
    };
    push_config_file(
        config_files,
        "node",
        "package.json",
        logical_root,
        content.as_str(),
    );
    let parsed = serde_json::from_str::<Value>(content.as_str()).unwrap_or(Value::Null);
    let scripts = parsed.get("scripts").and_then(Value::as_object);
    for script in ["dev", "start", "serve"] {
        if scripts.is_some_and(|scripts| scripts.contains_key(script)) {
            push_target(
                targets,
                format!("node:{script}"),
                format!("npm run {script}"),
                "node",
                "nodejs",
                logical_root,
                format!("npm run {script}"),
                "package.json",
            );
        }
    }
    if scripts.is_none_or(|scripts| {
        !["dev", "start", "serve"]
            .iter()
            .any(|key| scripts.contains_key(*key))
    }) {
        push_target(
            targets,
            "node:install".to_string(),
            "npm install".to_string(),
            "node",
            "nodejs",
            logical_root,
            "npm install".to_string(),
            "package.json",
        );
    }
}

pub(super) fn add_python_target(
    root: &Path,
    logical_root: &str,
    targets: &mut Vec<Value>,
    config_files: &mut Vec<Value>,
) {
    for manifest in ["pyproject.toml", "requirements.txt"] {
        if let Ok(content) = fs::read_to_string(root.join(manifest)) {
            push_config_file(
                config_files,
                "python",
                manifest,
                logical_root,
                content.as_str(),
            );
        }
    }
    let entrypoint = ["main.py", "app.py", "manage.py"]
        .into_iter()
        .find(|name| root.join(name).is_file());
    if let Some(entrypoint) = entrypoint {
        push_target(
            targets,
            format!("python:{entrypoint}"),
            format!("Python {entrypoint}"),
            "python",
            "python",
            logical_root,
            format!("python {entrypoint}"),
            entrypoint,
        );
    }
}
