// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::time::Duration;

use serde_json::json;
use serde_json::Value;

use chatos_mcp::sandbox_images::{SANDBOX_IMAGE_PROJECT_ID_HEADER, SANDBOX_IMAGE_RUN_ID_HEADER};
use chatos_mcp_runtime::{McpExecutor, McpHttpServer};
use chatos_service_runtime::http_body::{
    read_response_json_limited, read_response_preview_text_limited_or_message,
    ERROR_BODY_PREVIEW_LIMIT_BYTES, JSON_BODY_LIMIT_BYTES,
};

use crate::config::AppConfig;
use crate::models::{ProjectRecord, RuntimeEnvironmentProvider};
use crate::state::AppState;
use crate::trace_context::InternalTraceContextExt;

use super::routing::{
    find_enabled_local_sandbox_pairing, parse_local_connector_project_root, provider_label,
    RuntimeEnvironmentPlan,
};
use super::LOCAL_SANDBOX_IMAGE_MCP_PATH;

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
    call_sandbox_image_tool(
        &server,
        "create_image",
        json!({
            "features": features,
            "custom_build_script": custom_build_script,
            "timeout_ms": 7_200_000u64
        }),
        Duration::from_secs(2 * 60 * 60),
    )
    .await
}

pub(super) async fn get_sandbox_image_catalog(
    state: &AppState,
    project: &ProjectRecord,
    provider: RuntimeEnvironmentProvider,
    user_access_token: Option<&str>,
    run_id: &str,
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
    call_sandbox_image_tool(
        &server,
        "get_image_catalog",
        json!({}),
        Duration::from_secs(90),
    )
    .await
}

async fn call_sandbox_image_tool(
    server: &McpHttpServer,
    tool_name: &str,
    arguments: Value,
    timeout: Duration,
) -> Result<Value, String> {
    let result = chatos_mcp_runtime::jsonrpc_http_call_with_client(
        server.url.as_str(),
        server.headers.as_ref(),
        "tools/call",
        json!({
            "name": tool_name,
            "arguments": arguments
        }),
        Some(timeout),
        server.http_client.as_ref(),
    )
    .await?;
    Ok(result
        .get("structured_content")
        .cloned()
        .or_else(|| result.get("_structured_result").cloned())
        .unwrap_or(result))
}

pub(super) async fn prepare_sandbox_dependency_images(
    state: &AppState,
    provider: RuntimeEnvironmentProvider,
    project_id: &str,
    run_id: &str,
    image_refs: Vec<String>,
) -> Result<Value, String> {
    if image_refs.is_empty() {
        return Ok(json!({ "images": [] }));
    }
    if provider != RuntimeEnvironmentProvider::CloudSandboxManager {
        return Ok(json!({
            "images": image_refs.into_iter().map(|image_ref| json!({
                "image_ref": image_ref,
                "status": "deferred_to_local_compose"
            })).collect::<Vec<_>>()
        }));
    }
    let client_id = "project-service";
    let client_key = state
        .config
        .sandbox_manager_client_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "PROJECT_SERVICE_SANDBOX_MANAGER_CLIENT_KEY is required".to_string())?;
    let internal_token = chatos_service_runtime::issue_internal_service_token(
        client_key,
        client_id,
        "sandbox-manager",
        "sandbox.service",
        60,
    )?;
    let request = state
        .config
        .sandbox_manager_http_client
        .post(format!(
            "{}/api/internal/sandbox-images/prepare-dependencies",
            state
                .config
                .sandbox_manager_base_url
                .trim()
                .trim_end_matches('/')
        ))
        .header("x-sandbox-caller", client_id)
        .header("x-sandbox-internal-token", internal_token)
        .header(SANDBOX_IMAGE_PROJECT_ID_HEADER, project_id)
        .header(SANDBOX_IMAGE_RUN_ID_HEADER, run_id)
        .json(&json!({
                "image_refs": image_refs,
                "project_id": project_id,
                "run_id": run_id,
        }));
    let response = request
        .with_internal_trace_context()
        .send()
        .await
        .map_err(|err| format!("准备依赖镜像失败: {err}"))?;
    let status = response.status();
    if !status.is_success() {
        let body =
            read_response_preview_text_limited_or_message(response, ERROR_BODY_PREVIEW_LIMIT_BYTES)
                .await;
        return Err(format!("Sandbox Manager 准备依赖镜像返回 {status}: {body}"));
    }
    read_response_json_limited::<Value>(response, JSON_BODY_LIMIT_BYTES)
        .await
        .map_err(|err| format!("解析依赖镜像准备响应失败: {err}"))
}

