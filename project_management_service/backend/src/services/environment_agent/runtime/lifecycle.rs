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
    let application_index = images
        .iter()
        .position(runtime_image_is_application)
        .ok_or_else(|| "编排计划缺少应用运行时".to_string())?;
    let application_dockerfile = images[application_index]
        .dockerfile
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "编排计划缺少应用 Dockerfile".to_string())?
        .to_string();
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
        application_dockerfile.as_str(),
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
    for image in &mut images {
        image.status = "running".to_string();
        image.image_provider = RuntimeEnvironmentProvider::LocalConnector;
        image.image_ref = Some(if runtime_image_is_application(image) {
            format!("{project_name}-application")
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
    let identity =
        format!("{} {}", image.environment_key, image.environment_type).to_ascii_lowercase();
    identity.contains("application")
        || identity.contains("runtime")
        || matches!(image.environment_key.as_str(), "app" | "application")
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
