// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use chatos_ai_runtime::{
    AiRuntime, ContextualTurnRunner, MemoryContextComposer, MemoryContextOverflowRecovery,
    MemoryRecordScope, MemoryScope, ModelRuntimeConfig, RuntimeRecordOptions, RuntimeTurnSpec,
    SaveRecordInput,
};
use chatos_mcp_runtime::{
    BuiltinMcpKind, BuiltinMcpServerOptions, BuiltinToolProvider, McpBuiltinServer, McpExecutor,
    McpHttpServer, ToolCallContext, ToolStreamChunkCallback,
};
use chatos_mcp_service::{
    builtin_kind_header_value, HARNESS_CODE_ENABLED_BUILTIN_KINDS_HEADER,
    LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER,
};

use crate::config::AppConfig;
use crate::models::*;
use crate::state::AppState;
use crate::user_model_runtime_client::resolve_default_project_agent_model_runtime;

use super::runtime_environment::{
    default_runtime_environment_for_project, ensure_runtime_environment_for_project,
};

mod routing;

use self::routing::{
    find_enabled_local_sandbox_pairing, parse_local_connector_project_root, provider_label,
    resolve_runtime_environment_routing, LocalConnectorProjectRef, RoutingDecision, RoutingPlan,
    StopDecision,
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
) -> Result<ProjectRuntimeEnvironmentResponse, String> {
    let mut environment =
        ensure_runtime_environment_for_project(&state.store, project, None).await?;
    let run_id = format!("project_env_agent_{}", Uuid::new_v4());

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
) -> Result<(), String> {
    let executor = build_project_environment_mcp_executor(
        state,
        project,
        environment,
        &routing,
        user_access_token,
        run_id,
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

async fn build_project_environment_mcp_executor(
    state: &AppState,
    project: &ProjectRecord,
    environment: &ProjectRuntimeEnvironmentRecord,
    routing: &RoutingPlan,
    user_access_token: Option<&str>,
    run_id: &str,
) -> Result<McpExecutor, String> {
    let mut builder = McpExecutor::builder()
        .with_builtin_server(project_environment_builtin_server())
        .with_builtin_provider(ProjectEnvironmentToolProvider {
            state: state.clone(),
            project: project.clone(),
            run_id: run_id.to_string(),
        });

    match routing.file_provider {
        RuntimeEnvironmentProvider::Harness => {
            builder = builder.with_http_server(harness_file_mcp_server(&state.config, project)?);
        }
        RuntimeEnvironmentProvider::LocalConnector => {
            if let Some(project_ref) = project
                .root_path
                .as_deref()
                .and_then(parse_local_connector_project_root)
            {
                builder = builder.with_http_server(local_connector_file_mcp_server(
                    &state.config,
                    &project_ref,
                    user_access_token,
                )?);
            } else if let Some(root_path) = direct_local_project_root(project) {
                let server = BuiltinMcpKind::CodeMaintainerRead.server_with_options(
                    &BuiltinMcpServerOptions::new(root_path)
                        .with_project_id(project.id.clone())
                        .with_limits(512 * 1024, 5 * 1024 * 1024, 80),
                );
                let provider = chatos_builtin_tools::build_shared_builtin_provider(&server)?
                    .ok_or_else(|| {
                        "CodeMaintainerRead builtin provider is unavailable".to_string()
                    })?;
                builder = builder
                    .with_builtin_server(server)
                    .with_builtin_provider(provider);
            }
        }
        RuntimeEnvironmentProvider::None | RuntimeEnvironmentProvider::CloudSandboxManager => {}
    }

    let sandbox_server = match routing.sandbox_provider {
        RuntimeEnvironmentProvider::LocalConnector => {
            local_connector_sandbox_image_mcp_server(state, project, user_access_token).await?
        }
        RuntimeEnvironmentProvider::CloudSandboxManager => {
            cloud_sandbox_image_mcp_server(&state.config, environment.sandbox_provider)?
        }
        RuntimeEnvironmentProvider::None | RuntimeEnvironmentProvider::Harness => None,
    };
    if let Some(server) = sandbox_server {
        builder = builder.with_http_server(server);
    }

    builder.build_initialized().await
}

fn project_environment_builtin_server() -> McpBuiltinServer {
    McpBuiltinServer {
        name: PROJECT_ENVIRONMENT_MCP_SERVER_NAME.to_string(),
        kind: "ProjectEnvironmentRuntime".to_string(),
        workspace_dir: String::new(),
        user_id: None,
        project_id: None,
        remote_connection_id: None,
        contact_agent_id: None,
        auto_create_task: false,
        allow_writes: true,
        max_file_bytes: 0,
        max_write_bytes: 0,
        search_limit: 0,
    }
}

fn ensure_agent_required_tools_available(
    executor: &McpExecutor,
    project: &ProjectRecord,
    routing: &RoutingPlan,
) -> Result<(), String> {
    let tool_names = executor
        .available_tools()
        .into_iter()
        .filter_map(|tool| {
            tool.get("name")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .collect::<Vec<_>>();
    let has_project_update = tool_names
        .iter()
        .any(|name| name == "project_environment_update_current_project_runtime_environment");
    if !has_project_update {
        return Err("project environment update tool is unavailable".to_string());
    }
    let has_file_reader = tool_names.iter().any(|name| {
        name.ends_with("_read_file_raw")
            || name.ends_with("_read_file_range")
            || name.ends_with("_list_dir")
            || name.ends_with("_search_text")
    });
    if !has_file_reader {
        return Err(format!(
            "项目文件 MCP 不可用，无法分析项目文件：{}",
            provider_label(routing.file_provider)
        ));
    }
    let has_sandbox_images = tool_names
        .iter()
        .any(|name| name == "sandbox_images_search_images")
        && tool_names
            .iter()
            .any(|name| name == "sandbox_images_create_image");
    if !has_sandbox_images {
        return Err(format!(
            "沙箱镜像 MCP 不可用，无法为项目 {} 初始化运行环境镜像。",
            project.id
        ));
    }
    Ok(())
}

fn harness_file_mcp_server(
    config: &AppConfig,
    project: &ProjectRecord,
) -> Result<McpHttpServer, String> {
    let sync_secret = config
        .sync_secret
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "PROJECT_SERVICE_SYNC_SECRET is required for Harness MCP".to_string())?;
    let base = project_service_base_url(config);
    let mut headers = HashMap::new();
    headers.insert(
        "x-project-service-sync-secret".to_string(),
        sync_secret.to_string(),
    );
    headers.insert("x-task-runner-project-id".to_string(), project.id.clone());
    headers.insert(
        HARNESS_CODE_ENABLED_BUILTIN_KINDS_HEADER.to_string(),
        builtin_kind_header_value(["CodeMaintainerRead"]),
    );
    Ok(McpHttpServer::new(
        "harness_code",
        format!(
            "{base}/api/chatos-sync/projects/{}/harness/mcp",
            urlencoding::encode(project.id.as_str())
        ),
    )
    .with_headers(headers)
    .with_timeout(Duration::from_secs(90)))
}

fn local_connector_file_mcp_server(
    config: &AppConfig,
    project_ref: &LocalConnectorProjectRef,
    user_access_token: Option<&str>,
) -> Result<McpHttpServer, String> {
    let access_token = required_user_access_token(user_access_token, "Local Connector 文件 MCP")?;
    let mut url = format!(
        "{}/api/local-connectors/relay/{}/mcp?workspace_id={}",
        config
            .local_connector_service_base_url
            .trim()
            .trim_end_matches('/'),
        urlencoding::encode(project_ref.device_id.as_str()),
        urlencoding::encode(project_ref.workspace_id.as_str())
    );
    if let Some(relative_path) = project_ref.relative_path.as_deref() {
        url.push_str("&cwd=");
        url.push_str(urlencoding::encode(relative_path).as_ref());
    }
    let mut headers = HashMap::new();
    headers.insert(
        "authorization".to_string(),
        format!("Bearer {access_token}"),
    );
    headers.insert(
        LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER.to_string(),
        builtin_kind_header_value(["CodeMaintainerRead"]),
    );
    Ok(McpHttpServer::new("local_connector", url)
        .with_headers(headers)
        .with_timeout(Duration::from_secs(90)))
}

fn cloud_sandbox_image_mcp_server(
    config: &AppConfig,
    provider: RuntimeEnvironmentProvider,
) -> Result<Option<McpHttpServer>, String> {
    if provider != RuntimeEnvironmentProvider::CloudSandboxManager {
        return Ok(None);
    }
    let client_id = config
        .sandbox_manager_client_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "PROJECT_SERVICE_SANDBOX_MANAGER_CLIENT_ID is required".to_string())?;
    let client_key = config
        .sandbox_manager_client_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "PROJECT_SERVICE_SANDBOX_MANAGER_CLIENT_KEY is required".to_string())?;
    let mut headers = HashMap::new();
    headers.insert("x-sandbox-client-id".to_string(), client_id.to_string());
    headers.insert("x-sandbox-client-key".to_string(), client_key.to_string());
    let url = format!(
        "{}{}",
        config.sandbox_manager_base_url.trim().trim_end_matches('/'),
        CLOUD_SANDBOX_IMAGE_MCP_PATH
    );
    Ok(Some(
        McpHttpServer::new(SANDBOX_IMAGE_MCP_SERVER_NAME, url)
            .with_headers(headers)
            .with_timeout(config.sandbox_image_mcp_request_timeout),
    ))
}

async fn local_connector_sandbox_image_mcp_server(
    state: &AppState,
    project: &ProjectRecord,
    user_access_token: Option<&str>,
) -> Result<Option<McpHttpServer>, String> {
    let access_token =
        required_user_access_token(user_access_token, "Local Connector 沙箱镜像 MCP")?;
    let project_ref = project
        .root_path
        .as_deref()
        .and_then(parse_local_connector_project_root);
    let pairing =
        find_enabled_local_sandbox_pairing(&state.config, Some(access_token), project_ref.as_ref())
            .await?
            .ok_or_else(|| "没有找到已启用的 Local Connector 沙箱配对".to_string())?;
    let facade_base = pairing
        .id
        .as_deref()
        .map(|id| {
            format!(
                "{}/api/local-connectors/sandbox-facade/{}",
                state
                    .config
                    .local_connector_service_base_url
                    .trim()
                    .trim_end_matches('/'),
                urlencoding::encode(id)
            )
        })
        .or_else(|| {
            pairing
                .facade_base_url
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        })
        .ok_or_else(|| "Local Connector 沙箱配对缺少 facade_base_url".to_string())?;
    let mut headers = HashMap::new();
    headers.insert(
        "authorization".to_string(),
        format!("Bearer {access_token}"),
    );
    Ok(Some(
        McpHttpServer::new(
            SANDBOX_IMAGE_MCP_SERVER_NAME,
            format!(
                "{}{}",
                facade_base.trim_end_matches('/'),
                LOCAL_SANDBOX_IMAGE_MCP_PATH
            ),
        )
        .with_headers(headers)
        .with_timeout(state.config.sandbox_image_mcp_request_timeout),
    ))
}

fn project_service_base_url(config: &AppConfig) -> String {
    let host = match config.host {
        std::net::IpAddr::V4(addr) if addr.is_unspecified() => "127.0.0.1".to_string(),
        std::net::IpAddr::V6(addr) if addr.is_unspecified() => "127.0.0.1".to_string(),
        other => other.to_string(),
    };
    format!("http://{host}:{}", config.port)
}

fn direct_local_project_root(project: &ProjectRecord) -> Option<String> {
    if !matches!(project.source_type, ProjectSourceType::Local) {
        return None;
    }
    let root = project.root_path.as_deref()?.trim();
    if root.starts_with(LOCAL_CONNECTOR_ROOT_PREFIX) {
        return None;
    }
    Path::new(root).is_dir().then(|| root.to_string())
}

fn required_user_access_token<'a>(
    user_access_token: Option<&'a str>,
    label: &str,
) -> Result<&'a str, String> {
    user_access_token
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{label} 需要用户访问令牌"))
}

