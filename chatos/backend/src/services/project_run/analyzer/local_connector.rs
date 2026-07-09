// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use serde_json::{json, Value};

use crate::api::local_connectors::{
    call_local_mcp_tool, LocalConnectorRootRef, LOCAL_CONNECTOR_BUILTIN_CODE_READ,
};
use crate::core::time::now_rfc3339;
use crate::models::project::Project;
use crate::models::project_run::{ProjectRunCatalog, ProjectRunTarget};

use super::{build_error_catalog, target_model};

pub(super) async fn analyze_local_connector_project(
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

    let mut targets = Vec::new();
    targets.extend(detect_local_connector_node_targets(project, &root_ref, &root_listing).await);
    targets.extend(detect_local_connector_java_targets(project, &root_ref, &root_listing).await);
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

async fn detect_local_connector_java_targets(
    project: &Project,
    root_ref: &LocalConnectorRootRef,
    root_listing: &Value,
) -> Vec<ProjectRunTarget> {
    let root_entries = local_listing_entry_names(root_listing);
    let mut targets = Vec::new();

    if root_entries.contains("pom.xml") {
        let pom_content = read_local_connector_text_file(project, root_ref, "pom.xml").await;
        push_local_connector_maven_targets(
            project,
            &root_entries,
            pom_content.as_deref(),
            &mut targets,
        );
    }

    let gradle_manifest = [
        "build.gradle",
        "build.gradle.kts",
        "settings.gradle",
        "settings.gradle.kts",
    ]
    .into_iter()
    .find(|name| root_entries.contains(*name));
    if let Some(manifest_name) = gradle_manifest {
        let gradle_content = read_local_connector_text_file(project, root_ref, manifest_name).await;
        push_local_connector_gradle_targets(
            project,
            &root_entries,
            manifest_name,
            gradle_content.as_deref(),
            &mut targets,
        );
    }

    targets
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
    project: &Project,
    root_entries: &HashSet<String>,
    pom_content: Option<&str>,
    targets: &mut Vec<ProjectRunTarget>,
) {
    let runner = if root_entries.contains("mvnw") {
        "./mvnw"
    } else {
        "mvn"
    };
    let manifest_path = Some(format!(
        "{}/pom.xml",
        project.root_path.trim_end_matches('/')
    ));
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
            project.root_path.as_str(),
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
            project.root_path.as_str(),
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
        project.root_path.as_str(),
        "Java(Maven): test".to_string(),
        format!("{runner} test"),
        0.72,
        None,
        manifest_path,
        "local_connector_maven",
    );
}

fn push_local_connector_gradle_targets(
    project: &Project,
    root_entries: &HashSet<String>,
    manifest_name: &str,
    gradle_content: Option<&str>,
    targets: &mut Vec<ProjectRunTarget>,
) {
    let runner = if root_entries.contains("gradlew") {
        "./gradlew"
    } else {
        "gradle"
    };
    let manifest_path = Some(format!(
        "{}/{}",
        project.root_path.trim_end_matches('/'),
        manifest_name
    ));
    let gradle = gradle_content.unwrap_or_default();
    let has_spring_boot = local_manifest_has_spring_boot(gradle);
    let main_classes = local_java_main_classes_from_text(gradle);

    if has_spring_boot {
        push_local_connector_target(
            targets,
            project.root_path.as_str(),
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
            project.root_path.as_str(),
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
        project.root_path.as_str(),
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
