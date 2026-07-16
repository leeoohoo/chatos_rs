// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{json, Value};

#[derive(Debug, Serialize)]
pub(super) struct LocalProjectScanEvidence {
    files: Vec<String>,
    detected_stack: Value,
    required_services: Vec<String>,
    environment_variable_keys: Vec<String>,
    manifest_context: Vec<ManifestContext>,
}

#[derive(Debug, Serialize)]
struct ManifestContext {
    path: String,
    content: String,
}

pub(super) async fn scan_local_project(root: PathBuf) -> Result<LocalProjectScanEvidence, String> {
    tokio::task::spawn_blocking(move || scan(root.as_path()))
        .await
        .map_err(|error| error.to_string())?
}

fn scan(root: &Path) -> Result<LocalProjectScanEvidence, String> {
    let mut candidates = Vec::new();
    collect_files(root, root, 0, &mut candidates)?;
    let mut stack = serde_json::Map::new();
    let mut services = BTreeSet::new();
    let mut env_keys = BTreeSet::new();
    let mut contexts = Vec::new();
    for path in candidates.iter().take(40) {
        detect_stack(path, &mut stack);
        let content = std::fs::read_to_string(path).unwrap_or_default();
        detect_services(content.as_str(), &mut services);
        detect_env_keys(content.as_str(), &mut env_keys);
        if !content.is_empty() && contexts.len() < 24 {
            contexts.push(ManifestContext {
                path: relative(root, path),
                content: content.chars().take(16_000).collect(),
            });
        }
    }
    Ok(LocalProjectScanEvidence {
        files: candidates.iter().map(|path| relative(root, path)).collect(),
        detected_stack: Value::Object(stack),
        required_services: services.into_iter().collect(),
        environment_variable_keys: env_keys.into_iter().collect(),
        manifest_context: contexts,
    })
}

fn collect_files(
    root: &Path,
    dir: &Path,
    depth: usize,
    output: &mut Vec<PathBuf>,
) -> Result<(), String> {
    if depth > 5 || output.len() >= 400 {
        return Ok(());
    }
    let entries = std::fs::read_dir(dir).map_err(|error| error.to_string())?;
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if path.is_dir() {
            if !ignored_dir(name.as_str()) {
                collect_files(root, path.as_path(), depth + 1, output)?;
            }
        } else if relevant_file(name.as_str()) {
            output.push(path);
        }
        if output.len() >= 400 {
            break;
        }
    }
    Ok(())
}

fn ignored_dir(name: &str) -> bool {
    matches!(
        name,
        ".git" | "node_modules" | "target" | "dist" | "build" | ".next" | ".venv"
    )
}

fn relevant_file(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "package.json"
            | "cargo.toml"
            | "go.mod"
            | "pyproject.toml"
            | "requirements.txt"
            | "pom.xml"
            | "gemfile"
            | "composer.json"
            | ".env.example"
    ) || lower.starts_with("readme")
        || lower.starts_with("dockerfile")
        || lower.contains("docker-compose")
        || lower.starts_with("application")
        || lower.starts_with("bootstrap")
        || lower.ends_with(".csproj")
}

fn detect_stack(path: &Path, stack: &mut serde_json::Map<String, Value>) {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    for (marker, language) in [
        ("package.json", "nodejs"),
        ("cargo.toml", "rust"),
        ("go.mod", "go"),
        ("pyproject.toml", "python"),
        ("requirements.txt", "python"),
        ("pom.xml", "java"),
        (".csproj", "dotnet"),
        ("gemfile", "ruby"),
        ("composer.json", "php"),
    ] {
        if name == marker || name.ends_with(marker) {
            stack.insert(language.to_string(), json!(true));
        }
    }
}

fn detect_services(content: &str, services: &mut BTreeSet<String>) {
    let lower = content.to_ascii_lowercase();
    for service in [
        "postgres",
        "mysql",
        "redis",
        "mongodb",
        "nacos",
        "rabbitmq",
        "kafka",
        "elasticsearch",
        "minio",
    ] {
        if lower.contains(service) {
            services.insert(service.to_string());
        }
    }
}

fn detect_env_keys(content: &str, keys: &mut BTreeSet<String>) {
    for marker in ["process.env.", "std::env::var(\"", "os.getenv(\""] {
        for tail in content.split(marker).skip(1) {
            let key = tail
                .chars()
                .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
                .collect::<String>();
            if key.len() > 1 {
                keys.insert(key);
            }
        }
    }
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
}
