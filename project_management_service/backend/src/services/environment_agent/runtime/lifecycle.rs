// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::super::*;

pub(in crate::services::environment_agent) async fn start_project_runtime_environment_impl(
    state: &AppState,
    project: &ProjectRecord,
    user_access_token: Option<&str>,
) -> Result<ProjectRuntimeEnvironmentResponse, String> {
    let mut environment = state
        .store
        .get_project_runtime_environment(project.id.as_str())
        .await?
        .ok_or_else(|| "项目运行环境尚未初始化".to_string())?;
    crate::services::runtime_environment::refresh_environment_variable_values(&mut environment);
    if environment.sandbox_provider != RuntimeEnvironmentProvider::LocalConnector {
        return Err("项目级 Docker Compose 当前需要使用 Local Connector 本地沙箱".to_string());
    }
    let mut images = state
        .store
        .list_project_runtime_environment_images(project.id.as_str())
        .await?;
    for image in &mut images {
        crate::services::runtime_environment::apply_program_managed_image_policy(image);
    }
    let mut application_dockerfiles = std::collections::BTreeMap::new();
    for (index, image) in images
        .iter()
        .filter(|image| runtime_image_is_application(image))
        .enumerate()
    {
        let service_id = super::super::runtime_application_service_id(image, index);
        let dockerfile = image
            .dockerfile
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| format!("编排计划缺少应用 Dockerfile: {service_id}"))?;
        if application_dockerfiles
            .insert(service_id.clone(), dockerfile.to_string())
            .is_some()
        {
            return Err(format!("编排计划包含重复的应用服务标识: {service_id}"));
        }
    }
    if application_dockerfiles.is_empty() {
        return Err("编排计划缺少应用运行时".to_string());
    }
    let compose_yaml = environment
        .generated_config_files
        .iter()
        .find(|file| file.path == PROJECT_COMPOSE_FILE_PATH)
        .map(|file| file.content.as_str())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "编排计划缺少 docker-compose.chatos.yml，请重新分析项目环境".to_string())?
        .to_string();
    let project_name = runtime_compose_project_name(project.id.as_str());
    let env_file = runtime_environment_dotenv(&environment.environment_variables)?;
    for image in &mut images {
        image.status = "starting".to_string();
        image.error = None;
        image.updated_at = now_rfc3339();
    }
    state
        .store
        .replace_project_runtime_environment_images(project.id.as_str(), images.as_slice())
        .await?;

    let result = start_local_project_compose_environment(
        state,
        project,
        user_access_token,
        project_name.as_str(),
        compose_yaml.as_str(),
        &application_dockerfiles,
        env_file.as_str(),
    )
    .await;
    if let Err(error) = result {
        for image in &mut images {
            image.status = "failed".to_string();
            image.error = Some(error.clone());
            image.updated_at = now_rfc3339();
        }
        environment.status = ProjectRuntimeEnvironmentStatus::Failed;
        environment.last_error = Some(error.clone());
        environment.updated_at = now_rfc3339();
        state
            .store
            .upsert_project_runtime_environment(&environment)
            .await?;
        state
            .store
            .replace_project_runtime_environment_images(project.id.as_str(), images.as_slice())
            .await?;
        return Err(error);
    }
    let mut application_index = 0usize;
    for image in &mut images {
        image.status = "running".to_string();
        image.image_provider = RuntimeEnvironmentProvider::LocalConnector;
        image.image_ref = Some(if runtime_image_is_application(image) {
            let service_id = super::super::runtime_application_service_id(image, application_index);
            application_index += 1;
            format!("{project_name}-{service_id}")
        } else {
            compose_dependency_image_ref_impl(image)
                .unwrap_or_else(|| format!("compose://{project_name}/{}", image.environment_key))
        });
        image.error = None;
        image.updated_at = now_rfc3339();
    }
    environment.status =
        if crate::services::runtime_environment::required_environment_variables_are_complete(
            &environment.environment_variables,
        ) {
            ProjectRuntimeEnvironmentStatus::Ready
        } else {
            ProjectRuntimeEnvironmentStatus::PendingConfiguration
        };
    environment.last_error = None;
    environment.analysis_summary = Some(format!(
        "{} 项目级 Docker Compose 环境 `{project_name}` 已生成并启动，应用和依赖服务作为一个整体管理。",
        environment.analysis_summary.as_deref().unwrap_or("")
    ).trim().to_string());
    environment.updated_at = now_rfc3339();
    let environment = state
        .store
        .upsert_project_runtime_environment(&environment)
        .await?;
    let images = state
        .store
        .replace_project_runtime_environment_images(project.id.as_str(), images.as_slice())
        .await?;
    Ok(ProjectRuntimeEnvironmentResponse {
        environment,
        images,
    })
}

