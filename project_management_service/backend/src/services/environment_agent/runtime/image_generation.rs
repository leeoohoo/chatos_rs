// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::super::*;

pub(in crate::services::environment_agent) async fn generate_project_runtime_environment_image_impl(
    state: &AppState,
    project: &ProjectRecord,
    user_access_token: Option<&str>,
    image_record_id: &str,
) -> Result<ProjectRuntimeEnvironmentResponse, String> {
    let mut environment = state
        .store
        .get_project_runtime_environment(project.id.as_str())
        .await?
        .ok_or_else(|| "项目运行环境尚未初始化".to_string())?;
    crate::services::runtime_environment::refresh_environment_variable_values(&mut environment);
    let mut images = state
        .store
        .list_project_runtime_environment_images(project.id.as_str())
        .await?;
    for image in &mut images {
        crate::services::runtime_environment::apply_program_managed_image_policy(image);
    }
    if enforce_project_runtime_boundary(
        project.execution_plane,
        &mut environment,
        images.as_mut_slice(),
    ) {
        state
            .store
            .upsert_project_runtime_environment(&environment)
            .await?;
        images = state
            .store
            .replace_project_runtime_environment_images(project.id.as_str(), images.as_slice())
            .await?;
    }
    let requested_index = images
        .iter()
        .position(|image| image.id == image_record_id.trim())
        .ok_or_else(|| format!("镜像计划不存在: {image_record_id}"))?;
    if images[requested_index].service_role != RuntimeServiceRole::Application
        || images[requested_index].mcp_policy.attachment
            != RuntimeMcpAttachment::ProjectGatewayTarget
    {
        return Err("只有程序确认的应用服务才能生成 MCP 执行镜像".to_string());
    }
    let run_id = format!("project_image_build_{}", uuid::Uuid::new_v4());
    let application_indexes = images
        .iter()
        .enumerate()
        .filter_map(|(index, image)| {
            (image.service_role == RuntimeServiceRole::Application
                && image.mcp_policy.attachment == RuntimeMcpAttachment::ProjectGatewayTarget)
                .then_some(index)
        })
        .collect::<Vec<_>>();
    for index in &application_indexes {
        if images[*index]
            .dockerfile
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
        {
            return Err(format!(
                "镜像计划缺少 Dockerfile，请重新分析项目环境: {}",
                images[*index].service_id
            ));
        }
        let features =
            program_managed_sandbox_features(&images[*index], &environment.detected_stack);
        images[*index].features = serde_json::json!(features);
        images[*index].status = "building".to_string();
        images[*index].error = None;
        images[*index].updated_at = now_rfc3339();
    }
    let dependency_indexes = images
        .iter()
        .enumerate()
        .filter_map(|(index, image)| {
            (image.service_role == RuntimeServiceRole::Dependency).then_some(index)
        })
        .collect::<Vec<_>>();
    let dependency_image_refs = dependency_indexes
        .iter()
        .filter_map(|index| images[*index].image_ref.clone())
        .collect::<Vec<_>>();
    for index in &dependency_indexes {
        images[*index].status = "preparing".to_string();
        images[*index].error = None;
        images[*index].updated_at = now_rfc3339();
    }
    state
        .store
        .replace_project_runtime_environment_images(project.id.as_str(), images.as_slice())
        .await?;

    let catalog = if application_indexes
        .iter()
        .any(|index| images[*index].image_id.is_some())
    {
        get_sandbox_image_catalog(
            state,
            project,
            environment.sandbox_provider,
            user_access_token,
            run_id.as_str(),
        )
        .await
        .ok()
    } else {
        None
    };
    let application_plans = application_indexes
        .iter()
        .map(|index| (*index, images[*index].clone()))
        .collect::<Vec<_>>();
    let application_future = async {
        let mut results = Vec::with_capacity(application_plans.len());
        for (position, (index, image)) in application_plans.into_iter().enumerate() {
            let application_run_id = format!("{run_id}_app_{position}");
            let result = prepare_application_image(
                state,
                project,
                environment.sandbox_provider,
                user_access_token,
                application_run_id.as_str(),
                &image,
                catalog.as_ref(),
            )
            .await;
            results.push((index, result));
        }
        results
    };
    let dependency_future = prepare_sandbox_dependency_images(
        state,
        environment.sandbox_provider,
        project.id.as_str(),
        run_id.as_str(),
        dependency_image_refs,
    );
    let (application_results, dependency_result) =
        tokio::join!(application_future, dependency_future);

    let mut errors = Vec::new();
    for (index, result) in application_results {
        match result {
            Ok(result) => {
                if let Err(error) = apply_prepared_application_result(
                    &mut images[index],
                    environment.sandbox_provider,
                    &result,
                ) {
                    images[index].status = "failed".to_string();
                    images[index].error = Some(error.clone());
                    errors.push(error);
                }
            }
            Err(error) => {
                images[index].status = "failed".to_string();
                images[index].error = Some(error.clone());
                images[index].updated_at = now_rfc3339();
                errors.push(error);
            }
        }
    }
    match dependency_result {
        Ok(_) => {
            for index in &dependency_indexes {
                images[*index].status = "ready".to_string();
                images[*index].error = None;
                images[*index].updated_at = now_rfc3339();
            }
        }
        Err(error) => {
            for index in &dependency_indexes {
                images[*index].status = "failed".to_string();
                images[*index].error = Some(error.clone());
                images[*index].updated_at = now_rfc3339();
            }
            errors.push(error);
        }
    }
    environment.status = if errors.is_empty() && images.iter().all(runtime_image_is_ready) {
        if crate::services::runtime_environment::required_environment_variables_are_complete(
            &environment.environment_variables,
        ) {
            ProjectRuntimeEnvironmentStatus::Ready
        } else {
            ProjectRuntimeEnvironmentStatus::PendingConfiguration
        }
    } else {
        ProjectRuntimeEnvironmentStatus::PendingImageBuild
    };
    environment.last_error = (!errors.is_empty()).then(|| errors.join("; "));
    environment.updated_at = now_rfc3339();
    let environment = state
        .store
        .upsert_project_runtime_environment(&environment)
        .await?;
    let images = state
        .store
        .replace_project_runtime_environment_images(project.id.as_str(), &images)
        .await?;
    if let Some(error) = environment.last_error.clone() {
        return Err(error);
    }
    Ok(ProjectRuntimeEnvironmentResponse {
        environment,
        images,
    })
}

