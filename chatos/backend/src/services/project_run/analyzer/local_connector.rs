// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{HashSet, VecDeque};

use serde_json::{json, Value};

use crate::api::local_connectors::{
    call_local_mcp_tool, LocalConnectorRootRef, LOCAL_CONNECTOR_BUILTIN_CODE_READ,
};
use crate::core::time::now_rfc3339;
use crate::models::project::Project;
use crate::models::project_run::{ProjectRunCatalog, ProjectRunTarget};

use super::scan_budget::ScanBudget;
use super::target_model::MAX_TARGETS;
use super::{build_error_catalog, target_model};

const MAX_SCAN_DIRS: usize = 2500;
const MAX_SCAN_DEPTH: usize = 6;
const MAX_DIRECTORY_ENTRIES: usize = 1000;

const IGNORED_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "dist",
    "build",
    ".next",
    ".venv",
    "venv",
    "target",
    ".idea",
    ".vscode",
    ".chatos",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LocalConnectorDirectoryEntry {
    pub(super) name: String,
    pub(super) path: String,
    pub(super) is_dir: bool,
}

pub(super) async fn analyze_local_connector_project(
    project: &Project,
    root_ref: LocalConnectorRootRef,
) -> ProjectRunCatalog {
    let project_id = project.id.clone();
    let user_id = project.user_id.clone();
    let now = now_rfc3339();

    let mut targets = match detect_local_connector_targets(project, &root_ref).await {
        Ok(targets) => targets,
        Err(err) => {
            return build_error_catalog(
                project_id,
                user_id,
                now,
                format!("Local Connector 项目分析失败: {err}"),
            );
        }
    };
    sort_local_connector_targets(&mut targets);
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

async fn detect_local_connector_targets(
    project: &Project,
    root_ref: &LocalConnectorRootRef,
) -> Result<Vec<ProjectRunTarget>, String> {
    let mut budget = ScanBudget::for_project_run_analysis();
    let mut targets = Vec::new();
    let mut queue = VecDeque::from([(".".to_string(), 0usize)]);
    let mut visited = 0usize;

    while let Some((relative_dir, depth)) = queue.pop_front() {
        if visited >= MAX_SCAN_DIRS || targets.len() >= MAX_TARGETS {
            break;
        }
        budget.account_entry()?;
        visited += 1;

        let listing = match list_local_connector_directory(project, root_ref, &relative_dir).await {
            Ok(value) => value,
            Err(err) if relative_dir == "." => return Err(err),
            Err(err) => {
                tracing::warn!(
                    error = %err,
                    project_id = project.id.as_str(),
                    path = relative_dir.as_str(),
                    "Local Connector project subdirectory scan failed"
                );
                continue;
            }
        };
        let entries = local_listing_entries(&listing, relative_dir.as_str());
        for entry in &entries {
            budget.account_entry()?;
            if entry.is_dir
                && depth < MAX_SCAN_DEPTH
                && !is_ignored_local_connector_dir(entry.name.as_str())
            {
                queue.push_back((entry.path.clone(), depth + 1));
            }
        }

        let entry_names = entries
            .iter()
            .map(|entry| entry.name.to_ascii_lowercase())
            .collect::<HashSet<_>>();
        detect_local_connector_node_targets(
            project,
            root_ref,
            relative_dir.as_str(),
            &entry_names,
            &mut targets,
        )
        .await;
        detect_local_connector_java_targets(
            project,
            root_ref,
            relative_dir.as_str(),
            &entry_names,
            &mut targets,
        )
        .await;
    }

    Ok(targets)
}

async fn list_local_connector_directory(
    project: &Project,
    root_ref: &LocalConnectorRootRef,
    path: &str,
) -> Result<Value, String> {
    call_local_mcp_tool(
        root_ref.device_id.as_str(),
        root_ref.workspace_id.as_str(),
        root_ref.relative_path.as_deref(),
        &[LOCAL_CONNECTOR_BUILTIN_CODE_READ],
        "list_dir",
        json!({ "path": path, "max_entries": MAX_DIRECTORY_ENTRIES }),
    )
    .await
    .map_err(|err| {
        format!(
            "读取项目目录 {} 失败: {}",
            local_connector_project_path(project.root_path.as_str(), path),
            connector_error_message(err)
        )
    })
}

async fn detect_local_connector_node_targets(
    project: &Project,
    root_ref: &LocalConnectorRootRef,
    relative_dir: &str,
    entries: &HashSet<String>,
    targets: &mut Vec<ProjectRunTarget>,
) {
    if !entries.contains("package.json") {
        return;
    }
    let manifest_relative_path = local_relative_child_path(relative_dir, "package.json");
    let Some(content) =
        read_local_connector_text_file(project, root_ref, manifest_relative_path.as_str()).await
    else {
        return;
    };
    let cwd = local_connector_project_path(project.root_path.as_str(), relative_dir);
    let manifest_path =
        local_connector_project_path(project.root_path.as_str(), manifest_relative_path.as_str());
    push_local_connector_node_targets(
        cwd.as_str(),
        manifest_path.as_str(),
        entries,
        content.as_str(),
        targets,
    );
}

pub(super) fn push_local_connector_node_targets(
    cwd: &str,
    manifest_path: &str,
    entries: &HashSet<String>,
    package_content: &str,
    targets: &mut Vec<ProjectRunTarget>,
) {
    let Ok(package) = serde_json::from_str::<Value>(package_content) else {
        return;
    };
    let Some(scripts) = package.get("scripts").and_then(Value::as_object) else {
        return;
    };
    let package_manager = detect_local_node_package_manager(entries, &package);
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

    for script in script_names {
        let command = format!("{package_manager} run {script}");
        let mut target = target_model::build_target(
            cwd,
            format!("{package_manager} run {script}"),
            "node",
            command,
            0.82,
            Some(format!("package.json:scripts.{script}")),
            Some(manifest_path.to_string()),
            Vec::new(),
        );
        target.language = Some("JavaScript".to_string());
        target.source = "local_connector_package_json".to_string();
        target.required_toolchains = Vec::new();
        target_model::push_target(targets, target);
    }
}

async fn detect_local_connector_java_targets(
    project: &Project,
    root_ref: &LocalConnectorRootRef,
    relative_dir: &str,
    entries: &HashSet<String>,
    targets: &mut Vec<ProjectRunTarget>,
) {
    let cwd = local_connector_project_path(project.root_path.as_str(), relative_dir);

    if entries.contains("pom.xml") {
        let manifest_relative_path = local_relative_child_path(relative_dir, "pom.xml");
        let manifest_path = local_connector_project_path(
            project.root_path.as_str(),
            manifest_relative_path.as_str(),
        );
        let pom_content =
            read_local_connector_text_file(project, root_ref, manifest_relative_path.as_str())
                .await;
        push_local_connector_maven_targets(
            cwd.as_str(),
            manifest_path.as_str(),
            entries,
            pom_content.as_deref(),
            targets,
        );
    }

    let gradle_manifest = [
        "build.gradle",
        "build.gradle.kts",
        "settings.gradle",
        "settings.gradle.kts",
    ]
    .into_iter()
    .find(|name| entries.contains(*name));
    if let Some(manifest_name) = gradle_manifest {
        let manifest_relative_path = local_relative_child_path(relative_dir, manifest_name);
        let manifest_path = local_connector_project_path(
            project.root_path.as_str(),
            manifest_relative_path.as_str(),
        );
        let gradle_content =
            read_local_connector_text_file(project, root_ref, manifest_relative_path.as_str())
                .await;
        push_local_connector_gradle_targets(
            cwd.as_str(),
            manifest_path.as_str(),
            entries,
            gradle_content.as_deref(),
            targets,
        );
    }
}

async fn read_local_connector_text_file(
    project: &Project,
    root_ref: &LocalConnectorRootRef,
    path: &str,
) -> Option<String> {
    match call_local_mcp_tool(
        root_ref.device_id.as_str(),
        root_ref.workspace_id.as_str(),
        root_ref.relative_path.as_deref(),
        &[LOCAL_CONNECTOR_BUILTIN_CODE_READ],
        "read_file_raw",
        json!({ "path": path, "with_line_numbers": false }),
    )
    .await
    {
        Ok(value) => value
            .get("content")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        Err(err) => {
            tracing::warn!(
                error = %connector_error_message(err),
                project_id = project.id.as_str(),
                path,
                "Local Connector project manifest read failed"
            );
            None
        }
    }
}

pub(super) fn push_local_connector_maven_targets(
    cwd: &str,
    manifest_path: &str,
    entries: &HashSet<String>,
    pom_content: Option<&str>,
    targets: &mut Vec<ProjectRunTarget>,
) {
    let runner = if entries.contains("mvnw") {
        "./mvnw"
    } else {
        "mvn"
    };
    let manifest_path = Some(manifest_path.to_string());
    let pom = pom_content.unwrap_or_default();
    let main_classes = local_java_main_classes_from_text(pom);
    let has_spring_boot = local_manifest_has_spring_boot(pom);

    if has_spring_boot {
        let command = main_classes
            .first()
            .map(|main_class| {
                format!("{runner} -Dspring-boot.run.main-class={main_class} spring-boot:run")
            })
            .unwrap_or_else(|| format!("{runner} spring-boot:run"));
        let label = main_classes
            .first()
            .map(|main_class| format!("Java(Maven): {main_class}"))
            .unwrap_or_else(|| "Java(Maven): spring-boot:run".to_string());
        push_local_connector_target(
            targets,
            cwd,
            label,
            command,
            0.92,
            main_classes.first().cloned(),
            manifest_path.clone(),
            "local_connector_maven",
        );
    } else if let Some(main_class) = main_classes.first() {
        push_local_connector_target(
            targets,
            cwd,
            format!("Java(Maven): {main_class}"),
            format!("{runner} -Dexec.mainClass={main_class} exec:java"),
            0.88,
            Some(main_class.clone()),
            manifest_path.clone(),
            "local_connector_maven",
        );
    }

    push_local_connector_target(
        targets,
        cwd,
        "Java(Maven): test".to_string(),
        format!("{runner} test"),
        0.72,
        None,
        manifest_path,
        "local_connector_maven",
    );
}

fn push_local_connector_gradle_targets(
    cwd: &str,
    manifest_path: &str,
    entries: &HashSet<String>,
    gradle_content: Option<&str>,
    targets: &mut Vec<ProjectRunTarget>,
) {
    let runner = if entries.contains("gradlew") {
        "./gradlew"
    } else {
        "gradle"
    };
    let manifest_path = Some(manifest_path.to_string());
    let gradle = gradle_content.unwrap_or_default();
    let has_spring_boot = local_manifest_has_spring_boot(gradle);
    let main_classes = local_java_main_classes_from_text(gradle);

    if has_spring_boot {
        push_local_connector_target(
            targets,
            cwd,
            "Java(Gradle): bootRun".to_string(),
            format!("{runner} bootRun"),
            0.9,
            main_classes.first().cloned(),
            manifest_path.clone(),
            "local_connector_gradle",
        );
    } else if gradle.contains("application") || !main_classes.is_empty() {
        push_local_connector_target(
            targets,
            cwd,
            main_classes
                .first()
                .map(|main_class| format!("Java(Gradle): {main_class}"))
                .unwrap_or_else(|| "Java(Gradle): run".to_string()),
            format!("{runner} run"),
            0.82,
            main_classes.first().cloned(),
            manifest_path.clone(),
            "local_connector_gradle",
        );
    }

    push_local_connector_target(
        targets,
        cwd,
        "Java(Gradle): test".to_string(),
        format!("{runner} test"),
        0.7,
        None,
        manifest_path,
        "local_connector_gradle",
    );
}

fn push_local_connector_target(
    targets: &mut Vec<ProjectRunTarget>,
    cwd: &str,
    label: String,
    command: String,
    confidence: f64,
    entrypoint: Option<String>,
    manifest_path: Option<String>,
    source: &str,
) {
    let mut target = target_model::build_target(
        cwd,
        label,
        "java",
        command,
        confidence,
        entrypoint,
        manifest_path,
        Vec::new(),
    );
    target.source = source.to_string();
    target.required_toolchains = Vec::new();
    target_model::push_target(targets, target);
}

fn local_manifest_has_spring_boot(content: &str) -> bool {
    let lower = content.to_ascii_lowercase();
    lower.contains("spring-boot")
        || lower.contains("org.springframework.boot")
        || lower.contains("springboot")
}

fn local_java_main_classes_from_text(content: &str) -> Vec<String> {
    let mut classes = Vec::new();
    let mut seen = HashSet::new();
    for pattern in [
        "<mainClass>",
        "<main-class>",
        "mainClass",
        "main-class",
        "main_class",
    ] {
        for class_name in local_values_after_marker(content, pattern) {
            if seen.insert(class_name.clone()) {
                classes.push(class_name);
            }
        }
    }
    classes.sort();
    classes
}

fn local_values_after_marker(content: &str, marker: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut remaining = content;
    while let Some(index) = remaining.find(marker) {
        remaining = &remaining[index + marker.len()..];
        let value = if marker.starts_with('<') {
            remaining
                .split('<')
                .next()
                .map(str::trim)
                .unwrap_or_default()
                .to_string()
        } else {
            let trimmed = remaining.trim_start_matches(|ch: char| {
                ch.is_whitespace() || matches!(ch, '=' | ':' | '"' | '\'')
            });
            trimmed
                .split(|ch: char| {
                    ch.is_whitespace() || matches!(ch, '"' | '\'' | ',' | ')' | '}' | '\n' | '\r')
                })
                .next()
                .unwrap_or_default()
                .trim_matches(|ch| matches!(ch, '"' | '\'' | ';'))
                .to_string()
        };
        if local_java_class_name_looks_valid(value.as_str()) {
            values.push(value);
        }
    }
    values
}

fn local_java_class_name_looks_valid(value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty()
        && trimmed.contains('.')
        && trimmed.split('.').all(|part| {
            let mut chars = part.chars();
            chars
                .next()
                .is_some_and(|ch| ch == '_' || ch.is_ascii_alphabetic())
                && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
        })
}

pub(super) fn sort_local_connector_targets(targets: &mut [ProjectRunTarget]) {
    targets.sort_by(|a, b| {
        local_connector_target_priority(b)
            .cmp(&local_connector_target_priority(a))
            .then_with(|| {
                b.confidence
                    .partial_cmp(&a.confidence)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| a.label.cmp(&b.label))
    });
}

fn local_connector_target_priority(target: &ProjectRunTarget) -> i32 {
    let command = target.command.to_ascii_lowercase();
    if target.kind == "node" && command.contains(" run dev") {
        return 100;
    }
    if target.kind == "node" && command.contains(" run start") {
        return 95;
    }
    if target.kind == "java" && command.contains("spring-boot:run") {
        return 92;
    }
    if target.kind == "java" && command.contains("bootrun") {
        return 90;
    }
    if target.kind == "java" && command.contains("exec:java") {
        return 88;
    }
    if command.contains("test") {
        return 40;
    }
    70
}

pub(super) fn local_listing_entries(
    value: &Value,
    relative_dir: &str,
) -> Vec<LocalConnectorDirectoryEntry> {
    value
        .get("entries")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let name = entry.get("name").and_then(Value::as_str)?.trim();
            if name.is_empty()
                || name == "."
                || name == ".."
                || name.contains('/')
                || name.contains('\\')
            {
                return None;
            }
            let is_dir = entry
                .get("is_dir")
                .and_then(Value::as_bool)
                .unwrap_or_else(|| entry.get("type").and_then(Value::as_str) == Some("dir"));
            Some(LocalConnectorDirectoryEntry {
                name: name.to_string(),
                path: local_relative_child_path(relative_dir, name),
                is_dir,
            })
        })
        .collect()
}

pub(super) fn is_ignored_local_connector_dir(name: &str) -> bool {
    let normalized = name.trim().to_ascii_lowercase();
    IGNORED_DIRS.contains(&normalized.as_str())
}

fn local_relative_child_path(parent: &str, child: &str) -> String {
    let parent = parent.trim().trim_matches('/');
    if parent.is_empty() || parent == "." {
        child.to_string()
    } else {
        format!("{parent}/{child}")
    }
}

fn local_connector_project_path(project_root: &str, relative_path: &str) -> String {
    let project_root = project_root.trim().trim_end_matches('/');
    let relative_path = relative_path.trim().trim_matches('/');
    if relative_path.is_empty() || relative_path == "." {
        project_root.to_string()
    } else {
        format!("{project_root}/{relative_path}")
    }
}

fn detect_local_node_package_manager(entries: &HashSet<String>, package: &Value) -> String {
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

fn connector_error_message(err: (axum::http::StatusCode, axum::Json<Value>)) -> String {
    let (status, axum::Json(value)) = err;
    value
        .get("error")
        .and_then(Value::as_str)
        .map(|message| format!("{message} ({status})"))
        .unwrap_or_else(|| format!("{value} ({status})"))
}