fn normalize_owned(value: String) -> Option<String> {
    normalized_optional(Some(value))
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
- 沙箱镜像工具：使用 `sandbox_images_search_images` 搜索已有镜像；没有可用镜像时调用 `sandbox_images_create_image`。创建镜像必须同步等待，调用时传 `timeout_ms: 7200000`，不要做异步轮询或反复查进度。
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

#[derive(Clone)]
struct ProjectEnvironmentToolProvider {
    state: AppState,
    project: ProjectRecord,
    run_id: String,
}

#[derive(Debug, Default, Deserialize)]
struct UpdateProjectEnvironmentToolArgs {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    analysis_summary: Option<String>,
    #[serde(default)]
    not_runnable_reason: Option<String>,
    #[serde(default)]
    detected_stack: Option<Value>,
    #[serde(default)]
    required_services: Option<Value>,
    #[serde(default)]
    env_vars: Option<Value>,
    #[serde(default)]
    images: Vec<ProjectRuntimeEnvironmentImageInput>,
    #[serde(default)]
    last_error: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ProjectRuntimeEnvironmentImageInput {
    #[serde(default)]
    environment_key: Option<String>,
    #[serde(default)]
    environment_type: Option<String>,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    image_id: Option<String>,
    #[serde(default)]
    image_ref: Option<String>,
    #[serde(default)]
    image_provider: Option<String>,
    #[serde(default)]
    features: Option<Value>,
    #[serde(default)]
    ports: Option<Value>,
    #[serde(default)]
    env_vars: Option<Value>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

#[async_trait]
impl BuiltinToolProvider for ProjectEnvironmentToolProvider {
    fn server_name(&self) -> &str {
        PROJECT_ENVIRONMENT_MCP_SERVER_NAME
    }

    fn list_tools(&self) -> Vec<Value> {
        vec![
            json!({
                "name": "get_current_project_runtime_environment",
                "description": "Get the current project details and persisted runtime environment for this project. The project id is bound by the server.",
                "inputSchema": {
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }
            }),
            json!({
                "name": "update_current_project_runtime_environment",
                "description": "Persist the current project's runtime environment analysis, required service images, generated environment variables, or non-runnable reason. The project id is bound by the server.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "status": {
                            "type": "string",
                            "enum": ["ready", "not_runnable", "failed", "pending_configuration"]
                        },
                        "analysis_summary": {"type": "string"},
                        "not_runnable_reason": {"type": ["string", "null"]},
                        "detected_stack": {"type": "object"},
                        "required_services": {"type": "array"},
                        "env_vars": {"type": "object"},
                        "last_error": {"type": ["string", "null"]},
                        "images": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "environment_key": {"type": "string"},
                                    "environment_type": {"type": "string"},
                                    "display_name": {"type": "string"},
                                    "image_id": {"type": ["string", "null"]},
                                    "image_ref": {"type": ["string", "null"]},
                                    "image_provider": {"type": "string"},
                                    "features": {"type": "array"},
                                    "ports": {"type": "array"},
                                    "env_vars": {"type": "object"},
                                    "status": {"type": "string"},
                                    "error": {"type": ["string", "null"]}
                                },
                                "required": ["environment_key", "environment_type", "display_name", "status"],
                                "additionalProperties": false
                            }
                        }
                    },
                    "additionalProperties": false
                }
            }),
        ]
    }

    async fn call_tool(
        &self,
        name: &str,
        args: Value,
        _context: ToolCallContext,
        _on_stream_chunk: Option<ToolStreamChunkCallback>,
    ) -> Result<Value, String> {
        match name {
            "get_current_project_runtime_environment" => {
                self.get_current_project_runtime_environment().await
            }
            "update_current_project_runtime_environment" => {
                self.update_current_project_runtime_environment(args).await
            }
            other => Err(format!("unknown project environment tool: {other}")),
        }
    }
}