pub(in crate::services::environment_agent) async fn get_project_runtime_environment_deployment_impl(
    state: &AppState,
    project: &ProjectRecord,
    user_access_token: Option<&str>,
) -> Result<Value, String> {
    let environment = state
        .store
        .get_project_runtime_environment(project.id.as_str())
        .await?
        .ok_or_else(|| "项目运行环境尚未初始化".to_string())?;
    ensure_local_compose_provider(&environment)?;
    let project_name = runtime_compose_project_name(project.id.as_str());
    let deployment = get_local_project_compose_environment_status(
        state,
        project,
        user_access_token,
        project_name.as_str(),
    )
    .await?;
    let mut images = state
        .store
        .list_project_runtime_environment_images(project.id.as_str())
        .await?;
    for image in &mut images {
        crate::services::runtime_environment::apply_program_managed_image_policy(image);
    }
    let runtime_services = deployment
        .get("services")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let services = images
        .iter()
        .map(|image| {
            let runtime = compose_runtime_service(runtime_services.as_slice(), &image.service_id);
            serde_json::json!({
                "service_id": image.service_id,
                "environment_key": image.environment_key,
                "display_name": image.display_name,
                "service_role": image.service_role,
                "mcp_policy": image.mcp_policy,
                "status": runtime
                    .and_then(compose_runtime_service_status)
                    .unwrap_or_else(|| image.status.clone()),
                "image_ref": image.image_ref,
                "ports": image.ports,
                "runtime": runtime,
            })
        })
        .collect::<Vec<_>>();
    Ok(serde_json::json!({
        "project_id": project.id,
        "project_name": deployment
            .get("project_name")
            .and_then(Value::as_str)
            .unwrap_or(project_name.as_str()),
        "status": deployment
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("unknown"),
        "runtime_directory": deployment.get("runtime_directory"),
        "compose_file": deployment.get("compose_file"),
        "services": services,
    }))
}

pub(in crate::services::environment_agent) async fn stop_project_runtime_environment_impl(
    state: &AppState,
    project: &ProjectRecord,
    user_access_token: Option<&str>,
) -> Result<ProjectRuntimeEnvironmentResponse, String> {
    let mut environment = state
        .store
        .get_project_runtime_environment(project.id.as_str())
        .await?
        .ok_or_else(|| "项目运行环境尚未初始化".to_string())?;
    crate::services::runtime_environment::refresh_environment_variable_values(&mut environment);
    ensure_local_compose_provider(&environment)?;
    let project_name = runtime_compose_project_name(project.id.as_str());
    stop_local_project_compose_environment(
        state,
        project,
        user_access_token,
        project_name.as_str(),
    )
    .await?;

    let mut images = state
        .store
        .list_project_runtime_environment_images(project.id.as_str())
        .await?;
    for image in &mut images {
        crate::services::runtime_environment::apply_program_managed_image_policy(image);
        image.status = "stopped".to_string();
        image.error = None;
        image.updated_at = now_rfc3339();
    }
    environment.status = environment_ready_status(&environment);
    environment.last_error = None;
    environment.analysis_summary = Some(
        format!(
            "{} 项目级 Docker Compose 环境 `{project_name}` 已整体停止，受管数据卷保留。",
            environment.analysis_summary.as_deref().unwrap_or("")
        )
        .trim()
        .to_string(),
    );
    environment.updated_at = now_rfc3339();
    let environment = state
        .store
        .upsert_project_runtime_environment(&environment)
        .await?;
    let images = state
        .store
        .replace_project_runtime_environment_images(project.id.as_str(), images.as_slice())
        .await?;
    Ok(ProjectRuntimeEnvironmentResponse {
        environment,
        images,
    })
}

