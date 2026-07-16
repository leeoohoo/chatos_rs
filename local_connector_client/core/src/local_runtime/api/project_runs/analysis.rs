// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::path::Path;

use serde_json::{json, Value};

pub(super) fn analyze_project(root: &Path, logical_root: &str) -> Value {
    let mut targets = Vec::new();
    let mut config_files = Vec::new();

    super::language_targets::add_node_targets(root, logical_root, &mut targets, &mut config_files);
    add_manifest_target(
        root,
        logical_root,
        "Cargo.toml",
        "rust",
        "Rust",
        "cargo run",
        &mut targets,
        &mut config_files,
    );
    add_manifest_target(
        root,
        logical_root,
        "go.mod",
        "go",
        "Go",
        "go run .",
        &mut targets,
        &mut config_files,
    );
    add_manifest_target(
        root,
        logical_root,
        "pom.xml",
        "java",
        "Maven",
        maven_command(root),
        &mut targets,
        &mut config_files,
    );
    add_manifest_target(
        root,
        logical_root,
        "build.gradle",
        "java",
        "Gradle",
        gradle_command(root),
        &mut targets,
        &mut config_files,
    );
    add_manifest_target(
        root,
        logical_root,
        "build.gradle.kts",
        "kotlin",
        "Gradle",
        gradle_command(root),
        &mut targets,
        &mut config_files,
    );
    super::language_targets::add_python_target(root, logical_root, &mut targets, &mut config_files);
    add_manifest_target(
        root,
        logical_root,
        "Makefile",
        "make",
        "Make",
        "make",
        &mut targets,
        &mut config_files,
    );

    let now = crate::local_now_rfc3339();
    let default_target_id = targets
        .first()
        .and_then(|target| target.get("id"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    json!({
        "status": if targets.is_empty() { "empty" } else { "ready" },
        "default_target_id": default_target_id,
        "targets": targets,
        "config_files": config_files,
        "error_message": Value::Null,
        "analyzed_at": now,
        "updated_at": now,
    })
}

#[allow(clippy::too_many_arguments)]
fn add_manifest_target(
    root: &Path,
    logical_root: &str,
    manifest_name: &str,
    kind: &str,
    label: &str,
    command: &str,
    targets: &mut Vec<Value>,
    config_files: &mut Vec<Value>,
) {
    let manifest = root.join(manifest_name);
    let Ok(content) = fs::read_to_string(manifest.as_path()) else {
        return;
    };
    push_config_file(
        config_files,
        kind,
        manifest_name,
        logical_root,
        content.as_str(),
    );
    push_target(
        targets,
        format!("{kind}:default"),
        label.to_string(),
        kind,
        kind,
        logical_root,
        command.to_string(),
        manifest_name,
    );
}

#[allow(clippy::too_many_arguments)]
pub(super) fn push_target(
    targets: &mut Vec<Value>,
    id: String,
    label: String,
    kind: &str,
    language: &str,
    cwd: &str,
    command: String,
    manifest: &str,
) {
    if targets
        .iter()
        .any(|target| target.get("id").and_then(Value::as_str) == Some(id.as_str()))
    {
        return;
    }
    targets.push(json!({
        "id": id,
        "label": label,
        "kind": kind,
        "language": language,
        "cwd": cwd,
        "command": command,
        "source": "local_runtime",
        "confidence": 0.95,
        "is_default": targets.is_empty(),
        "entrypoint": manifest,
        "manifest_path": manifest,
        "required_toolchains": [],
    }));
}

pub(super) fn push_config_file(
    config_files: &mut Vec<Value>,
    kind: &str,
    name: &str,
    logical_root: &str,
    content: &str,
) {
    config_files.push(json!({
        "kind": kind,
        "label": name,
        "path": format!("{}/{}", logical_root.trim_end_matches('/'), name),
        "preview": content.chars().take(2_000).collect::<String>(),
        "source": "local_runtime",
    }));
}

fn maven_command(root: &Path) -> &'static str {
    if root.join("mvnw").is_file() {
        "./mvnw spring-boot:run"
    } else {
        "mvn spring-boot:run"
    }
}

fn gradle_command(root: &Path) -> &'static str {
    if root.join("gradlew").is_file() {
        "./gradlew run"
    } else {
        "gradle run"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_local_node_run_targets_without_cloud_services() {
        let root = std::env::temp_dir().join(format!(
            "chatos-local-run-analysis-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).expect("create project");
        fs::write(
            root.join("package.json"),
            r#"{"scripts":{"dev":"vite","start":"node server.js"}}"#,
        )
        .expect("write package manifest");

        let catalog = analyze_project(&root, "local://connector/device/workspace/project");

        let targets = catalog["targets"].as_array().expect("targets");
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0]["command"], "npm run dev");
        assert_eq!(catalog["status"], "ready");
        fs::remove_dir_all(root).expect("cleanup project");
    }
}