impl ProjectEnvironmentToolProvider {
    async fn get_current_project_runtime_environment(&self) -> Result<Value, String> {
        let environment = self
            .state
            .store
            .get_project_runtime_environment(self.project.id.as_str())
            .await?
            .unwrap_or_else(|| default_runtime_environment_for_project(&self.project, None));
        let images = self
            .state
            .store
            .list_project_runtime_environment_images(self.project.id.as_str())
            .await?;
        Ok(mcp_tool_result(
            "当前项目运行环境详情已读取。",
            json!({
                "project": self.project,
                "environment": environment,
                "images": images,
            }),
        ))
    }

    async fn update_current_project_runtime_environment(
        &self,
        args: Value,
    ) -> Result<Value, String> {
        let args: UpdateProjectEnvironmentToolArgs = serde_json::from_value(args)
            .map_err(|err| format!("invalid project environment update args: {err}"))?;
        let mut environment = self
            .state
            .store
            .get_project_runtime_environment(self.project.id.as_str())
            .await?
            .unwrap_or_else(|| default_runtime_environment_for_project(&self.project, None));

        if let Some(value) = args.analysis_summary.and_then(normalize_owned) {
            environment.analysis_summary = Some(value);
        }
        environment.not_runnable_reason = args.not_runnable_reason.and_then(normalize_owned);
        if let Some(value) = args.detected_stack {
            environment.detected_stack = ensure_object(value);
        }
        if let Some(value) = args.required_services {
            environment.required_services = ensure_array(value);
        }
        let inferred_status = if environment.not_runnable_reason.is_some() {
            ProjectRuntimeEnvironmentStatus::NotRunnable
        } else if args
            .last_error
            .as_deref()
            .map(str::trim)
            .is_some_and(|value| !value.is_empty())
        {
            ProjectRuntimeEnvironmentStatus::Failed
        } else {
            ProjectRuntimeEnvironmentStatus::Ready
        };
        environment.status = match args.status.as_deref() {
            Some(status) => parse_runtime_environment_status(status)?,
            None => inferred_status,
        };
        if environment.status == ProjectRuntimeEnvironmentStatus::NotRunnable {
            environment.required_services = empty_array();
        }
        let env_source = args.env_vars.as_ref().or(Some(&environment.env_vars));
        environment.env_vars =
            generated_environment_variables(&environment.required_services, env_source);
        environment.last_agent_run_id = Some(self.run_id.clone());
        environment.last_error = args.last_error.and_then(normalize_owned);
        environment.updated_at = now_rfc3339();

        let mut image_records = Vec::new();
        if environment.status != ProjectRuntimeEnvironmentStatus::NotRunnable {
            for (index, image) in args.images.into_iter().enumerate() {
                image_records.push(image_input_to_record(
                    self.project.id.as_str(),
                    image,
                    index,
                    environment.sandbox_provider,
                ));
            }
        }

        let environment = self
            .state
            .store
            .upsert_project_runtime_environment(&environment)
            .await?;
        let images = self
            .state
            .store
            .replace_project_runtime_environment_images(
                self.project.id.as_str(),
                image_records.as_slice(),
            )
            .await?;
        Ok(mcp_tool_result(
            "当前项目运行环境初始化结果已保存。",
            json!({
                "environment": environment,
                "images": images,
            }),
        ))
    }
}

