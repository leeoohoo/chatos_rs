// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use crate::models::*;
use crate::state::AppState;
use crate::user_model_runtime_client::resolve_default_project_agent_model_runtime;
use chatos_ai_runtime::{
    AiRuntime, ContextualTurnRunner, MemoryContextOverflowRecovery, ModelRuntimeConfig,
    RuntimeRecordOptions, RuntimeTurnSpec, SaveRecordInput,
};
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
const PROJECT_ENVIRONMENT_AGENT_MAX_ITERATIONS: usize = 600;

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

    let mut agent_model_config = model_config.clone();
    agent_model_config.instructions = Some(project_environment_agent_system_prompt(
        model_config.instructions.as_deref(),
    ));
    if agent_model_config.temperature.is_none() {
        agent_model_config.temperature = Some(0.1);
    }
    if agent_model_config.max_output_tokens.is_none() {
        agent_model_config.max_output_tokens = Some(4_000);
    }

    let runtime = AiRuntime::from_mcp_executor(executor)
        .with_max_iterations(PROJECT_ENVIRONMENT_AGENT_MAX_ITERATIONS)
        .with_record_writer(Some(Arc::new(memory.writer.clone())));
    let runner = ContextualTurnRunner::new(runtime, Some(memory.composer.clone()))
        .with_context_overflow_recovery(Some(
            MemoryContextOverflowRecovery::new()
                .with_trigger_reason("project_environment_agent_context_overflow"),
        ));

    let prompt = build_project_environment_agent_prompt(
        project,
        environment,
        &routing,
        local_inspection,
        run_id,
    )?;
    let conversation_id = memory.conversation_id.clone();
    let metadata = json!({
        "agent": "project_management_environment_agent",
        "run_id": run_id,
        "project_id": project.id,
        "file_provider": routing.file_provider.as_str(),
        "sandbox_provider": routing.sandbox_provider.as_str(),
    });
    let user_record = Some(
        SaveRecordInput::user_message(memory.conversation_id.clone(), prompt.clone())
            .with_conversation_turn_id(run_id.to_string())
            .with_message_mode("project_environment_agent")
            .with_message_source("project_management_service")
            .with_metadata(metadata.clone()),
    );
    let record_options = RuntimeRecordOptions::persist_all()
        .with_assistant_message_mode("project_environment_agent")
        .with_assistant_message_source("project_management_service")
        .with_assistant_metadata(metadata.clone())
        .with_tool_message_mode("project_environment_agent")
        .with_tool_message_source("project_management_service")
        .with_tool_metadata(metadata);
    let spec = RuntimeTurnSpec::for_user_text(agent_model_config, conversation_id, prompt)
        .with_conversation_turn_id(run_id.to_string())
        .with_caller_model(model_config.model.clone())
        .with_record_options(record_options)
        .with_memory_scope(Some(memory.scope.clone()))
        .with_user_record(user_record);
    let result = runner.run_turn(spec.into_contextual_turn_request()).await?;
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

fn project_environment_agent_system_prompt(existing: Option<&str>) -> String {
    let fixed = "你是 Project Management Service 内置的运行环境初始化 Agent。你的业务范围固定：读取当前项目文件，判断项目是否可运行，识别运行时和依赖服务，使用沙箱镜像 MCP 搜索或同步创建所需镜像，然后通过项目环境工具写入当前项目的运行环境结果。不要处理需求拆解、任务执行、代码修改或其它项目管理任务。";
    existing
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| format!("{value}\n\n{fixed}"))
        .unwrap_or_else(|| fixed.to_string())
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
- 不要臆造文件中没有依据的依赖服务。可以先读 package.json、Cargo.toml、go.mod、pyproject.toml、pom.xml、build.gradle、docker-compose、README、.env.example 等关键文件。

判断规则：
- 如果项目为空、缺少入口、只有文档/配置片段、或明显不具备运行条件，直接调用更新工具写入 `status: "not_runnable"` 和中文 `not_runnable_reason`，不要创建镜像。
- 如果项目可运行，识别语言、框架、包管理器、启动方式和依赖服务。依赖服务包括但不限于 nacos、postgres、mysql、redis、mongodb、rabbitmq。
- 数据库、nacos、redis 等需要启动密码/令牌时，在 `env_vars` 里给出环境变量名；值可以留空或给出非真实占位，服务会补齐随机值。
- 对每个运行时/依赖服务准备镜像记录。搜索到可用镜像就复用；搜索不到就创建。镜像记录要包含 environment_key、environment_type、display_name、image_id/image_ref、features、ports、env_vars、status。
- 完成后把 `status` 写成 `ready`；如果镜像创建失败，把 `status` 写成 `failed`，并把失败原因写入 `last_error` 和对应 image.error。

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