pub(in crate::services::environment_agent) async fn restart_project_runtime_environment_impl(
    state: &AppState,
    project: &ProjectRecord,
    user_access_token: Option<&str>,
) -> Result<ProjectRuntimeEnvironmentResponse, String> {
    let mut environment = state
        .store
        .get_project_runtime_environment(project.id.as_str())
        .await?
        .ok_or_else(|| "项目运行环境尚未初始化".to_string())?;
    crate::services::runtime_environment::refresh_environment_variable_values(&mut environment);
    ensure_local_compose_provider(&environment)?;
    let project_name = runtime_compose_project_name(project.id.as_str());
    let mut images = state
        .store
        .list_project_runtime_environment_images(project.id.as_str())
        .await?;
    for image in &mut images {
        crate::services::runtime_environment::apply_program_managed_image_policy(image);
        image.status = "starting".to_string();
        image.error = None;
        image.updated_at = now_rfc3339();
    }
    state
        .store
        .replace_project_runtime_environment_images(project.id.as_str(), images.as_slice())
        .await?;

    if let Err(error) = restart_local_project_compose_environment(
        state,
        project,
        user_access_token,
        project_name.as_str(),
    )
    .await
    {
        for image in &mut images {
            image.status = "failed".to_string();
            image.error = Some(error.clone());
            image.updated_at = now_rfc3339();
        }
        environment.status = ProjectRuntimeEnvironmentStatus::Failed;
        environment.last_error = Some(error.clone());
        environment.updated_at = now_rfc3339();
        state
            .store
            .upsert_project_runtime_environment(&environment)
            .await?;
        state
            .store
            .replace_project_runtime_environment_images(project.id.as_str(), images.as_slice())
            .await?;
        return Err(error);
    }

    mark_compose_images_running(images.as_mut_slice(), project_name.as_str());
    environment.status = environment_ready_status(&environment);
    environment.last_error = None;
    environment.analysis_summary = Some(
        format!(
            "{} 项目级 Docker Compose 环境 `{project_name}` 已整体重启。",
            environment.analysis_summary.as_deref().unwrap_or("")
        )
        .trim()
        .to_string(),
    );
    environment.updated_at = now_rfc3339();
    let environment = state
        .store
        .upsert_project_runtime_environment(&environment)
        .await?;
    let images = state
        .store
        .replace_project_runtime_environment_images(project.id.as_str(), images.as_slice())
        .await?;
    Ok(ProjectRuntimeEnvironmentResponse {
        environment,
        images,
    })
}

fn ensure_local_compose_provider(
    environment: &ProjectRuntimeEnvironmentRecord,
) -> Result<(), String> {
    if environment.sandbox_provider == RuntimeEnvironmentProvider::LocalConnector {
        Ok(())
    } else {
        Err("项目级 Docker Compose 当前需要使用 Local Connector 本地沙箱".to_string())
    }
}

fn environment_ready_status(
    environment: &ProjectRuntimeEnvironmentRecord,
) -> ProjectRuntimeEnvironmentStatus {
    if crate::services::runtime_environment::required_environment_variables_are_complete(
        &environment.environment_variables,
    ) {
        ProjectRuntimeEnvironmentStatus::Ready
    } else {
        ProjectRuntimeEnvironmentStatus::PendingConfiguration
    }
}