fn image_input_to_record(
    project_id: &str,
    image: ProjectRuntimeEnvironmentImageInput,
    index: usize,
    default_provider: RuntimeEnvironmentProvider,
) -> ProjectRuntimeEnvironmentImageRecord {
    let now = now_rfc3339();
    let environment_type = image
        .environment_type
        .and_then(normalize_owned)
        .unwrap_or_else(|| "runtime".to_string());
    let environment_key = image
        .environment_key
        .and_then(normalize_owned)
        .unwrap_or_else(|| format!("{}_{}", environment_type, index + 1));
    let display_name = image
        .display_name
        .and_then(normalize_owned)
        .unwrap_or_else(|| environment_key.clone());
    let error = image.error.and_then(normalize_owned);
    let status = image
        .status
        .and_then(normalize_owned)
        .unwrap_or_else(|| if error.is_some() { "failed" } else { "ready" }.to_string());
    ProjectRuntimeEnvironmentImageRecord {
        id: format!("project_env_image_{}", Uuid::new_v4()),
        project_id: project_id.to_string(),
        environment_key,
        environment_type,
        display_name,
        image_id: image.image_id.and_then(normalize_owned),
        image_ref: image.image_ref.and_then(normalize_owned),
        image_provider: image
            .image_provider
            .as_deref()
            .map(parse_runtime_environment_provider)
            .unwrap_or(default_provider),
        features: image.features.map(ensure_array).unwrap_or_else(empty_array),
        ports: image.ports.map(ensure_array).unwrap_or_else(empty_array),
        env_vars: image
            .env_vars
            .map(ensure_object)
            .unwrap_or_else(empty_object),
        status,
        error,
        created_at: now.clone(),
        updated_at: now,
    }
}