pub(super) async fn start_local_project_compose_environment(
    state: &AppState,
    project: &ProjectRecord,
    user_access_token: Option<&str>,
    project_name: &str,
    compose_yaml: &str,
    application_dockerfiles: &std::collections::BTreeMap<String, String>,
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
    let response = state
        .config
        .local_connector_http_client
        .post(format!(
            "{}/api/local/sandbox/environments/compose/up",
            facade_base.trim_end_matches('/')
        ))
        .timeout(Duration::from_secs(2 * 60 * 60))
        .bearer_auth(access_token)
        .json(&json!({
            "project_name": project_name,
            "project_relative_path": project_ref.relative_path,
            "compose_yaml": compose_yaml,
            "application_dockerfiles": application_dockerfiles,
            "env_file": env_file,
        }))
        .with_internal_trace_context()
        .send()
        .await
        .map_err(|err| format!("启动本地 Docker Compose 环境失败: {err}"))?;
    let status = response.status();
    if !status.is_success() {
        let body =
            read_response_preview_text_limited_or_message(response, ERROR_BODY_PREVIEW_LIMIT_BYTES)
                .await;
        return Err(format!(
            "Local Connector Docker Compose 返回 {status}: {}",
            body.chars().take(4096).collect::<String>()
        ));
    }
    read_response_json_limited::<Value>(response, JSON_BODY_LIMIT_BYTES)
        .await
        .map_err(|err| format!("解析 Local Connector Docker Compose 响应失败: {err}"))
}

pub(super) async fn get_local_project_compose_environment_status(
    state: &AppState,
    project: &ProjectRecord,
    user_access_token: Option<&str>,
    project_name: &str,
) -> Result<Value, String> {
    call_local_project_compose_action(
        state,
        project,
        user_access_token,
        project_name,
        "status",
        "查询",
    )
    .await
}

pub(super) async fn stop_local_project_compose_environment(
    state: &AppState,
    project: &ProjectRecord,
    user_access_token: Option<&str>,
    project_name: &str,
) -> Result<Value, String> {
    call_local_project_compose_action(
        state,
        project,
        user_access_token,
        project_name,
        "stop",
        "停止",
    )
    .await
}

pub(super) async fn restart_local_project_compose_environment(
    state: &AppState,
    project: &ProjectRecord,
    user_access_token: Option<&str>,
    project_name: &str,
) -> Result<Value, String> {
    call_local_project_compose_action(
        state,
        project,
        user_access_token,
        project_name,
        "restart",
        "重启",
    )
    .await
}