fn mark_compose_images_running(
    images: &mut [ProjectRuntimeEnvironmentImageRecord],
    project_name: &str,
) {
    for image in images {
        image.status = "running".to_string();
        image.image_provider = RuntimeEnvironmentProvider::LocalConnector;
        image.image_ref = Some(if runtime_image_is_application(image) {
            format!("{project_name}-{}", image.service_id)
        } else {
            compose_dependency_image_ref_impl(image)
                .unwrap_or_else(|| format!("compose://{project_name}/{}", image.service_id))
        });
        image.error = None;
        image.updated_at = now_rfc3339();
    }
}

fn compose_runtime_service<'a>(services: &'a [Value], service_id: &str) -> Option<&'a Value> {
    services.iter().find(|service| {
        ["Service", "service"]
            .iter()
            .filter_map(|key| service.get(*key).and_then(Value::as_str))
            .any(|value| value == service_id)
    })
}

fn compose_runtime_service_status(service: &Value) -> Option<String> {
    let state = service
        .get("State")
        .or_else(|| service.get("state"))
        .and_then(Value::as_str)?
        .trim()
        .to_ascii_lowercase();
    let health = service
        .get("Health")
        .or_else(|| service.get("health"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase);
    Some(match health {
        Some(health) if !health.eq_ignore_ascii_case("healthy") => {
            format!("{state}:{health}")
        }
        _ => state,
    })
}

fn runtime_compose_project_name(project_id: &str) -> String {
    let suffix = project_id
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .take(12)
        .collect::<String>()
        .to_ascii_lowercase();
    format!(
        "chatos-{}",
        if suffix.is_empty() {
            "project"
        } else {
            suffix.as_str()
        }
    )
}

fn runtime_environment_dotenv(
    variables: &[ProjectRuntimeEnvironmentVariableRecord],
) -> Result<String, String> {
    let mut output = String::new();
    for variable in variables {
        let Some(value) = variable.effective_value.as_deref() else {
            continue;
        };
        if value.contains('\0') {
            return Err(format!("环境变量 {} 包含非法字符", variable.name));
        }
        let encoded = serde_json::to_string(value)
            .map_err(|err| format!("编码环境变量 {} 失败: {err}", variable.name))?;
        output.push_str(variable.name.as_str());
        output.push('=');
        output.push_str(encoded.as_str());
        output.push('\n');
    }
    Ok(output)
}

fn runtime_image_is_application(image: &ProjectRuntimeEnvironmentImageRecord) -> bool {
    image.service_role == RuntimeServiceRole::Application
}

pub(in crate::services::environment_agent) fn compose_dependency_image_ref_impl(
    image: &ProjectRuntimeEnvironmentImageRecord,
) -> Option<String> {
    let identity = format!(
        "{} {} {}",
        image.environment_key, image.environment_type, image.display_name
    )
    .to_ascii_lowercase();
    [
        ("mysql", "mysql:8.4"),
        ("mongo", "mongo:7.0"),
        ("postgres", "postgres:16-alpine"),
        ("redis", "redis:7-alpine"),
        ("nacos", "nacos/nacos-server:v2.4.3"),
        ("rabbitmq", "rabbitmq:3.13-management-alpine"),
        ("kafka", "bitnami/kafka:3.7"),
        (
            "elasticsearch",
            "docker.elastic.co/elasticsearch/elasticsearch:8.14.3",
        ),
        ("minio", "minio/minio:latest"),
    ]
    .into_iter()
    .find_map(|(marker, image_ref)| identity.contains(marker).then(|| image_ref.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deployment_topology_matches_compose_children_by_program_service_id() {
        let services = vec![
            serde_json::json!({"Service": "services-api", "State": "running", "Health": "healthy"}),
            serde_json::json!({"Service": "redis", "State": "running", "Health": "starting"}),
        ];
        let api = compose_runtime_service(services.as_slice(), "services-api")
            .expect("application child");
        let redis =
            compose_runtime_service(services.as_slice(), "redis").expect("dependency child");
        assert_eq!(
            compose_runtime_service_status(api).as_deref(),
            Some("running")
        );
        assert_eq!(
            compose_runtime_service_status(redis).as_deref(),
            Some("running:starting")
        );
        assert!(compose_runtime_service(services.as_slice(), "mysql").is_none());
    }
}