fn parse_runtime_environment_status(
    value: &str,
) -> Result<ProjectRuntimeEnvironmentStatus, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "disabled" => Ok(ProjectRuntimeEnvironmentStatus::Disabled),
        "pending_configuration" | "pending-configuration" => {
            Ok(ProjectRuntimeEnvironmentStatus::PendingConfiguration)
        }
        "pending" => Ok(ProjectRuntimeEnvironmentStatus::Pending),
        "analyzing" => Ok(ProjectRuntimeEnvironmentStatus::Analyzing),
        "ready" => Ok(ProjectRuntimeEnvironmentStatus::Ready),
        "not_runnable" | "not-runnable" => Ok(ProjectRuntimeEnvironmentStatus::NotRunnable),
        "failed" => Ok(ProjectRuntimeEnvironmentStatus::Failed),
        other => Err(format!(
            "unsupported project runtime environment status: {other}"
        )),
    }
}

fn parse_runtime_environment_provider(value: &str) -> RuntimeEnvironmentProvider {
    match value.trim().to_ascii_lowercase().as_str() {
        "local_connector" | "local" => RuntimeEnvironmentProvider::LocalConnector,
        "harness" => RuntimeEnvironmentProvider::Harness,
        "cloud_sandbox_manager" | "cloud" | "sandbox_manager" => {
            RuntimeEnvironmentProvider::CloudSandboxManager
        }
        _ => RuntimeEnvironmentProvider::None,
    }
}

