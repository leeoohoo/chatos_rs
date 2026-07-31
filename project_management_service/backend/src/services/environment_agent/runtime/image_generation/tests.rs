// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

fn program_managed_sandbox_features(
    image: &ProjectRuntimeEnvironmentImageRecord,
    detected_stack: &Value,
    include_project_stack_evidence: bool,
) -> Vec<String> {
    const ORDERED_RUNTIMES: [&str; 10] = [
        "java", "node", "python", "rust", "go", "dotnet", "php", "ruby", "gcc", "clang",
    ];
    let mut selected = std::collections::BTreeMap::new();
    for raw in image
        .features
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        if let Some((runtime, feature)) = canonical_sandbox_runtime(raw) {
            let entry = selected.entry(runtime).or_insert_with(|| feature.clone());
            if !entry.contains('@') && feature.contains('@') {
                *entry = feature;
            }
        }
    }

    let project_stack_evidence = if include_project_stack_evidence {
        serde_json::to_string(detected_stack).unwrap_or_default()
    } else {
        String::new()
    };
    let evidence = format!(
        "{} {}",
        image.dockerfile.as_deref().unwrap_or_default(),
        project_stack_evidence,
    )
    .to_ascii_lowercase();
    for (runtime, markers) in [
        (
            "java",
            &[
                "from maven",
                "temurin",
                "openjdk",
                "pom.xml",
                "spring",
                "gradle",
                "\"java\"",
            ][..],
        ),
        (
            "node",
            &[
                "from node",
                "from oven/bun",
                "package.json",
                "nodejs",
                "typescript",
                "\"node\"",
            ][..],
        ),
        (
            "python",
            &[
                "from python",
                "requirements.txt",
                "pyproject.toml",
                "python3",
                "\"python\"",
            ][..],
        ),
        (
            "rust",
            &["from rust", "cargo.toml", "cargo build", "\"rust\""][..],
        ),
        ("go", &["from golang", "go.mod", "go build", "\"go\""][..]),
        (
            "dotnet",
            &[
                "from mcr.microsoft.com/dotnet",
                ".csproj",
                "dotnet ",
                "\"dotnet\"",
            ][..],
        ),
        ("php", &["from php", "composer.json", "\"php\""][..]),
        (
            "ruby",
            &["from ruby", "gemfile", "bundle install", "\"ruby\""][..],
        ),
        ("gcc", &["from gcc", "g++", "cmakelists.txt"][..]),
        ("clang", &["from clang", "from llvm", "clang++"][..]),
    ] {
        if markers.iter().any(|marker| evidence.contains(marker)) {
            selected
                .entry(runtime)
                .or_insert_with(|| runtime.to_string());
        }
    }

    ORDERED_RUNTIMES
        .into_iter()
        .filter_map(|runtime| selected.get(runtime).cloned())
        .collect()
}

fn canonical_sandbox_runtime(value: &str) -> Option<(&'static str, String)> {
    let value = value.trim().to_ascii_lowercase();
    if value.is_empty() {
        return None;
    }
    if let Some((name, version)) = value.split_once('@').or_else(|| value.split_once(':')) {
        let runtime = sandbox_runtime_for_name(name.trim())?;
        let version = version.trim().trim_start_matches('v');
        return Some((
            runtime,
            if version.is_empty() {
                runtime.to_string()
            } else {
                format!("{runtime}@{version}")
            },
        ));
    }
    if let Some(runtime) = sandbox_runtime_for_name(value.as_str()) {
        return Some((runtime, runtime.to_string()));
    }
    for name in [
        "javascript",
        "typescript",
        "openjdk",
        "nodejs",
        "python",
        "dotnet",
        "golang",
        "clang",
        "java",
        "rust",
        "ruby",
        "node",
        "gcc",
        "jdk",
        "php",
        "go",
    ] {
        let Some(version) = value.strip_prefix(name) else {
            continue;
        };
        let version = version
            .trim_matches(['-', '_', '@', ':'])
            .trim_start_matches('v');
        if version.is_empty()
            || !(version.chars().any(|character| character.is_ascii_digit())
                || matches!(version, "stable" | "beta" | "nightly"))
        {
            continue;
        }
        let runtime = sandbox_runtime_for_name(name)?;
        return Some((runtime, format!("{runtime}@{version}")));
    }
    None
}

fn sandbox_runtime_for_name(value: &str) -> Option<&'static str> {
    match value {
        "java" | "jdk" | "openjdk" | "maven" | "mvn" | "gradle" | "spring" | "springboot"
        | "spring-boot" => Some("java"),
        "node" | "nodejs" | "js" | "javascript" | "typescript" | "npm" | "pnpm" | "yarn"
        | "bun" => Some("node"),
        "python" | "python3" | "py" | "pip" | "pip3" | "poetry" | "uv" => Some("python"),
        "rust" | "cargo" => Some("rust"),
        "go" | "golang" | "gomod" => Some("go"),
        "dotnet" | "csharp" | "cs" | "fsharp" | "msbuild" => Some("dotnet"),
        "php" | "composer" => Some("php"),
        "ruby" | "rails" | "gem" | "bundler" => Some("ruby"),
        "gcc" | "c" | "cpp" | "c++" | "cplusplus" | "g++" => Some("gcc"),
        "clang" | "llvm" => Some("clang"),
        _ => None,
    }
}

