// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::*;
use crate::state::AppState;
use crate::user_model_runtime_client::resolve_default_project_agent_model_runtime;
use chatos_agent::{AgentExecutor, AgentTurnMemory, AgentTurnRequest, PROJECT_ENVIRONMENT_AGENT};
use chatos_ai_runtime::ModelRuntimeConfig;
use chatos_mcp_runtime::BuiltinMcpKind;
use chatos_plugin_management_sdk::{
    ResolveAgentCapabilitiesRequest, ResolvedAgentCapabilities, SystemAgentKey,
    PROJECT_ENVIRONMENT_MCP_RESOURCE_ID, SANDBOX_IMAGES_MCP_RESOURCE_ID,
};
use serde_json::json;

use super::runtime_environment::ensure_runtime_environment_for_project;

mod inspection;
mod mcp_servers;
mod memory;
mod progress;
mod routing;
mod tool_provider;

pub use self::progress::get_project_runtime_environment_progress;

use self::inspection::{inspect_local_project, LocalProjectInspection};
use self::mcp_servers::{
    build_project_environment_mcp_executor, ensure_agent_required_tools_available,
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
pub async fn analyze_project_runtime_environment(
    state: &AppState,
    project: &ProjectRecord,
    user_access_token: Option<&str>,
    run_id: &str,
) -> Result<ProjectRuntimeEnvironmentResponse, String> {
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

    let model_runtime = match resolve_default_project_agent_model_runtime(
        &state.config,
        owner_user_id,
    )
    .await
    {
        Ok(Some(runtime)) => runtime,
        Ok(None) => {
            environment.status = ProjectRuntimeEnvironmentStatus::PendingConfiguration;
            environment.analysis_summary = Some(
                    "项目可进入运行环境分析，但还没有配置“项目管理 Agent 模型”。请先在用户菜单中配置默认模型。"
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
            environment.analysis_summary = Some("读取项目管理 Agent 模型配置失败。".to_string());
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
        environment,
        &routing,
        user_access_token,
        run_id,
        capability_policy,
    )
    .await?;
    ensure_agent_required_tools_available(&executor, project, &routing)?;

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
        SANDBOX_IMAGES_MCP_RESOURCE_ID,
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
- 沙箱镜像工具：使用 `sandbox_images_search_images` 搜索已有镜像；没有可用镜像时调用 `sandbox_images_create_image`。标准 features 无法覆盖项目依赖时，可以在 `custom_build_script` 中提供非交互 Bash 脚本，由镜像构建器在安装标准运行时后以 root 执行。脚本应可重复执行、使用 `set -e`、不得写入密钥，退出非 0 会使镜像创建失败。创建镜像必须同步等待，调用时传 `timeout_ms: 7200000`，不要做异步轮询或反复查进度；成功结果会在顶层返回 `image_id` 和 `image_ref`，必须直接写入镜像记录。
- 每次初始化都必须在本轮实际调用 `sandbox_images_search_images` 获取当前镜像状态；如果没有满足依赖的可用镜像且项目可运行，必须在本轮调用一次 `sandbox_images_create_image`。不得根据 Memory Engine 中以前的镜像失败、Docker 错误或旧运行环境记录直接判定本轮仍然失败。
- 不要臆造文件中没有依据的依赖服务。可以先读 package.json、Cargo.toml、go.mod、pyproject.toml、pom.xml、build.gradle、docker-compose、README、.env.example 等关键文件。

判断规则：
- 平台目标是让导入的项目尽快具备验证和迭代条件，必须采用“优先初始化、最后才判不可运行”的策略。
- 如果发现 Java、Node.js、Python、Go、Rust、.NET、PHP、Ruby 等应用运行时，必须为应用准备运行时镜像。
- 如果发现 nacos、postgres、mysql、redis、mongodb、rabbitmq 等外部依赖，必须把它们记录到 `required_services`，并分别搜索或创建对应环境镜像。远程地址、密码、配置中心文件缺失属于需要本地替代和自动配置的 provisioning 输入，不是 `not_runnable` 理由。
- 对 Nacos 等远程配置中心，优先初始化本地兼容服务，并生成本地服务地址、命名空间、用户名、密码和令牌环境变量。对数据库和缓存同样生成容器内可访问的默认主机名、端口、数据库名与随机凭据。
- 标准 runtime features 只填写镜像目录真实支持的运行时版本；Redis、MongoDB、MySQL、Nacos 等非标准 feature 应使用基础镜像加 `custom_build_script` 安装，不得把不支持的服务名直接当作 runtime feature。
- 环境镜像全部准备成功后写入 `status: "ready"`。即使原项目引用的是远程 Nacos、Redis 或 MongoDB，只要已创建本地替代环境并生成连接配置，也应写为 `ready`，并在分析摘要说明替代方案。
- 只有项目目录确实为空、没有任何可执行入口或构建清单、仅包含说明文档/零散配置且无法识别可启动组件时，才允许写入 `status: "not_runnable"`。不得因为缺少 application.yml、远程 datasource 地址、Nacos 配置、Redis/MongoDB 连接信息而判定不可运行。
- 如果确实需要无法自动生成的第三方业务凭据，在基础运行时和可自动创建的依赖镜像准备完成后写入 `pending_configuration`，列出需要用户补充的最小变量；不要写 `not_runnable`。
- 如果项目可运行，识别语言、框架、包管理器、启动方式和依赖服务。依赖服务包括但不限于 nacos、postgres、mysql、redis、mongodb、rabbitmq。
- 数据库、nacos、redis 等需要启动密码/令牌时，在 `env_vars` 里给出环境变量名；值可以留空或给出非真实占位，服务会补齐随机值和本地连接默认值。
- 对每个运行时/依赖服务准备镜像记录。搜索到可用镜像就复用；搜索不到就创建。镜像记录要包含 environment_key、environment_type、display_name、image_id/image_ref、features、ports、env_vars、status。
- 完成后把 `status` 写成 `ready`；如果镜像创建失败，把 `status` 写成 `failed`，并把失败原因写入 `last_error` 和对应 image.error。不要用 `not_runnable` 代替镜像创建或配置生成失败。

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
    if matches!(
        routing.sandbox_provider,
        RuntimeEnvironmentProvider::LocalConnector
            | RuntimeEnvironmentProvider::CloudSandboxManager
    ) {
        resource_ids.push(SANDBOX_IMAGES_MCP_RESOURCE_ID.to_string());
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