fn ensure_array(value: Value) -> Value {
    if value.is_array() {
        value
    } else {
        empty_array()
    }
}

fn ensure_object(value: Value) -> Value {
    if value.is_object() {
        value
    } else {
        empty_object()
    }
}

fn mcp_tool_result(message: impl Into<String>, structured: Value) -> Value {
    let message = message.into();
    let text = serde_json::to_string_pretty(&structured).unwrap_or_else(|_| message.clone());
    json!({
        "content": [{
            "type": "text",
            "text": format!("{message}\n{text}")
        }],
        "_structured_result": structured
    })
}

#[derive(Debug)]
struct LocalProjectInspection {
    detected_stack: Value,
    required_services: Value,
    manifest_context: Vec<ManifestContextFile>,
}

#[derive(Debug, Clone, Serialize)]
struct ManifestContextFile {
    path: String,
    content_preview: String,
}

struct ProjectAgentMemory {
    composer: MemoryContextComposer,
    writer: chatos_ai_runtime::MemoryEngineRecordWriter,
    scope: MemoryScope,
    conversation_id: String,
}

async fn build_project_agent_memory(
    config: &AppConfig,
    owner_user_id: &str,
    project_id: &str,
    user_access_token: Option<&str>,
) -> Result<ProjectAgentMemory, String> {
    let base_url = config.memory_engine_base_url.trim();
    let source_id = config.memory_engine_source_id.trim();
    if base_url.is_empty() || source_id.is_empty() {
        return Err(
            "PROJECT_SERVICE_MEMORY_ENGINE_BASE_URL and PROJECT_SERVICE_MEMORY_ENGINE_SOURCE_ID are required"
                .to_string(),
        );
    }
    let thread_id = format!("project_environment:{project_id}");
    ensure_project_agent_memory_source(config).await?;
    let client = build_memory_engine_client(config, user_access_token)?;
    ensure_project_agent_memory_thread(&client, owner_user_id, project_id, &thread_id).await?;
    let composer = MemoryContextComposer::from_client(client.clone());
    let writer = chatos_ai_runtime::MemoryEngineRecordWriter::from_client(
        client,
        MemoryRecordScope::message_thread(owner_user_id.to_string(), thread_id.clone()),
    );
    Ok(ProjectAgentMemory {
        composer,
        writer,
        scope: MemoryScope::thread(
            owner_user_id.to_string(),
            source_id.to_string(),
            thread_id.clone(),
        )
        .with_subject_id(project_id.to_string()),
        conversation_id: thread_id,
    })
}

async fn ensure_project_agent_memory_source(config: &AppConfig) -> Result<(), String> {
    let base_url = config.memory_engine_base_url.trim();
    if base_url.is_empty() {
        return Err("PROJECT_SERVICE_MEMORY_ENGINE_BASE_URL is required".to_string());
    }
    let source_id = config.memory_engine_source_id.trim();
    if source_id.is_empty() {
        return Err("PROJECT_SERVICE_MEMORY_ENGINE_SOURCE_ID is required".to_string());
    }
    let Some(operator_token) = config
        .memory_engine_operator_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Err("PROJECT_SERVICE_MEMORY_ENGINE_OPERATOR_TOKEN is required to register project management agent memory source".to_string());
    };
    let client = memory_engine_sdk::MemoryEngineClient::new_platform(
        base_url.to_string(),
        config.memory_engine_request_timeout,
    )?
    .with_operator_token(operator_token.to_string());
    client
        .upsert_source(
            source_id,
            &memory_engine_sdk::UpsertSourceRequest {
                tenant_id: None,
                source_type: "project_management_agent".to_string(),
                name: "Project Management Agent".to_string(),
                description: Some(
                    "Project runtime environment initialization agent managed by project_management_service."
                        .to_string(),
                ),
                config: Some(json!({
                    "platform_managed": true,
                    "owner_service": "project_management_service",
                    "capabilities": [
                        "threads",
                        "records",
                        "context_compose",
                        "project_runtime_environment"
                    ],
                })),
                sdk_enabled: Some(true),
                status: Some("active".to_string()),
            },
        )
        .await?;
    Ok(())
}

