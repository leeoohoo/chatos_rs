// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::Path;

use crate::models::project_run::ProjectRunTarget;
use crate::models::project_run_environment::ProjectRunConfigFileSummary;

use super::super::environment_support::{home_dir, normalize_path};
use super::super::file_limits::{read_to_string_limited, MAX_CONFIG_PREVIEW_BYTES};

fn read_preview_lines(path: &Path, max_lines: usize) -> Option<String> {
    let content = read_to_string_limited(path, MAX_CONFIG_PREVIEW_BYTES)?;
    let lines = content
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .filter(|line| !line.starts_with('#'))
        .take(max_lines)
        .collect::<Vec<_>>();
    if lines.is_empty() {
        None
    } else {
        Some(lines.join(" "))
    }
}

pub(in crate::services::project_run) fn collect_project_config_files(
    project_root: &Path,
    targets: &[ProjectRunTarget],
) -> Vec<ProjectRunConfigFileSummary> {
    let mut out = Vec::new();

    let has_java = targets.iter().any(|target| target.kind == "java");
    let has_node = targets.iter().any(|target| target.kind == "node");
    let has_python = targets.iter().any(|target| target.kind == "python");
    let has_go = targets.iter().any(|target| target.kind == "go");
    let has_rust = targets.iter().any(|target| target.kind == "rust");

    let mut candidates: Vec<(&str, &str, &str, &str)> = Vec::new();

    if has_java {
        candidates.extend([
            (
                "maven_config",
                "Maven Config",
                ".mvn/maven.config",
                "project-local",
            ),
            (
                "maven_jvm_config",
                "Maven JVM Config",
                ".mvn/jvm.config",
                "project-local",
            ),
            (
                "gradle_properties",
                "Gradle Properties",
                "gradle.properties",
                "project-local",
            ),
            (
                "gradle_properties",
                "Gradle Properties",
                ".gradle/gradle.properties",
                "project-local",
            ),
        ]);
    }

    if has_node {
        candidates.extend([
            (
                "package_json",
                "package.json",
                "package.json",
                "project-local",
            ),
            (
                "node_lockfile",
                "pnpm-lock.yaml",
                "pnpm-lock.yaml",
                "project-local",
            ),
            (
                "node_lockfile",
                "package-lock.json",
                "package-lock.json",
                "project-local",
            ),
            ("node_lockfile", "yarn.lock", "yarn.lock", "project-local"),
            (
                "node_workspace",
                "pnpm-workspace.yaml",
                "pnpm-workspace.yaml",
                "project-local",
            ),
            (
                "node_workspace",
                "turbo.json",
                "turbo.json",
                "project-local",
            ),
            (
                "node_runtime_config",
                "vite.config.ts",
                "vite.config.ts",
                "project-local",
            ),
            (
                "node_runtime_config",
                "vite.config.js",
                "vite.config.js",
                "project-local",
            ),
            (
                "node_runtime_config",
                "next.config.js",
                "next.config.js",
                "project-local",
            ),
            (
                "node_runtime_config",
                "next.config.mjs",
                "next.config.mjs",
                "project-local",
            ),
            (
                "node_runtime_config",
                "tsconfig.json",
                "tsconfig.json",
                "project-local",
            ),
        ]);
    }

    if has_python {
        candidates.extend([
            (
                "python_manifest",
                "pyproject.toml",
                "pyproject.toml",
                "project-local",
            ),
            (
                "python_manifest",
                "requirements.txt",
                "requirements.txt",
                "project-local",
            ),
            ("python_manifest", "Pipfile", "Pipfile", "project-local"),
            (
                "python_manifest",
                "poetry.lock",
                "poetry.lock",
                "project-local",
            ),
            (
                "python_runtime_config",
                "pytest.ini",
                "pytest.ini",
                "project-local",
            ),
            (
                "python_runtime_config",
                ".python-version",
                ".python-version",
                "project-local",
            ),
        ]);
    }

    if has_go {
        candidates.extend([
            ("go_manifest", "go.mod", "go.mod", "project-local"),
            ("go_manifest", "go.work", "go.work", "project-local"),
        ]);
    }

    if has_rust {
        candidates.extend([
            (
                "cargo_manifest",
                "Cargo.toml",
                "Cargo.toml",
                "project-local",
            ),
            (
                "cargo_manifest",
                "Cargo.lock",
                "Cargo.lock",
                "project-local",
            ),
            (
                "cargo_runtime_config",
                ".cargo/config.toml",
                ".cargo/config.toml",
                "project-local",
            ),
            (
                "cargo_runtime_config",
                ".cargo/config",
                ".cargo/config",
                "project-local",
            ),
            (
                "cargo_toolchain",
                "rust-toolchain.toml",
                "rust-toolchain.toml",
                "project-local",
            ),
            (
                "cargo_toolchain",
                "rust-toolchain",
                "rust-toolchain",
                "project-local",
            ),
        ]);
    }

    for (kind, label, relative_path, source) in candidates {
        let path = project_root.join(relative_path);
        if !path.is_file() {
            continue;
        }
        out.push(ProjectRunConfigFileSummary {
            kind: kind.to_string(),
            label: label.to_string(),
            path: normalize_path(path.as_path()),
            preview: read_preview_lines(path.as_path(), 3),
            source: source.to_string(),
        });
    }

    if has_java {
        if let Some(home) = home_dir() {
            let user_gradle_properties = Path::new(home.as_str()).join(".gradle/gradle.properties");
            if user_gradle_properties.is_file() {
                out.push(ProjectRunConfigFileSummary {
                    kind: "gradle_user_properties".to_string(),
                    label: "用户 Gradle Properties".to_string(),
                    path: normalize_path(user_gradle_properties.as_path()),
                    preview: read_preview_lines(user_gradle_properties.as_path(), 3),
                    source: "env".to_string(),
                });
            }
        }
    }

    out
}
