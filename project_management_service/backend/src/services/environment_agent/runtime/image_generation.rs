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
    let index = images
        .iter()
        .position(|image| image.id == image_record_id.trim())
        .ok_or_else(|| format!("镜像计划不存在: {image_record_id}"))?;
    if images[index]
        .dockerfile
        .as_deref()
        .is_none_or(|value| value.trim().is_empty())
    {
        return Err("镜像计划缺少 Dockerfile，请重新分析项目环境".to_string());
    }
    let features = images[index]
        .features
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    let custom_build_script = images[index].custom_build_script.clone();
    if features.is_empty()
        && custom_build_script
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
    {
        return Err("镜像计划缺少可执行的 features 或 custom_build_script".to_string());
    }

    let run_id = format!("project_image_build_{}", uuid::Uuid::new_v4());
    images[index].status = "building".to_string();
    images[index].error = None;
    images[index].updated_at = now_rfc3339();
    state
        .store
        .replace_project_runtime_environment_images(project.id.as_str(), images.as_slice())
        .await?;

    let result = create_sandbox_image_from_plan(
        state,
        project,
        environment.sandbox_provider,
        user_access_token,
        run_id.as_str(),
        features,
        custom_build_script,
    )
    .await;
    match result {
        Ok(result) => {
            let image_id = result
                .get("image_id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);
            let image_ref = result
                .get("image_ref")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);
            if image_id.is_none() && image_ref.is_none() {
                return persist_image_build_failure(
                    state,
                    project,
                    environment,
                    images,
                    index,
                    "镜像构建成功响应缺少 image_id/image_ref".to_string(),
                )
                .await;
            }
            images[index].image_id = image_id;
            images[index].image_ref = image_ref;
            images[index].image_provider = environment.sandbox_provider;
            images[index].status = "ready".to_string();
            images[index].error = None;
            images[index].updated_at = now_rfc3339();
            environment.status = if images.iter().all(runtime_image_is_ready) {
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
            environment.last_error = None;
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
        Err(error) => {
            persist_image_build_failure(state, project, environment, images, index, error).await
        }
    }
}

async fn persist_image_build_failure(
    state: &AppState,
    project: &ProjectRecord,
    mut environment: ProjectRuntimeEnvironmentRecord,
    mut images: Vec<ProjectRuntimeEnvironmentImageRecord>,
    index: usize,
    error: String,
) -> Result<ProjectRuntimeEnvironmentResponse, String> {
    images[index].status = "failed".to_string();
    images[index].error = Some(error.clone());
    images[index].updated_at = now_rfc3339();
    environment.status = ProjectRuntimeEnvironmentStatus::PendingImageBuild;
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
    Err(error)
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