async fn ensure_project_agent_memory_thread(
    client: &memory_engine_sdk::MemoryEngineClient,
    owner_user_id: &str,
    project_id: &str,
    thread_id: &str,
) -> Result<(), String> {
    client
        .upsert_thread(
            thread_id,
            &memory_engine_sdk::SdkUpsertThreadRequest {
                tenant_id: owner_user_id.to_string(),
                subject_id: project_id.to_string(),
                thread_type: "project_environment_agent".to_string(),
                external_thread_id: Some(project_id.to_string()),
                title: Some(format!("Project environment agent: {project_id}")),
                labels: Some(vec![
                    "project_management_agent".to_string(),
                    "project_environment".to_string(),
                    format!("project:{project_id}"),
                ]),
                metadata: Some(json!({
                    "owner_service": "project_management_service",
                    "agent": "project_management_environment_agent",
                    "project_id": project_id,
                })),
                status: Some("active".to_string()),
                created_at: None,
                updated_at: None,
                archived_at: None,
            },
        )
        .await?;
    Ok(())
}

fn build_memory_engine_client(
    config: &AppConfig,
    user_access_token: Option<&str>,
) -> Result<memory_engine_sdk::MemoryEngineClient, String> {
    let base_url = config.memory_engine_base_url.trim();
    if base_url.is_empty() {
        return Err("PROJECT_SERVICE_MEMORY_ENGINE_BASE_URL is required".to_string());
    }
    let source_id = config.memory_engine_source_id.trim();
    if source_id.is_empty() {
        return Err("PROJECT_SERVICE_MEMORY_ENGINE_SOURCE_ID is required".to_string());
    }
    let mut client = memory_engine_sdk::MemoryEngineClient::new_direct(
        base_url.to_string(),
        config.memory_engine_request_timeout,
        source_id.to_string(),
    )?;
    if let Some(access_token) = user_access_token
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        client = client.with_bearer_token(access_token.to_string());
    } else if let Some(operator_token) = config
        .memory_engine_operator_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        client = client.with_operator_token(operator_token.to_string());
    } else {
        return Err(
            "Memory Engine client requires a user access token or PROJECT_SERVICE_MEMORY_ENGINE_OPERATOR_TOKEN"
                .to_string(),
        );
    }
    Ok(client)
}

fn inspect_local_project(project: &ProjectRecord) -> Option<LocalProjectInspection> {
    if !matches!(project.source_type, ProjectSourceType::Local) {
        return None;
    }
    let root_path = project.root_path.as_deref()?.trim();
    if root_path.starts_with(LOCAL_CONNECTOR_ROOT_PREFIX) {
        return None;
    }
    let root = Path::new(root_path);
    if !root.is_dir() {
        return None;
    }
    let entries = fs::read_dir(root).ok()?;
    let names = entries
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect::<Vec<_>>();
    let mut languages = Vec::new();
    let mut manifests = Vec::new();
    push_marker(
        &names,
        "package.json",
        &mut manifests,
        "node",
        &mut languages,
    );
    push_marker(
        &names,
        "pnpm-lock.yaml",
        &mut manifests,
        "node",
        &mut languages,
    );
    push_marker(&names, "Cargo.toml", &mut manifests, "rust", &mut languages);
    push_marker(
        &names,
        "pyproject.toml",
        &mut manifests,
        "python",
        &mut languages,
    );
    push_marker(
        &names,
        "requirements.txt",
        &mut manifests,
        "python",
        &mut languages,
    );
    push_marker(&names, "go.mod", &mut manifests, "go", &mut languages);
    push_marker(&names, "pom.xml", &mut manifests, "java", &mut languages);
    push_marker(
        &names,
        "build.gradle",
        &mut manifests,
        "java",
        &mut languages,
    );
    push_marker(
        &names,
        "build.gradle.kts",
        &mut manifests,
        "java",
        &mut languages,
    );

    let compose = ["docker-compose.yml", "docker-compose.yaml", "compose.yml"]
        .iter()
        .find_map(|name| {
            let path = root.join(name);
            path.is_file().then_some((name.to_string(), path))
        });
    let mut required_services = Vec::new();
    if let Some((manifest, path)) = compose {
        manifests.push(manifest);
        if let Ok(content) = fs::read_to_string(path) {
            for (service_type, aliases) in [
                ("redis", &["redis"] as &[_]),
                ("postgres", &["postgres", "postgresql"]),
                ("mysql", &["mysql", "mariadb"]),
                ("nacos", &["nacos"]),
                ("mongodb", &["mongo", "mongodb"]),
                ("rabbitmq", &["rabbitmq"]),
            ] {
                if aliases
                    .iter()
                    .any(|alias| content.to_ascii_lowercase().contains(alias))
                {
                    required_services.push(json!({
                        "type": service_type,
                        "source": "docker_compose"
                    }));
                }
            }
        }
    }

    languages.sort();
    languages.dedup();
    manifests.sort();
    manifests.dedup();
    Some(LocalProjectInspection {
        detected_stack: json!({
            "languages": languages,
            "manifests": manifests,
            "source": "project_management_agent_preflight"
        }),
        required_services: Value::Array(required_services),
        manifest_context: collect_manifest_context(root),
    })
}

