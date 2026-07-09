// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use serde_json::Value;

use chatos_mcp_runtime::{
    BuiltinMcpKind, BuiltinMcpServerOptions, McpBuiltinServer, McpExecutor, McpHttpServer,
};
use chatos_mcp_service::{
    builtin_kind_header_value, HARNESS_CODE_ENABLED_BUILTIN_KINDS_HEADER,
    LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER,
};

use crate::config::AppConfig;
use crate::models::{
    ProjectRecord, ProjectRuntimeEnvironmentRecord, ProjectSourceType, RuntimeEnvironmentProvider,
};
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

pub(super) fn ensure_agent_required_tools_available(
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