async fn prepare_application_image(
    state: &AppState,
    project: &ProjectRecord,
    provider: RuntimeEnvironmentProvider,
    user_access_token: Option<&str>,
    run_id: &str,
    image: &ProjectRuntimeEnvironmentImageRecord,
    catalog: Option<&Value>,
) -> Result<Value, String> {
    if let Some(image_id) = image
        .image_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if let Some(reused) = reusable_catalog_image(catalog, image_id) {
            return Ok(reused);
        }
    }
    let features = image
        .features
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    create_sandbox_image_from_plan(
        state,
        project,
        provider,
        user_access_token,
        run_id,
        features,
        image.custom_build_script.clone(),
    )
    .await
}

fn reusable_catalog_image(catalog: Option<&Value>, image_id: &str) -> Option<Value> {
    let image = catalog?
        .get("images")?
        .as_array()?
        .iter()
        .find(|image| image.get("id").and_then(Value::as_str) == Some(image_id))?;
    let initialized = image
        .get("initialized")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let status = image
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if !initialized
        && !matches!(
            status.as_str(),
            "ready" | "available" | "local" | "succeeded" | "initialized"
        )
    {
        return None;
    }
    Some(serde_json::json!({
        "reused": true,
        "image_id": image.get("id").cloned().unwrap_or(Value::Null),
        "image_ref": image.get("image_ref").cloned().unwrap_or(Value::Null),
        "status": image.get("status").cloned().unwrap_or(Value::Null),
        "features": image.get("features").cloned().unwrap_or_else(|| serde_json::json!([])),
    }))
}

fn apply_prepared_application_result(
    image: &mut ProjectRuntimeEnvironmentImageRecord,
    provider: RuntimeEnvironmentProvider,
    result: &Value,
) -> Result<(), String> {
    let image_id = result
        .get("image_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "镜像准备成功响应缺少 image_id".to_string())?;
    let image_ref = result
        .get("image_ref")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "镜像准备成功响应缺少 image_ref".to_string())?;
    image.image_id = Some(image_id.to_string());
    image.image_ref = Some(image_ref.to_string());
    image.image_provider = provider;
    if let Some(features) = result.get("features").and_then(Value::as_array) {
        image.features = Value::Array(features.clone());
    }
    image.status = "ready".to_string();
    image.error = None;
    image.updated_at = now_rfc3339();
    Ok(())
}

fn runtime_image_is_ready(image: &ProjectRuntimeEnvironmentImageRecord) -> bool {
    image.image_provider != RuntimeEnvironmentProvider::None
        && image
            .image_id
            .as_deref()
            .or(image.image_ref.as_deref())
            .is_some_and(|value| !value.trim().is_empty())
        && matches!(
            image.status.trim().to_ascii_lowercase().as_str(),
            "ready" | "available" | "local" | "succeeded" | "completed" | "running"
        )
}

fn program_managed_sandbox_features(
    image: &ProjectRuntimeEnvironmentImageRecord,
    detected_stack: &Value,
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

    let evidence = format!(
        "{} {}",
        image.dockerfile.as_deref().unwrap_or_default(),
        serde_json::to_string(detected_stack).unwrap_or_default(),
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

#[cfg(test)]
mod tests {
    use super::{program_managed_sandbox_features, *};

    fn application(features: Value, dockerfile: &str) -> ProjectRuntimeEnvironmentImageRecord {
        ProjectRuntimeEnvironmentImageRecord {
            id: "image-1".to_string(),
            project_id: "project-1".to_string(),
            environment_key: "api".to_string(),
            environment_type: "application".to_string(),
            display_name: "API".to_string(),
            service_id: "api".to_string(),
            service_role: RuntimeServiceRole::Application,
            mcp_policy: ProgramManagedMcpPolicy::application_target(),
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

    #[test]
    fn build_tools_are_mapped_to_supported_program_managed_runtimes() {
        let image = application(
            serde_json::json!(["maven", "spring-boot", "unknown-build-tool"]),
            "FROM maven:3-eclipse-temurin-21 AS build\nFROM eclipse-temurin:21-jre\n",
        );
        assert_eq!(
            program_managed_sandbox_features(&image, &serde_json::json!({})),
            vec!["java"]
        );
    }

    #[test]
    fn dockerfile_and_stack_evidence_fill_missing_runtime_features() {
        let image = application(
            serde_json::json!(["base"]),
            "FROM node:24-bookworm AS build\n",
        );
        assert_eq!(
            program_managed_sandbox_features(&image, &serde_json::json!({"languages": ["Python"]}),),
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
            program_managed_sandbox_features(&image, &serde_json::json!({})),
            vec!["java@8", "node@22"]
        );
    }
}
