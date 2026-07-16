// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::*;
use crate::state::AppState;
use crate::user_model_runtime_client::resolve_default_environment_initialization_model_runtime;
use chatos_agent::{AgentExecutor, AgentTurnMemory, AgentTurnRequest, PROJECT_ENVIRONMENT_AGENT};
use chatos_ai_runtime::ModelRuntimeConfig;
use chatos_mcp_runtime::BuiltinMcpKind;
use chatos_plugin_management_sdk::{
    ResolveAgentCapabilitiesRequest, ResolvedAgentCapabilities, SystemAgentKey,
    PROJECT_ENVIRONMENT_MCP_RESOURCE_ID,
};
use serde_json::{json, Value};

use super::runtime_environment::{
    enforce_project_runtime_boundary, ensure_runtime_environment_for_project,
};

mod inspection;
mod mcp_servers;
mod memory;
mod progress;
mod routing;
mod tool_provider;

pub use self::progress::get_project_runtime_environment_progress;

use self::inspection::{inspect_local_project, LocalProjectInspection};
use self::mcp_servers::{
    build_project_environment_mcp_executor, create_sandbox_image_from_plan,
    ensure_agent_required_tools_available, start_local_project_compose_environment,
};
use self::memory::{build_project_agent_memory, ProjectAgentMemory};
use self::routing::{
    provider_label, resolve_runtime_environment_routing, RoutingDecision, RoutingPlan, StopDecision,
};

const LOCAL_CONNECTOR_ROOT_PREFIX: &str = "local://connector/";
const PROJECT_ENVIRONMENT_MCP_SERVER_NAME: &str = "project_environment";
const SANDBOX_IMAGE_MCP_SERVER_NAME: &str = "sandbox_images";
const CLOUD_SANDBOX_IMAGE_MCP_PATH: &str = "/api/sandbox-images/mcp";
const LOCAL_SANDBOX_IMAGE_MCP_PATH: &str = "/api/local/sandbox/images/mcp";
const PROJECT_COMPOSE_FILE_PATH: &str = ".chatos/runtime-environment/docker-compose.chatos.yml";

