// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use serde_json::json;
use serde_json::Value;

use chatos_mcp_runtime::{
    BuiltinMcpKind, BuiltinMcpServerOptions, McpBuiltinServer, McpExecutor, McpHttpServer,
};
use chatos_mcp_service::{
    builtin_kind_header_value, HARNESS_CODE_ENABLED_BUILTIN_KINDS_HEADER,
    LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER,
};
use chatos_plugin_management_sdk::{
    ResolvedAgentCapabilities, PROJECT_ENVIRONMENT_MCP_RESOURCE_ID,
};
use chatos_sandbox_image_mcp::{SANDBOX_IMAGE_PROJECT_ID_HEADER, SANDBOX_IMAGE_RUN_ID_HEADER};

use crate::config::AppConfig;
use crate::models::{ProjectRecord, ProjectSourceType, RuntimeEnvironmentProvider};
use crate::state::AppState;

use super::routing::{
    find_enabled_local_sandbox_pairing, parse_local_connector_project_root, provider_label,
    LocalConnectorProjectRef, RoutingPlan,
};
use super::tool_provider::ProjectEnvironmentToolProvider;
use super::{
    CLOUD_SANDBOX_IMAGE_MCP_PATH, LOCAL_CONNECTOR_ROOT_PREFIX, LOCAL_SANDBOX_IMAGE_MCP_PATH,
    PROJECT_ENVIRONMENT_MCP_SERVER_NAME, SANDBOX_IMAGE_MCP_SERVER_NAME,
};