use super::*;

fn application(features: Value, dockerfile: &str) -> ProjectRuntimeEnvironmentImageRecord {
    ProjectRuntimeEnvironmentImageRecord {
        id: "image-1".to_string(),
        project_id: "project-1".to_string(),
        environment_key: "api".to_string(),
        environment_type: "application".to_string(),
        display_name: "API".to_string(),
        service_id: "api".to_string(),
        service_role: RuntimeServiceRole::Application,
        source_root: ".".to_string(),
        component_kind: "application".to_string(),
        startup_command: None,
        test_command: None,
        depends_on: Vec::new(),
        auto_start: false,
        mcp_policy: ProgramManagedMcpPolicy::default(),
        image_id: None,
        image_ref: None,
        image_provider: RuntimeEnvironmentProvider::CloudSandboxManager,
        features,
        ports: empty_array(),
        env_vars: empty_object(),
        dockerfile: Some(dockerfile.to_string()),
        custom_build_script: None,
        status: "planned".to_string(),
        error: None,
        created_at: "now".to_string(),
        updated_at: "now".to_string(),
    }
}

fn dependency(id: &str, image_ref: &str) -> ProjectRuntimeEnvironmentImageRecord {
    ProjectRuntimeEnvironmentImageRecord {
        id: id.to_string(),
        project_id: "project-1".to_string(),
        environment_key: id.to_string(),
        environment_type: "dependency".to_string(),
        display_name: id.to_string(),
        service_id: id.to_string(),
        service_role: RuntimeServiceRole::Dependency,
        source_root: ".".to_string(),
        component_kind: "dependency".to_string(),
        startup_command: None,
        test_command: None,
        depends_on: Vec::new(),
        auto_start: true,
        mcp_policy: ProgramManagedMcpPolicy::default(),
        image_id: None,
        image_ref: Some(image_ref.to_string()),
        image_provider: RuntimeEnvironmentProvider::CloudSandboxManager,
        features: empty_array(),
        ports: empty_array(),
        env_vars: empty_object(),
        dockerfile: None,
        custom_build_script: None,
        status: "preparing".to_string(),
        error: None,
        created_at: "now".to_string(),
        updated_at: "now".to_string(),
    }
}

#[test]
fn build_tools_are_mapped_to_supported_program_managed_runtimes() {
    let image = application(
        serde_json::json!(["maven", "spring-boot", "unknown-build-tool"]),
        "FROM maven:3-eclipse-temurin-21 AS build\nFROM eclipse-temurin:21-jre\n",
    );
    assert_eq!(
        program_managed_sandbox_features(&image, &serde_json::json!({}), false),
        vec!["java"]
    );
}

#[test]
fn project_stack_evidence_is_not_shared_across_independent_applications() {
    let image = application(
        serde_json::json!(["base"]),
        "FROM nginx:1.27-alpine\nCOPY . /usr/share/nginx/html\n",
    );
    assert_eq!(
        program_managed_sandbox_features(
            &image,
            &serde_json::json!({"languages": ["Python"]}),
            false,
        ),
        Vec::<String>::new()
    );
}

#[test]
fn single_application_can_use_project_stack_evidence() {
    let image = application(
        serde_json::json!(["base"]),
        "FROM nginx:1.27-alpine\nCOPY . /usr/share/nginx/html\n",
    );
    assert_eq!(
        program_managed_sandbox_features(
            &image,
            &serde_json::json!({"languages": ["Python"]}),
            true,
        ),
        vec!["python"]
    );
}

#[test]
fn dockerfile_and_stack_evidence_fill_missing_runtime_features() {
    let image = application(
        serde_json::json!(["base"]),
        "FROM node:24-bookworm AS build\n",
    );
    assert_eq!(
        program_managed_sandbox_features(
            &image,
            &serde_json::json!({"languages": ["Python"]}),
            true,
        ),
        vec!["node", "python"]
    );
}

#[test]
fn explicit_runtime_versions_are_preserved_for_program_initialization() {
    let image = application(
        serde_json::json!(["java8", "node@22"]),
        "FROM eclipse-temurin:8-jre\n",
    );
    assert_eq!(
        program_managed_sandbox_features(&image, &serde_json::json!({}), false),
        vec!["java@8", "node@22"]
    );
}

#[test]
fn dependency_prepare_results_are_applied_per_image_ref() {
    let mut images = vec![
        dependency("postgres", "postgres:16-alpine"),
        dependency("redis", "redis:7-alpine"),
    ];
    let mut errors = Vec::new();

    apply_prepared_dependency_results(
        images.as_mut_slice(),
        &[0, 1],
        &serde_json::json!({
            "images": [
                { "image_ref": "postgres:16-alpine", "status": "failed", "error": "pull timed out" },
                { "image_ref": "redis:7-alpine", "status": "ready" }
            ]
        }),
        &mut errors,
    );

    assert_eq!(images[0].status, "failed");
    assert_eq!(images[0].error.as_deref(), Some("pull timed out"));
    assert_eq!(images[1].status, "ready");
    assert!(images[1].error.is_none());
    assert_eq!(errors, vec!["pull timed out".to_string()]);
}