pub async fn start_project_runtime_environment(
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
            compose_dependency_image_ref(image)
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

fn compose_dependency_image_ref(image: &ProjectRuntimeEnvironmentImageRecord) -> Option<String> {
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

pub async fn generate_project_runtime_environment_image(
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

pub async fn analyze_project_runtime_environment(
    state: &AppState,
    project: &ProjectRecord,
    user_access_token: Option<&str>,
    run_id: &str,
) -> Result<ProjectRuntimeEnvironmentResponse, String> {
    if project.execution_plane == ProjectExecutionPlane::LocalConnector {
        return Err(format!(
            "local_runtime_required: project {} orchestration must run in the Local Connector client; cloud project agent execution is disabled",
            project.id
        ));
    }
    let mut environment =
        ensure_runtime_environment_for_project(&state.store, project, None).await?;
    let run_id = run_id.to_string();

    if !environment.sandbox_enabled {
        environment.status = ProjectRuntimeEnvironmentStatus::Disabled;
        environment.sandbox_provider = RuntimeEnvironmentProvider::None;
        environment.file_provider = RuntimeEnvironmentProvider::None;
        environment.analysis_summary =
            Some("该项目已关闭沙箱环境初始化，不会自动分析或创建运行环境镜像。".to_string());
        environment.not_runnable_reason = None;
        environment.last_agent_run_id = Some(run_id);
        environment.last_error = None;
        environment.updated_at = now_rfc3339();
        let environment = state
            .store
            .upsert_project_runtime_environment(&environment)
            .await?;
        state
            .store
            .replace_project_runtime_environment_images(project.id.as_str(), &[])
            .await?;
        return response_for_project(state, environment).await;
    }

    let owner_user_id = project
        .owner_user_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let Some(owner_user_id) = owner_user_id else {
        environment.status = ProjectRuntimeEnvironmentStatus::Failed;
        environment.analysis_summary =
            Some("无法运行项目管理 Agent：项目缺少 owner_user_id。".to_string());
        environment.last_error = Some("project owner_user_id is required".to_string());
        environment.updated_at = now_rfc3339();
        let environment = state
            .store
            .upsert_project_runtime_environment(&environment)
            .await?;
        return response_for_project(state, environment).await;
    };

    let capability_policy =
        match resolve_project_agent_capabilities(state, owner_user_id, user_access_token).await {
            Ok(policy) => policy,
            Err(err) => {
                environment.status = ProjectRuntimeEnvironmentStatus::Failed;
                environment.analysis_summary =
                    Some("项目管理 Agent 所需 MCP 能力不可用。".to_string());
                environment.last_error = Some(err);
                environment.updated_at = now_rfc3339();
                let environment = state
                    .store
                    .upsert_project_runtime_environment(&environment)
                    .await?;
                return response_for_project(state, environment).await;
            }
        };

    let routing = match resolve_runtime_environment_routing(
        project,
        &state.config,
        user_access_token,
    )
    .await
    {
        RoutingDecision::Stop(stop) => {
            apply_stop_decision(&mut environment, run_id, stop);
            let environment = state
                .store
                .upsert_project_runtime_environment(&environment)
                .await?;
            if matches!(
                environment.status,
                ProjectRuntimeEnvironmentStatus::NotRunnable
                    | ProjectRuntimeEnvironmentStatus::Disabled
                    | ProjectRuntimeEnvironmentStatus::Failed
            ) {
                state
                    .store
                    .replace_project_runtime_environment_images(project.id.as_str(), &[])
                    .await?;
            }
            return response_for_project(state, environment).await;
        }
        RoutingDecision::Ready(routing) => routing,
    };

    environment.status = ProjectRuntimeEnvironmentStatus::Analyzing;
    environment.file_provider = routing.file_provider;
    environment.sandbox_provider = routing.sandbox_provider;
    environment.analysis_summary = Some(routing.summary.clone());
    environment.not_runnable_reason = None;
    environment.last_agent_run_id = Some(run_id.clone());
    environment.last_error = None;
    environment.updated_at = now_rfc3339();
    environment = state
        .store
        .upsert_project_runtime_environment(&environment)
        .await?;

    let model_runtime = match resolve_default_environment_initialization_model_runtime(
        &state.config,
        owner_user_id,
    )
    .await
    {
        Ok(Some(runtime)) => runtime,
        Ok(None) => {
            environment.status = ProjectRuntimeEnvironmentStatus::PendingConfiguration;
            environment.analysis_summary = Some(
                "项目可进入运行环境分析，但还没有配置“环境初始化模型”。请先在用户菜单中配置默认模型。"
                    .to_string(),
            );
            environment.last_error = None;
            environment.updated_at = now_rfc3339();
            let environment = state
                .store
                .upsert_project_runtime_environment(&environment)
                .await?;
            return response_for_project(state, environment).await;
        }
        Err(err) => {
            environment.status = ProjectRuntimeEnvironmentStatus::Failed;
            environment.analysis_summary = Some("读取环境初始化模型配置失败。".to_string());
            environment.last_error = Some(err);
            environment.updated_at = now_rfc3339();
            let environment = state
                .store
                .upsert_project_runtime_environment(&environment)
                .await?;
            return response_for_project(state, environment).await;
        }
    };

    let local_inspection = inspect_local_project(project);
    let memory = match build_project_agent_memory(
        &state.config,
        owner_user_id,
        project.id.as_str(),
        user_access_token,
    )
    .await
    {
        Ok(memory) => memory,
        Err(err) => {
            environment.status = ProjectRuntimeEnvironmentStatus::Failed;
            environment.analysis_summary =
                Some("项目管理 Agent Memory Engine 初始化失败。".to_string());
            environment.last_error = Some(err);
            environment.updated_at = now_rfc3339();
            let environment = state
                .store
                .upsert_project_runtime_environment(&environment)
                .await?;
            return response_for_project(state, environment).await;
        }
    };
    let agent_result = run_project_environment_agent(
        state,
        project,
        &environment,
        routing,
        &model_runtime.model_config,
        local_inspection.as_ref(),
        &memory,
        user_access_token,
        run_id.as_str(),
        &capability_policy,
    )
    .await;

    match agent_result {
        Ok(()) => {
            let Some(environment) = state
                .store
                .get_project_runtime_environment(project.id.as_str())
                .await?
            else {
                return Err(
                    "project environment agent did not persist runtime environment".to_string(),
                );
            };
            if environment.status == ProjectRuntimeEnvironmentStatus::Analyzing {
                let mut failed = environment;
                failed.status = ProjectRuntimeEnvironmentStatus::Failed;
                failed.analysis_summary =
                    Some("项目管理 Agent 已执行，但没有写入运行环境初始化结果。".to_string());
                failed.last_error = Some(
                    "agent did not call update_current_project_runtime_environment".to_string(),
                );
                failed.updated_at = now_rfc3339();
                let failed = state
                    .store
                    .upsert_project_runtime_environment(&failed)
                    .await?;
                return response_for_project(state, failed).await;
            }
            response_for_project(state, environment).await
        }
        Err(err) => {
            environment.status = ProjectRuntimeEnvironmentStatus::Failed;
            environment.analysis_summary = Some("项目管理 Agent 初始化运行环境失败。".to_string());
            environment.last_error = Some(err.clone());
            environment.updated_at = now_rfc3339();
            tracing::warn!(
                project_id = project.id.as_str(),
                model_config_id = model_runtime.model_config_id.as_str(),
                model = model_runtime.model_config.model.as_str(),
                error = err.as_str(),
                "project environment agent failed"
            );
            let environment = state
                .store
                .upsert_project_runtime_environment(&environment)
                .await?;
            response_for_project(state, environment).await
        }
    }
}

async fn response_for_project(
    state: &AppState,
    environment: ProjectRuntimeEnvironmentRecord,
) -> Result<ProjectRuntimeEnvironmentResponse, String> {
    let images = state
        .store
        .list_project_runtime_environment_images(environment.project_id.as_str())
        .await?;
    Ok(ProjectRuntimeEnvironmentResponse {
        environment,
        images,
    })
}

async fn run_project_environment_agent(
    state: &AppState,
    project: &ProjectRecord,
    environment: &ProjectRuntimeEnvironmentRecord,
    routing: RoutingPlan,
    model_config: &ModelRuntimeConfig,
    local_inspection: Option<&LocalProjectInspection>,
    memory: &ProjectAgentMemory,
    user_access_token: Option<&str>,
    run_id: &str,
    capability_policy: &ResolvedAgentCapabilities,
) -> Result<(), String> {
    let executor = build_project_environment_mcp_executor(
        state,
        project,
        &routing,
        user_access_token,
        run_id,
        capability_policy,
    )
    .await?;
    ensure_agent_required_tools_available(&executor, &routing)?;

    let mut prompt = build_project_environment_agent_prompt(
        project,
        environment,
        &routing,
        local_inspection,
        run_id,
    )?;
    let effective_mcp_resource_ids = effective_project_environment_mcp_resource_ids(&routing);
    if let Some(provider_skills_prompt) = capability_policy.compose_provider_skills_prompt(
        effective_mcp_resource_ids.iter().map(String::as_str),
        Some("zh-CN"),
    ) {
        prompt.push_str("\n\n");
        prompt.push_str(provider_skills_prompt.trim());
    }
    let metadata = json!({
        "agent": "project_management_environment_agent",
        "run_id": run_id,
        "project_id": project.id,
        "file_provider": routing.file_provider.as_str(),
        "sandbox_provider": routing.sandbox_provider.as_str(),
    });
    let agent_memory = AgentTurnMemory::new(
        memory.composer.clone(),
        memory.writer.clone(),
        memory.scope.clone(),
        memory.conversation_id.clone(),
    );
    let request = AgentTurnRequest::new(
        model_config.clone(),
        memory.conversation_id.clone(),
        run_id,
        prompt,
    )
    .with_mcp_executor(executor)
    .with_memory(Some(agent_memory))
    .with_max_iterations(chatos_agent::load_agent_max_iterations("project-service").await)
    .with_metadata(metadata);
    let result = AgentExecutor::new()
        .run(&PROJECT_ENVIRONMENT_AGENT, request)
        .await
        .map_err(|error| error.message().to_string())?;
    tracing::info!(
        project_id = project.id.as_str(),
        run_id,
        finish_reason = result.finish_reason.as_deref().unwrap_or(""),
        "project environment agent completed"
    );
    Ok(())
}

async fn resolve_project_agent_capabilities(
    state: &AppState,
    owner_user_id: &str,
    user_access_token: Option<&str>,
) -> Result<ResolvedAgentCapabilities, String> {
    let request =
        ResolveAgentCapabilitiesRequest::new(SystemAgentKey::ProjectManagementAgent, owner_user_id);
    let capabilities = if let Some(access_token) = user_access_token
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        state
            .plugin_management_client
            .resolve_for_user(&request, access_token)
            .await
            .map_err(|err| err.to_string())?
    } else {
        state
            .plugin_management_client
            .resolve_for_service(&request)
            .await
            .map_err(|err| err.to_string())?
    };
    capabilities
        .ensure_required_available()
        .map_err(|err| err.to_string())?;
    capabilities
        .ensure_required_skills_supported(std::iter::empty::<&str>())
        .map_err(|err| err.to_string())?;
    let code_read_resource_id = BuiltinMcpKind::CodeMaintainerRead
        .config_id()
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| "system_builtin_code_maintainer_read".to_string());
    for resource_id in [
        code_read_resource_id.as_str(),
        PROJECT_ENVIRONMENT_MCP_RESOURCE_ID,
    ] {
        capabilities
            .require_available_mcp(resource_id)
            .map_err(|err| err.to_string())?;
    }
    Ok(capabilities)
}

fn build_project_environment_agent_prompt(
    project: &ProjectRecord,
    environment: &ProjectRuntimeEnvironmentRecord,
    routing: &RoutingPlan,
    local_inspection: Option<&LocalProjectInspection>,
    run_id: &str,
) -> Result<String, String> {
    let detected_stack = serde_json::to_string_pretty(
        &local_inspection
            .map(|inspection| inspection.detected_stack.clone())
            .unwrap_or_else(empty_object),
    )
    .map_err(|err| format!("serialize detected stack failed: {err}"))?;
    let required_services = serde_json::to_string_pretty(
        &local_inspection
            .map(|inspection| inspection.required_services.clone())
            .unwrap_or_else(empty_array),
    )
    .map_err(|err| format!("serialize required services failed: {err}"))?;
    let manifest_context = serde_json::to_string_pretty(
        &local_inspection
            .map(|inspection| inspection.manifest_context.clone())
            .unwrap_or_default(),
    )
    .map_err(|err| format!("serialize manifest context failed: {err}"))?;
    let file_tool_hint = file_tool_hint(project, routing.file_provider);
    Ok(format!(
        r#"请为当前项目初始化沙箱运行环境。你只做这个固定业务流程，不调用 task runner，不创建任务，不修改项目代码。

当前运行：
- run_id: {run_id}
- project_id: {project_id}
- project_name: {project_name}
- file_provider: {file_provider}
- sandbox_provider: {sandbox_provider}
- sandbox_enabled: {sandbox_enabled}

工具约束：
- 项目详情工具只操作当前项目：先调用 `project_environment_get_current_project_runtime_environment`，最后必须调用 `project_environment_update_current_project_runtime_environment` 写入结果。
- 文件读取工具：{file_tool_hint}
- 本轮只生成项目级容器编排计划，不调用 Sandbox Images 搜索或创建工具。后端会生成一个 `docker-compose.chatos.yml`，用户确认后一次性启动整个环境。
- 只为应用运行时生成完整 Dockerfile；MySQL、MongoDB、Redis、Nacos 等依赖服务使用平台维护的标准镜像，并作为同一个 Docker Compose 项目下的服务。Dockerfile 不得包含密码、令牌或其他密钥。
- 不要臆造文件中没有依据的依赖服务。可以先读 package.json、Cargo.toml、go.mod、pyproject.toml、pom.xml、build.gradle、docker-compose、README、.env.example 等关键文件。

强制执行顺序（不得跳步或调换）：
1. 全项目扫描：先列出项目目录结构，排除 target、node_modules、.git、构建产物和二进制文件；全局搜索所有环境变量引用，再读取命中的配置文件、启动入口、部署文件和说明文档。至少覆盖 `.env*`、`application*.yml/yaml/properties`、`bootstrap*.yml/yaml/properties`、Docker Compose、Dockerfile、Kubernetes/Helm、CI 配置、启动脚本、README，以及 `System.getenv`、`process.env`、`os.getenv`、`std::env`、GitHub Actions 表达式和 `${{VAR}}` 等代码引用。
2. 变量清单：在生成任何镜像 Dockerfile 计划之前，先形成完整 `environment_variables`。每个变量只有一个最终生效值：项目值适用就使用项目值；缺失或不适配就生成当前沙箱可用值。记录来源判断、是否必填、是否敏感和生成原因。
3. 依赖与运行计划：根据变量清单、构建清单和启动入口确认语言运行时、依赖服务、端口、启动命令和服务间连接方式。
4. 环境配置文件：根据前三步生成 `generated_config_files`，例如 `.env.chatos`、`application-chatos.yml`、`application-chatos.properties`、`config.chatos.toml` 或配置中心 YAML。文件路径必须相对项目工作区；敏感值和用户可编辑值使用环境变量占位符，不得写死到文件。项目确实不需要生成配置文件时也必须提交空数组并说明判断依据。
5. 项目级编排计划：完成前四步后，为应用运行时生成一个 Dockerfile；为每个依赖服务生成独立服务记录。所有记录使用 `status: "planned"`，不要实际申请或启动容器。后端会据此生成一个总的 Docker Compose 文件，把应用和全部依赖包在同一个项目组中。
6. 最终保存：调用项目环境更新工具，同时提交扫描证据、唯一值变量、配置文件、应用 Dockerfile 和全部依赖服务记录。不得为每个依赖生成散落的独立构建任务。

判断规则：
- 平台目标是让导入的项目尽快具备验证和迭代条件，必须采用“优先初始化、最后才判不可运行”的策略。
- 如果发现 Java、Node.js、Python、Go、Rust、.NET、PHP、Ruby 等应用运行时，必须为应用准备运行时镜像。
- 如果发现 nacos、postgres、mysql、redis、mongodb、rabbitmq 等外部依赖，必须把它们记录到 `required_services`，并分别生成对应环境的 Dockerfile 计划。远程地址、密码、配置中心文件缺失属于需要本地替代和自动配置的 provisioning 输入，不是 `not_runnable` 理由。
- 环境变量中出现 MYSQL、MONGO、REDIS、NACOS、POSTGRES、RABBITMQ、KAFKA、ELASTICSEARCH 或 MINIO 等引用时，也视为检测到了对应依赖，不得通过把 enabled 设置为 false 或漏写 `required_services` 来跳过环境准备。
- 对 Nacos 等远程配置中心，优先初始化本地兼容服务，并生成本地服务地址、命名空间、用户名、密码和令牌环境变量。对数据库和缓存同样生成容器内可访问的默认主机名、端口、数据库名与随机凭据。
- 标准 runtime features 只填写镜像目录真实支持的运行时版本；Redis、MongoDB、MySQL、Nacos 等非标准 feature 应使用基础镜像加 `custom_build_script` 安装，不得把不支持的服务名直接当作 runtime feature。
- 环境分析和项目级 Compose 计划完成后提交结果，后端会把整体状态置为 `pending_image_build`；用户点击一次“生成并启动整个环境”后才会变为 `ready`。
- 只有项目目录确实为空、没有任何可执行入口或构建清单、仅包含说明文档/零散配置且无法识别可启动组件时，才允许写入 `status: "not_runnable"`。不得因为缺少 application.yml、远程 datasource 地址、Nacos 配置、Redis/MongoDB 连接信息而判定不可运行。
- 如果确实需要无法自动生成的第三方业务凭据，在基础运行时和可自动创建的依赖镜像准备完成后写入 `pending_configuration`，列出需要用户补充的最小变量；不要写 `not_runnable`。
- 如果项目可运行，识别语言、框架、包管理器、启动方式和依赖服务。依赖服务包括但不限于 nacos、postgres、mysql、redis、mongodb、rabbitmq。
- 所有启动参数和环境变量优先写入 `environment_variables`：从项目配置/代码/.env 中读取到的值放入 `project_value` 并判断 `project_value_suitable`；缺失或不适配时必须生成 `recommended_value` 与 `recommendation_reason`。每个变量最终只有一个生效值，用户可以直接修改该值；Agent 不得写入或覆盖用户已经修改的值，服务会自动保留。
- 项目中的 localhost、127.0.0.1、宿主机绝对路径、远程生产域名或当前沙箱不可达地址通常不适合直接复用，应保留为 `project_value`、标记 `project_value_suitable: false`，并生成面向当前沙箱服务名和端口的推荐值。
- 数据库、Nacos、Redis 等需要启动密码/令牌时，为缺失项生成安全的推荐值；变量要标记 `secret: true`。`env_vars` 只作为旧格式兼容，新的分析结果必须优先提供 `environment_variables`。
- 根据项目实际加载配置的方式生成环境专用配置文件。Spring 项目可以生成独立 profile 配置，Node/Python 等项目可以生成专用 dotenv 或框架配置；使用 Nacos、Consul 等配置中心时生成对应 data-id 的配置内容。配置内容必须来自扫描到的配置键、代码引用和已经确定的变量，不得臆造无依据的业务配置。
- 生成文件不得覆盖项目已有配置文件，默认使用带 `chatos` 或 `sandbox` 标识的新文件名；在 `source_files` 中记录推断依据。密码、令牌、密钥和用户后续可能修改的值使用 `${{ENV_NAME}}` 等目标格式支持的环境变量引用。
- 为应用运行时准备一个包含完整 Dockerfile 的记录；每个依赖服务分别准备一个不需要 Dockerfile 的服务记录。每条记录必须包含 environment_key/environment_type/display_name、ports、env_vars 和 `status: "planned"`。
- 后端会根据环境变量和 `required_services` 复核编排计划；缺少应用 Dockerfile 或遗漏任一 MySQL、MongoDB、Redis、Nacos 等依赖服务记录时会拒绝保存。

预扫描技术栈候选：
{detected_stack}

预扫描依赖服务候选：
{required_services}

本地预扫描关键文件预览（可能为空，仍需优先使用 MCP 文件工具确认）：
{manifest_context}

当前运行环境记录：
{environment_json}
"#,
        run_id = run_id,
        project_id = project.id,
        project_name = project.name,
        file_provider = provider_label(routing.file_provider),
        sandbox_provider = provider_label(routing.sandbox_provider),
        sandbox_enabled = environment.sandbox_enabled,
        environment_json = serde_json::to_string_pretty(environment)
            .map_err(|err| format!("serialize runtime environment failed: {err}"))?,
    ))
}

fn effective_project_environment_mcp_resource_ids(routing: &RoutingPlan) -> Vec<String> {
    let mut resource_ids = vec![PROJECT_ENVIRONMENT_MCP_RESOURCE_ID.to_string()];
    if matches!(
        routing.file_provider,
        RuntimeEnvironmentProvider::Harness | RuntimeEnvironmentProvider::LocalConnector
    ) {
        resource_ids.push(
            BuiltinMcpKind::CodeMaintainerRead
                .config_id()
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| "system_builtin_code_maintainer_read".to_string()),
        );
    }
    resource_ids
}

