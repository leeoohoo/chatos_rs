// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::path::Path;

use serde::Serialize;
use serde_json::{json, Value};

use crate::models::{ProjectRecord, ProjectSourceType};

use super::LOCAL_CONNECTOR_ROOT_PREFIX;

#[derive(Debug)]
pub(super) struct LocalProjectInspection {
    pub(super) detected_stack: Value,
    pub(super) required_services: Value,
    pub(super) manifest_context: Vec<ManifestContextFile>,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct ManifestContextFile {
    path: String,
    content_preview: String,
}

pub(super) fn inspect_local_project(project: &ProjectRecord) -> Option<LocalProjectInspection> {
    if !matches!(project.source_type, ProjectSourceType::Local) {
        return None;
    }
    let root_path = project.root_path.as_deref()?.trim();
    if root_path.starts_with(LOCAL_CONNECTOR_ROOT_PREFIX) {
        return None;
    }
    let root = Path::new(root_path);
    if !root.is_dir() {
        return None;
    }
    let entries = fs::read_dir(root).ok()?;
    let names = entries
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect::<Vec<_>>();
    let mut languages = Vec::new();
    let mut manifests = Vec::new();
    push_marker(
        &names,
        "package.json",
        &mut manifests,
        "node",
        &mut languages,
    );
    push_marker(
        &names,
        "pnpm-lock.yaml",
        &mut manifests,
        "node",
        &mut languages,
    );
    push_marker(&names, "Cargo.toml", &mut manifests, "rust", &mut languages);
    push_marker(
        &names,
        "pyproject.toml",
        &mut manifests,
        "python",
        &mut languages,
    );
    push_marker(
        &names,
        "requirements.txt",
        &mut manifests,
        "python",
        &mut languages,
    );
    push_marker(&names, "go.mod", &mut manifests, "go", &mut languages);
    push_marker(&names, "pom.xml", &mut manifests, "java", &mut languages);
    push_marker(
        &names,
        "build.gradle",
        &mut manifests,
        "java",
        &mut languages,
    );
    push_marker(
        &names,
        "build.gradle.kts",
        &mut manifests,
        "java",
        &mut languages,
    );

    let compose = ["docker-compose.yml", "docker-compose.yaml", "compose.yml"]
        .iter()
        .find_map(|name| {
            let path = root.join(name);
            path.is_file().then_some((name.to_string(), path))
        });
    let mut required_services = Vec::new();
    if let Some((manifest, path)) = compose {
        manifests.push(manifest);
        if let Ok(content) = fs::read_to_string(path) {
            for (service_type, aliases) in [
                ("redis", &["redis"] as &[_]),
                ("postgres", &["postgres", "postgresql"]),
                ("mysql", &["mysql", "mariadb"]),
                ("nacos", &["nacos"]),
                ("mongodb", &["mongo", "mongodb"]),
                ("rabbitmq", &["rabbitmq"]),
            ] {
                if aliases
                    .iter()
                    .any(|alias| content.to_ascii_lowercase().contains(alias))
                {
                    required_services.push(json!({
                        "type": service_type,
                        "source": "docker_compose"
                    }));
                }
            }
        }
    }

    languages.sort();
    languages.dedup();
    manifests.sort();
    manifests.dedup();
    Some(LocalProjectInspection {
        detected_stack: json!({
            "languages": languages,
            "manifests": manifests,
            "source": "project_management_agent_preflight"
        }),
        required_services: Value::Array(required_services),
        manifest_context: collect_manifest_context(root),
    })
}

fn collect_manifest_context(root: &Path) -> Vec<ManifestContextFile> {
    let mut files = Vec::new();
    let mut remaining = 24_000usize;
    for relative_path in [
        "package.json",
        "pnpm-lock.yaml",
        "yarn.lock",
        "Cargo.toml",
        "pyproject.toml",
        "requirements.txt",
        "go.mod",
        "pom.xml",
        "build.gradle",
        "build.gradle.kts",
        "docker-compose.yml",
        "docker-compose.yaml",
        "compose.yml",
        ".env.example",
    ] {
        if remaining == 0 {
            break;
        }
        let path = root.join(relative_path);
        if !path.is_file() {
            continue;
        }
        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };
        let preview = truncate_chars(content.as_str(), remaining.min(6_000));
        remaining = remaining.saturating_sub(preview.len());
        files.push(ManifestContextFile {
            path: relative_path.to_string(),
            content_preview: preview,
        });
    }
    files
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let mut out = value.chars().take(max_chars).collect::<String>();
    out.push_str("\n...[truncated]");
    out
}

fn push_marker(
    names: &[String],
    marker: &str,
    manifests: &mut Vec<String>,
    language: &str,
    languages: &mut Vec<String>,
) {
    if names.iter().any(|name| name == marker) {
        manifests.push(marker.to_string());
        languages.push(language.to_string());
    }
}
