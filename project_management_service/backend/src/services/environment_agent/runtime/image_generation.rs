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
    if environment.status == ProjectRuntimeEnvironmentStatus::NotRunnable
        || environment
            .not_runnable_reason
            .as_deref()
            .map(str::trim)
            .is_some_and(|reason| !reason.is_empty())
    {
        return Err(environment
            .not_runnable_reason
            .clone()
            .unwrap_or_else(|| "当前项目没有可生成的运行环境".to_string()));
    }
    crate::services::runtime_environment::refresh_environment_variable_values(&mut environment);
    let mut images = state
        .store
        .list_project_runtime_environment_images(project.id.as_str())
        .await?;
    for image in &mut images {
        crate::services::runtime_environment::apply_program_managed_image_policy(image);
    }
    if enforce_project_runtime_boundary(project, &mut environment, &mut images) {
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
    if images[requested_index].service_role != RuntimeServiceRole::Workspace
        || images[requested_index].mcp_policy.attachment
            != RuntimeMcpAttachment::WorkspaceGatewayTarget
    {
        return Err("只有程序生成的工作区计划才能生成执行镜像".to_string());
    }
    let workspace_indexes = images
        .iter()
        .enumerate()
        .filter_map(|(index, image)| {
            (image.service_role == RuntimeServiceRole::Workspace
                && image.mcp_policy.attachment == RuntimeMcpAttachment::WorkspaceGatewayTarget)
                .then_some(index)
        })
        .collect::<Vec<_>>();
    if workspace_indexes.len() != 1 {
        return Err("项目必须且只能包含一个工作区执行镜像计划".to_string());
    }
    let dependency_indexes = images
        .iter()
        .enumerate()
        .filter_map(|(index, image)| {
            (image.service_role == RuntimeServiceRole::Dependency).then_some(index)
        })
        .collect::<Vec<_>>();
    let required_indexes = workspace_indexes
        .iter()
        .chain(dependency_indexes.iter())
        .copied()
        .collect::<Vec<_>>();
    if required_indexes
        .iter()
        .all(|index| runtime_image_is_ready(&images[*index]))
    {
        environment.status =
            if crate::services::runtime_environment::required_environment_variables_are_complete(
                &environment.environment_variables,
            ) {
                ProjectRuntimeEnvironmentStatus::Ready
            } else {
                ProjectRuntimeEnvironmentStatus::PendingConfiguration
            };
        environment.last_error = None;
        environment.analysis_summary = Some(
            crate::services::runtime_environment::program_generated_runtime_analysis_summary(
                &environment,
                images.as_slice(),
            ),
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
        return Ok(ProjectRuntimeEnvironmentResponse {
            environment,
            images,
        });
    }
    if active_project_image_build(&environment, images.as_slice())
        && state
            .runtime_environment_image_jobs
            .lock()
            .await
            .contains(project.id.as_str())
    {
        return Ok(ProjectRuntimeEnvironmentResponse {
            environment,
            images,
        });
    }
    {
        let mut active = state.runtime_environment_image_jobs.lock().await;
        if !active.insert(project.id.clone()) {
            return Ok(ProjectRuntimeEnvironmentResponse {
                environment,
                images,
            });
        }
    }
    let run_id = format!("project_image_build_{}", uuid::Uuid::new_v4());
    let features = crate::services::runtime_environment::workspace_runtime_features(
        images.as_slice(),
        &environment.detected_stack,
    );
    let workspace_build_indexes = workspace_indexes
        .iter()
        .copied()
        .filter(|index| !runtime_image_is_ready(&images[*index]))
        .collect::<Vec<_>>();
    for index in &workspace_build_indexes {
        images[*index].features = serde_json::json!(features.clone());
        images[*index].status = "building".to_string();
        images[*index].error = None;
        images[*index].updated_at = now_rfc3339();
    }
    let dependency_prepare_indexes = dependency_indexes
        .iter()
        .copied()
        .filter(|index| !runtime_image_is_ready(&images[*index]))
        .collect::<Vec<_>>();
    let dependency_image_refs = dependency_prepare_indexes
        .iter()
        .filter_map(|index| images[*index].image_ref.clone())
        .collect::<Vec<_>>();
    for index in &dependency_prepare_indexes {
        images[*index].status = "preparing".to_string();
        images[*index].error = None;
        images[*index].updated_at = now_rfc3339();
    }
    environment.status = ProjectRuntimeEnvironmentStatus::PendingImageBuild;
    environment.last_agent_run_id = Some(run_id.clone());
    environment.last_error = None;
    environment.updated_at = now_rfc3339();
    if let Err(error) = state
        .store
        .upsert_project_runtime_environment(&environment)
        .await
    {
        state
            .runtime_environment_image_jobs
            .lock()
            .await
            .remove(project.id.as_str());
        return Err(error);
    }
    if let Err(error) = state
        .store
        .replace_project_runtime_environment_images(project.id.as_str(), images.as_slice())
        .await
    {
        state
            .runtime_environment_image_jobs
            .lock()
            .await
            .remove(project.id.as_str());
        return Err(error);
    }

    let response = ProjectRuntimeEnvironmentResponse {
        environment: environment.clone(),
        images: images.clone(),
    };
    let worker_state = state.clone();
    let worker_project = project.clone();
    let worker_project_id = project.id.clone();
    let worker_access_token = user_access_token.map(ToOwned::to_owned);
    let worker_run_id = run_id.clone();
    let worker_provider = environment.sandbox_provider;
    tokio::spawn(async move {
        if let Err(error) = finish_project_runtime_environment_image_build(
            worker_state.clone(),
            worker_project,
            worker_access_token,
            worker_run_id.clone(),
            worker_provider,
            images,
            workspace_build_indexes,
            dependency_prepare_indexes,
            dependency_image_refs,
        )
        .await
        {
            persist_background_image_build_failure(
                &worker_state,
                worker_project_id.as_str(),
                worker_run_id.as_str(),
                error.as_str(),
            )
            .await;
        }
        worker_state
            .runtime_environment_image_jobs
            .lock()
            .await
            .remove(worker_project_id.as_str());
    });

    Ok(response)
}

async fn finish_project_runtime_environment_image_build(
    state: AppState,
    project: ProjectRecord,
    user_access_token: Option<String>,
    run_id: String,
    provider: RuntimeEnvironmentProvider,
    mut images: Vec<ProjectRuntimeEnvironmentImageRecord>,
    workspace_indexes: Vec<usize>,
    dependency_indexes: Vec<usize>,
    dependency_image_refs: Vec<String>,
) -> Result<(), String> {
    let user_access_token = user_access_token.as_deref();
    let catalog = if workspace_indexes
        .iter()
        .any(|index| images[*index].image_id.is_some())
    {
        get_sandbox_image_catalog(
            &state,
            &project,
            provider,
            user_access_token,
            run_id.as_str(),
        )
        .await
        .ok()
    } else {
        None
    };
    let workspace_plans = workspace_indexes
        .iter()
        .map(|index| (*index, images[*index].clone()))
        .collect::<Vec<_>>();
    let workspace_future = async {
        let mut results = Vec::with_capacity(workspace_plans.len());
        for (position, (index, image)) in workspace_plans.into_iter().enumerate() {
            let workspace_run_id = format!("{run_id}_workspace_{position}");
            let result = prepare_application_image(
                &state,
                &project,
                provider,
                user_access_token,
                workspace_run_id.as_str(),
                &image,
                catalog.as_ref(),
            )
            .await;
            results.push((index, result));
        }
        results
    };
    let dependency_future = prepare_sandbox_dependency_images(
        &state,
        provider,
        project.id.as_str(),
        run_id.as_str(),
        dependency_image_refs,
    );
    let (workspace_results, dependency_result) = tokio::join!(workspace_future, dependency_future);

    let mut errors = Vec::new();
    for (index, result) in workspace_results {
        match result {
            Ok(result) => {
                if let Err(error) =
                    apply_prepared_application_result(&mut images[index], provider, &result)
                {
                    images[index].status = "failed".to_string();
                    images[index].error = Some(error.clone());
                    images[index].updated_at = now_rfc3339();
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
        Ok(result) => {
            apply_prepared_dependency_results(
                &mut images,
                dependency_indexes.as_slice(),
                &result,
                &mut errors,
            );
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
    let Some(mut environment) = state
        .store
        .get_project_runtime_environment(project.id.as_str())
        .await?
    else {
        return Err("项目运行环境不存在，无法保存镜像生成结果".to_string());
    };
    if environment.last_agent_run_id.as_deref() != Some(run_id.as_str())
        || environment.status != ProjectRuntimeEnvironmentStatus::PendingImageBuild
    {
        return Ok(());
    }
    crate::services::runtime_environment::refresh_environment_variable_values(&mut environment);
    if environment
        .not_runnable_reason
        .as_deref()
        .map(str::trim)
        .is_some_and(|reason| !reason.is_empty())
    {
        environment.status = ProjectRuntimeEnvironmentStatus::NotRunnable;
        environment.analysis_summary = environment.not_runnable_reason.clone();
        environment.execution_service_id = None;
        environment.last_error = None;
        environment.updated_at = now_rfc3339();
        state
            .store
            .upsert_project_runtime_environment(&environment)
            .await?;
        state
            .store
            .replace_project_runtime_environment_images(project.id.as_str(), &[])
            .await?;
        return Ok(());
    }
    environment.status = if errors.is_empty()
        && images
            .iter()
            .filter(|image| {
                crate::services::runtime_environment::runtime_image_is_execution_required(image)
            })
            .all(runtime_image_is_ready)
    {
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
    environment.analysis_summary = Some(
        crate::services::runtime_environment::program_generated_runtime_analysis_summary(
            &environment,
            images.as_slice(),
        ),
    );
    environment.updated_at = now_rfc3339();
    state
        .store
        .upsert_project_runtime_environment(&environment)
        .await?;
    state
        .store
        .replace_project_runtime_environment_images(project.id.as_str(), &images)
        .await?;
    Ok(())
}

fn active_project_image_build(
    environment: &ProjectRuntimeEnvironmentRecord,
    images: &[ProjectRuntimeEnvironmentImageRecord],
) -> bool {
    environment.status == ProjectRuntimeEnvironmentStatus::PendingImageBuild
        && environment
            .last_agent_run_id
            .as_deref()
            .is_some_and(|run_id| run_id.starts_with("project_image_build_"))
        && images.iter().any(|image| {
            matches!(
                image.status.trim().to_ascii_lowercase().as_str(),
                "building" | "preparing" | "running"
            )
        })
}

async fn persist_background_image_build_failure(
    state: &AppState,
    project_id: &str,
    run_id: &str,
    error: &str,
) {
    let Ok(Some(mut environment)) = state
        .store
        .get_project_runtime_environment(project_id)
        .await
    else {
        tracing::error!(project_id, run_id, error, "load failed project image build");
        return;
    };
    if environment.last_agent_run_id.as_deref() != Some(run_id)
        || environment.status != ProjectRuntimeEnvironmentStatus::PendingImageBuild
    {
        return;
    }
    environment.status = ProjectRuntimeEnvironmentStatus::PendingImageBuild;
    environment.analysis_summary = Some("沙箱镜像后台准备失败。".to_string());
    environment.last_error = Some(error.to_string());
    environment.updated_at = now_rfc3339();
    if let Err(persist_error) = state
        .store
        .upsert_project_runtime_environment(&environment)
        .await
    {
        tracing::error!(
            project_id,
            run_id,
            error = persist_error.as_str(),
            "persist failed project image build"
        );
    }
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
    let dependency_feature = result
        .get("features")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .any(|feature| {
            feature
                .trim()
                .to_ascii_lowercase()
                .starts_with("dependency@")
        });
    if image_id.to_ascii_lowercase().starts_with("dependency:") || dependency_feature {
        return Err(format!(
            "工作区执行镜像不能使用依赖服务镜像: {image_id} ({image_ref})"
        ));
    }
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

fn apply_prepared_dependency_results(
    images: &mut [ProjectRuntimeEnvironmentImageRecord],
    dependency_indexes: &[usize],
    result: &Value,
    errors: &mut Vec<String>,
) {
    let prepared = result
        .get("images")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|record| {
            let image_ref = record
                .get("image_ref")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())?;
            Some((image_ref.to_string(), record))
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    for index in dependency_indexes {
        let image = &mut images[*index];
        let image_ref = image
            .image_ref
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let Some(image_ref) = image_ref else {
            let error = format!("依赖镜像 {} 缺少 image_ref", image.display_name);
            image.status = "failed".to_string();
            image.error = Some(error.clone());
            image.updated_at = now_rfc3339();
            errors.push(error);
            continue;
        };
        let Some(record) = prepared.get(image_ref.as_str()) else {
            let error = format!("依赖镜像 {image_ref} 没有返回准备结果");
            image.status = "failed".to_string();
            image.error = Some(error.clone());
            image.updated_at = now_rfc3339();
            errors.push(error);
            continue;
        };
        let status = record
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        if matches!(
            status.as_str(),
            "ready" | "mock" | "deferred_to_local_compose"
        ) {
            image.status = "ready".to_string();
            image.error = None;
            image.updated_at = now_rfc3339();
            continue;
        }
        let error = record
            .get("error")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| format!("依赖镜像 {image_ref} 准备失败: {status}"));
        image.status = "failed".to_string();
        image.error = Some(error.clone());
        image.updated_at = now_rfc3339();
        errors.push(error);
    }
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

#[cfg(test)]
mod tests;