fn file_tool_hint(project: &ProjectRecord, provider: RuntimeEnvironmentProvider) -> String {
    match provider {
        RuntimeEnvironmentProvider::Harness => {
            "`harness_code_list_dir`、`harness_code_read_file_raw`、`harness_code_search_text`。"
                .to_string()
        }
        RuntimeEnvironmentProvider::LocalConnector => {
            if project
                .root_path
                .as_deref()
                .is_some_and(|root| root.trim().starts_with(LOCAL_CONNECTOR_ROOT_PREFIX))
            {
                "`local_connector_list_dir`、`local_connector_read_file_raw`、`local_connector_search_text`。".to_string()
            } else {
                "`code_maintainer_read_list_dir`、`code_maintainer_read_read_file_raw`、`code_maintainer_read_search_text`。".to_string()
            }
        }
        RuntimeEnvironmentProvider::CloudSandboxManager | RuntimeEnvironmentProvider::None => {
            "没有可用文件 MCP。".to_string()
        }
    }
}

fn apply_stop_decision(
    environment: &mut ProjectRuntimeEnvironmentRecord,
    run_id: String,
    stop: StopDecision,
) {
    environment.status = stop.status;
    environment.sandbox_provider = RuntimeEnvironmentProvider::None;
    environment.file_provider = RuntimeEnvironmentProvider::None;
    environment.analysis_summary = Some(stop.summary);
    environment.not_runnable_reason = stop.not_runnable_reason;
    environment.last_agent_run_id = Some(run_id);
    environment.last_error = stop.last_error;
    environment.updated_at = now_rfc3339();
}