async fn call_local_project_compose_action(
    state: &AppState,
    project: &ProjectRecord,
    user_access_token: Option<&str>,
    project_name: &str,
    action: &str,
    operation_label: &str,
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
    let response = state
        .config
        .local_connector_http_client
        .post(format!(
            "{}/api/local/sandbox/environments/compose/{action}",
            facade_base.trim_end_matches('/')
        ))
        .timeout(Duration::from_secs(10 * 60))
        .bearer_auth(access_token)
        .json(&json!({
            "project_name": project_name,
            "project_relative_path": project_ref.relative_path,
        }))
        .with_internal_trace_context()
        .send()
        .await
        .map_err(|err| format!("{operation_label}本地 Docker Compose 环境失败: {err}"))?;
    let status = response.status();
    if !status.is_success() {
        let body =
            read_response_preview_text_limited_or_message(response, ERROR_BODY_PREVIEW_LIMIT_BYTES)
                .await;
        return Err(format!(
            "Local Connector Docker Compose 返回 {status}: {}",
            body.chars().take(4096).collect::<String>()
        ));
    }
    read_response_json_limited::<Value>(response, JSON_BODY_LIMIT_BYTES)
        .await
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

pub(super) fn ensure_agent_required_tools_available(
    executor: &McpExecutor,
    plan: &RuntimeEnvironmentPlan,
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
    for required in [
        "project_management_service_list_requirements",
        "project_management_service_list_project_tasks",
    ] {
        if !tool_names.iter().any(|name| name == required) {
            return Err(format!(
                "project planning read tool is unavailable: {required}"
            ));
        }
    }
    let has_image_search = tool_names
        .iter()
        .any(|name| name == "sandbox_images_search_images");
    if plan.sandbox_provider != RuntimeEnvironmentProvider::None && !has_image_search {
        return Err("sandbox image search tool is unavailable".to_string());
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
            provider_label(plan.file_provider)
        ));
    }
    Ok(())
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
        "/api/internal/sandbox-images/mcp"
    );
    Ok(Some(
        McpHttpServer::new(
            chatos_mcp::system_mcp_descriptor(
                chatos_plugin_management_sdk::SystemMcpKey::SandboxImages,
            )
            .server_name,
            url,
        )
        .with_headers(headers)
        .with_http_client(config.sandbox_manager_http_client.clone())
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
            chatos_mcp::system_mcp_descriptor(
                chatos_plugin_management_sdk::SystemMcpKey::SandboxImages,
            )
            .server_name,
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

fn required_user_access_token<'a>(
    user_access_token: Option<&'a str>,
    label: &str,
) -> Result<&'a str, String> {
    user_access_token
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{label} 需要用户访问令牌"))
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, StatusCode};
    use axum::routing::post;
    use axum::{Json, Router};

    use super::*;

    #[tokio::test]
    async fn sandbox_image_tool_uses_server_http_client() {
        async fn mcp(headers: HeaderMap, Json(request): Json<Value>) -> (StatusCode, Json<Value>) {
            if headers
                .get("x-sandbox-mtls-client")
                .and_then(|value| value.to_str().ok())
                != Some("configured")
            {
                return (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({"error": "custom client was not used"})),
                );
            }
            (
                StatusCode::OK,
                Json(json!({
                    "jsonrpc": "2.0",
                    "id": request.get("id").cloned().unwrap_or(Value::Null),
                    "result": {
                        "structured_content": {"status": "ready"}
                    }
                })),
            )
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test MCP server");
        let address = listener.local_addr().expect("test MCP address");
        let server_task = tokio::spawn(async move {
            axum::serve(listener, Router::new().route("/mcp", post(mcp)))
                .await
                .expect("serve test MCP");
        });
        let mut default_headers = reqwest::header::HeaderMap::new();
        default_headers.insert(
            "x-sandbox-mtls-client",
            reqwest::header::HeaderValue::from_static("configured"),
        );
        let client = reqwest::Client::builder()
            .default_headers(default_headers)
            .build()
            .expect("build custom client");
        let server = McpHttpServer::new("sandbox-images", format!("http://{address}/mcp"))
            .with_http_client(client);

        let result = call_sandbox_image_tool(
            &server,
            "get_image_catalog",
            json!({}),
            Duration::from_secs(5),
        )
        .await
        .expect("custom HTTP client should reach MCP server");

        assert_eq!(
            result.pointer("/status").and_then(Value::as_str),
            Some("ready")
        );
        server_task.abort();
    }
}