fn collect_manifest_context(root: &Path) -> Vec<ManifestContextFile> {
    let mut files = Vec::new();
    let mut remaining = 24_000usize;
    for relative_path in [
        "package.json",
        "pnpm-lock.yaml",
        "yarn.lock",
        "Cargo.toml",
        "pyproject.toml",
        "requirements.txt",
        "go.mod",
        "pom.xml",
        "build.gradle",
        "build.gradle.kts",
        "docker-compose.yml",
        "docker-compose.yaml",
        "compose.yml",
        ".env.example",
    ] {
        if remaining == 0 {
            break;
        }
        let path = root.join(relative_path);
        if !path.is_file() {
            continue;
        }
        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };
        let preview = truncate_chars(content.as_str(), remaining.min(6_000));
        remaining = remaining.saturating_sub(preview.len());
        files.push(ManifestContextFile {
            path: relative_path.to_string(),
            content_preview: preview,
        });
    }
    files
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let mut out = value.chars().take(max_chars).collect::<String>();
    out.push_str("\n...[truncated]");
    out
}

fn generated_environment_variables(
    required_services: &Value,
    agent_env_vars: Option<&Value>,
) -> Value {
    let mut env_vars = agent_env_vars
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    for service in required_services.as_array().into_iter().flatten() {
        let service_type = service
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        match service_type.as_str() {
            "redis" => insert_secret_default(&mut env_vars, "REDIS_PASSWORD"),
            "postgres" | "postgresql" => {
                insert_text_default(&mut env_vars, "POSTGRES_USER", "app");
                insert_secret_default(&mut env_vars, "POSTGRES_PASSWORD");
                insert_text_default(&mut env_vars, "POSTGRES_DB", "app");
            }
            "mysql" | "mariadb" => {
                insert_secret_default(&mut env_vars, "MYSQL_ROOT_PASSWORD");
                insert_text_default(&mut env_vars, "MYSQL_DATABASE", "app");
                insert_text_default(&mut env_vars, "MYSQL_USER", "app");
                insert_secret_default(&mut env_vars, "MYSQL_PASSWORD");
            }
            "nacos" => {
                insert_text_default(&mut env_vars, "NACOS_USERNAME", "nacos");
                insert_secret_default(&mut env_vars, "NACOS_PASSWORD");
                insert_secret_default(&mut env_vars, "NACOS_AUTH_TOKEN");
            }
            "mongodb" | "mongo" => {
                insert_text_default(&mut env_vars, "MONGO_INITDB_ROOT_USERNAME", "app");
                insert_secret_default(&mut env_vars, "MONGO_INITDB_ROOT_PASSWORD");
            }
            "rabbitmq" => {
                insert_text_default(&mut env_vars, "RABBITMQ_DEFAULT_USER", "app");
                insert_secret_default(&mut env_vars, "RABBITMQ_DEFAULT_PASS");
            }
            _ => {}
        }
    }
    Value::Object(env_vars)
}

fn insert_text_default(env_vars: &mut serde_json::Map<String, Value>, key: &str, value: &str) {
    let should_insert = env_vars
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .is_none_or(|value| value.is_empty());
    if should_insert {
        env_vars.insert(key.to_string(), Value::String(value.to_string()));
    }
}

fn insert_secret_default(env_vars: &mut serde_json::Map<String, Value>, key: &str) {
    let should_insert = env_vars
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .is_none_or(|value| value.is_empty());
    if should_insert {
        env_vars.insert(
            key.to_string(),
            Value::String(format!("pm-{}", Uuid::new_v4().simple())),
        );
    }
}

fn push_marker(
    names: &[String],
    marker: &str,
    manifests: &mut Vec<String>,
    language: &str,
    languages: &mut Vec<String>,
) {
    if names.iter().any(|name| name == marker) {
        manifests.push(marker.to_string());
        languages.push(language.to_string());
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