pub(super) async fn build_project_environment_mcp_executor(
    state: &AppState,
    project: &ProjectRecord,
    routing: &RoutingPlan,
    user_access_token: Option<&str>,
    run_id: &str,
    capability_policy: &ResolvedAgentCapabilities,
) -> Result<McpExecutor, String> {
    let mut builder = McpExecutor::builder();
    if capability_allows_mcp(capability_policy, PROJECT_ENVIRONMENT_MCP_RESOURCE_ID) {
        builder = builder
            .with_builtin_server(project_environment_builtin_server())
            .with_builtin_provider(ProjectEnvironmentToolProvider {
                state: state.clone(),
                project: project.clone(),
                run_id: run_id.to_string(),
            });
    }

    if capability_allows_builtin(capability_policy, BuiltinMcpKind::CodeMaintainerRead) {
        match routing.file_provider {
            RuntimeEnvironmentProvider::Harness => {
                builder =
                    builder.with_http_server(harness_file_mcp_server(&state.config, project)?);
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
    }

    builder.build_initialized().await
}

fn capability_allows_mcp(policy: &ResolvedAgentCapabilities, resource_id: &str) -> bool {
    policy
        .mcps
        .iter()
        .any(|item| item.resource.id == resource_id && item.available)
}

pub(super) async fn create_sandbox_image_from_plan(
    state: &AppState,
    project: &ProjectRecord,
    provider: RuntimeEnvironmentProvider,
    user_access_token: Option<&str>,
    run_id: &str,
    features: Vec<String>,
    custom_build_script: Option<String>,
) -> Result<Value, String> {
    let server = match provider {
        RuntimeEnvironmentProvider::LocalConnector => {
            local_connector_sandbox_image_mcp_server(state, project, user_access_token, run_id)
                .await?
        }
        RuntimeEnvironmentProvider::CloudSandboxManager => {
            cloud_sandbox_image_mcp_server(&state.config, provider, project.id.as_str(), run_id)?
        }
        RuntimeEnvironmentProvider::None | RuntimeEnvironmentProvider::Harness => None,
    }
    .ok_or_else(|| "当前项目没有可用的沙箱镜像 Provider".to_string())?;
    let result = chatos_mcp_runtime::jsonrpc_http_call(
        server.url.as_str(),
        server.headers.as_ref(),
        "tools/call",
        json!({
            "name": "create_image",
            "arguments": {
                "features": features,
                "custom_build_script": custom_build_script,
                "timeout_ms": 7_200_000u64
            }
        }),
        Some(Duration::from_secs(2 * 60 * 60)),
    )
    .await?;
    Ok(result
        .get("structured_content")
        .cloned()
        .or_else(|| result.get("_structured_result").cloned())
        .unwrap_or(result))
}

pub(super) async fn start_local_project_compose_environment(
    state: &AppState,
    project: &ProjectRecord,
    user_access_token: Option<&str>,
    project_name: &str,
    compose_yaml: &str,
    application_dockerfile: &str,
    env_file: &str,
) -> Result<Value, String> {
    let access_token =
        required_user_access_token(user_access_token, "Local Connector Docker Compose")?;
    let project_ref = project
        .root_path
        .as_deref()
        .and_then(parse_local_connector_project_root)
        .ok_or_else(|| "当前项目不是有效的 Local Connector 本地项目".to_string())?;
    let pairing =
        find_enabled_local_sandbox_pairing(&state.config, Some(access_token), Some(&project_ref))
            .await?
            .ok_or_else(|| "没有找到已启用的 Local Connector 沙箱配对".to_string())?;
    let facade_base = local_connector_facade_base(state, &pairing)?;
    let response = reqwest::Client::new()
        .post(format!(
            "{}/api/local/sandbox/environments/compose/up",
            facade_base.trim_end_matches('/')
        ))
        .bearer_auth(access_token)
        .json(&json!({
            "project_name": project_name,
            "project_relative_path": project_ref.relative_path,
            "compose_yaml": compose_yaml,
            "application_dockerfile": application_dockerfile,
            "env_file": env_file,
        }))
        .timeout(Duration::from_secs(2 * 60 * 60))
        .send()
        .await
        .map_err(|err| format!("启动本地 Docker Compose 环境失败: {err}"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|err| format!("读取 Local Connector Docker Compose 响应失败: {err}"))?;
    if !status.is_success() {
        return Err(format!(
            "Local Connector Docker Compose 返回 {status}: {}",
            body.chars().take(4096).collect::<String>()
        ));
    }
    serde_json::from_str(body.as_str())
        .map_err(|err| format!("解析 Local Connector Docker Compose 响应失败: {err}"))
}

fn local_connector_facade_base(
    state: &AppState,
    pairing: &super::routing::LocalConnectorSandboxPairing,
) -> Result<String, String> {
    pairing
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
        .ok_or_else(|| "Local Connector 沙箱配对缺少 facade_base_url".to_string())
}

fn capability_allows_builtin(policy: &ResolvedAgentCapabilities, kind: BuiltinMcpKind) -> bool {
    policy.mcps.iter().any(|item| {
        item.available
            && item.resource.runtime.kind == "builtin"
            && item.resource.runtime.builtin_kind.as_deref() == Some(kind.kind_name())
    })
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

pub(super) fn ensure_agent_required_tools_available(
    executor: &McpExecutor,
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
    Ok(())
}

fn harness_file_mcp_server(
    config: &AppConfig,
    project: &ProjectRecord,
) -> Result<McpHttpServer, String> {
    let internal_secret = config
        .internal_api_secrets
        .get("project-service")
        .map(String::as_str)
        .or(config.sync_secret.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            "PROJECT_SERVICE_SELF_INTERNAL_API_SECRET is required for Harness MCP".to_string()
        })?;
    let base = project_service_base_url(config);
    let mut headers = HashMap::new();
    headers.insert(
        "x-project-service-sync-secret".to_string(),
        internal_secret.to_string(),
    );
    headers.insert(
        "x-project-service-caller".to_string(),
        "project-service".to_string(),
    );
    headers.insert(
        "x-project-service-internal-scope".to_string(),
        "project.harness".to_string(),
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
    project_id: &str,
    run_id: &str,
) -> Result<Option<McpHttpServer>, String> {
    if provider != RuntimeEnvironmentProvider::CloudSandboxManager {
        return Ok(None);
    }
    let client_id = "project-service";
    let client_key = config
        .sandbox_manager_client_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "PROJECT_SERVICE_SANDBOX_MANAGER_CLIENT_KEY is required".to_string())?;
    let mut headers = HashMap::new();
    headers.insert("x-sandbox-caller".to_string(), client_id.to_string());
    headers.insert("x-sandbox-client-key".to_string(), client_key.to_string());
    headers.insert(
        "x-sandbox-internal-scope".to_string(),
        "sandbox.service".to_string(),
    );
    headers.insert(
        SANDBOX_IMAGE_PROJECT_ID_HEADER.to_string(),
        project_id.to_string(),
    );
    headers.insert(SANDBOX_IMAGE_RUN_ID_HEADER.to_string(), run_id.to_string());
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
    run_id: &str,
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
    let facade_base = local_connector_facade_base(state, &pairing)?;
    let mut headers = HashMap::new();
    headers.insert(
        "authorization".to_string(),
        format!("Bearer {access_token}"),
    );
    headers.insert(
        SANDBOX_IMAGE_PROJECT_ID_HEADER.to_string(),
        project.id.clone(),
    );
    headers.insert(SANDBOX_IMAGE_RUN_ID_HEADER.to_string(), run_id.to_string());
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
