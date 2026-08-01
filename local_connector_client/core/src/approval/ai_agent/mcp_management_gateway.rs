// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use chatos_mcp_management_sdk::RuntimeSessionResponse;
use chatos_mcp_runtime::{McpExecutor, McpHttpServer};
use serde::Serialize;

use crate::{tracing_stdout, LocalState};

use super::super::decision_tool::ApprovalToolDecision;
use super::super::types::CommandApprovalRequest;

const GATEWAY_SERVER_NAME: &str = "mcp_management";
const DEFAULT_TOOL_TIMEOUT_MS: u64 = 180_000;

mod tool_executor;

use self::tool_executor::ApprovalMcpGatewayToolExecutor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum McpManagementExecutionMode {
    Off,
    Shadow,
    Gateway,
}

impl McpManagementExecutionMode {
    fn from_value(value: Option<&str>) -> Self {
        match value
            .map(str::trim)
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "off" | "disabled" | "false" | "0" => Self::Off,
            "gateway" | "enabled" | "on" | "true" | "1" => Self::Gateway,
            _ => Self::Shadow,
        }
    }

    pub(super) fn from_env() -> Self {
        Self::from_value(
            std::env::var("LOCAL_CONNECTOR_COMMAND_APPROVAL_MCP_MANAGEMENT_MODE")
                .ok()
                .as_deref(),
        )
    }

    pub(super) const fn uses_gateway(self) -> bool {
        matches!(self, Self::Gateway)
    }
}

pub(super) enum ApprovalMcpResolution {
    Legacy,
    Gateway(Box<ApprovalMcpGateway>),
}

pub(super) struct ApprovalMcpGateway {
    executor: ApprovalMcpGatewayToolExecutor,
    close: ApprovalMcpSessionClose,
    provider_skills_prompt: Option<String>,
}

impl ApprovalMcpGateway {
    pub(super) fn executor(&self) -> ApprovalMcpGatewayToolExecutor {
        self.executor.clone()
    }

    pub(super) fn provider_skills_prompt(&self) -> Option<String> {
        self.provider_skills_prompt.clone()
    }

    pub(super) async fn close(self) {
        if let Err(error) = self.close.close().await {
            tracing_stdout(
                format!("close command approval MCP Management runtime session failed: {error:#}")
                    .as_str(),
            );
        }
    }
}

#[derive(Clone)]
struct ApprovalMcpSessionClose {
    http: reqwest::Client,
    service_base_url: String,
    access_token: String,
    session_id: String,
    project_id: String,
    run_id: String,
}

impl ApprovalMcpSessionClose {
    async fn close(&self) -> Result<()> {
        let url = format!(
            "{}/api/local-connectors/mcp-management/command-approval/sessions/{}/close",
            self.service_base_url,
            urlencoding::encode(self.session_id.as_str())
        );
        let response = self
            .http
            .post(url)
            .bearer_auth(self.access_token.as_str())
            .json(&CloseRuntimeSessionRequest {
                project_id: self.project_id.as_str(),
                run_id: self.run_id.as_str(),
            })
            .send()
            .await
            .context("close command approval MCP Management runtime session")?;
        ensure_success(
            response,
            "close command approval MCP Management runtime session",
        )
        .await
    }
}

#[derive(Serialize)]
struct ResolveRuntimeSessionRequest<'a> {
    project_id: &'a str,
    device_id: &'a str,
    workspace_id: &'a str,
    run_id: &'a str,
    model_config_id: &'a str,
}

#[derive(Serialize)]
struct CloseRuntimeSessionRequest<'a> {
    project_id: &'a str,
    run_id: &'a str,
}

pub(super) async fn resolve_approval_mcp(
    state: &LocalState,
    request: &CommandApprovalRequest,
    run_id: &str,
    model_config_id: &str,
    decision: Arc<Mutex<Option<ApprovalToolDecision>>>,
    mode: McpManagementExecutionMode,
) -> Result<ApprovalMcpResolution> {
    if mode == McpManagementExecutionMode::Off {
        return Ok(ApprovalMcpResolution::Legacy);
    }
    let Some(project_id) = request
        .project_key
        .project_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return shadow_or_error(
            mode,
            "command approval MCP Management requires a cloud project binding",
        );
    };
    let auth = state.auth.as_ref().ok_or_else(|| {
        anyhow!("Local Connector login is required for MCP Management command approval")
    })?;
    let service_base_url = auth.cloud_base_url.trim().trim_end_matches('/').to_string();
    let access_token = auth.access_token.trim().to_string();
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .context("build command approval MCP Management client")?;
    let response = http
        .post(format!(
            "{service_base_url}/api/local-connectors/mcp-management/command-approval/sessions/resolve"
        ))
        .bearer_auth(access_token.as_str())
        .json(&ResolveRuntimeSessionRequest {
            project_id,
            device_id: request.project_key.device_id.as_str(),
            workspace_id: request.project_key.workspace_id.as_str(),
            run_id,
            model_config_id,
        })
        .send()
        .await;
    let session = match response {
        Ok(response) => match decode_session_response(response).await {
            Ok(session) => session,
            Err(error) if mode == McpManagementExecutionMode::Shadow => {
                tracing_stdout(
                    format!(
                        "MCP Management shadow session resolution failed; legacy command approval tools remain active: {error:#}"
                    )
                    .as_str(),
                );
                return Ok(ApprovalMcpResolution::Legacy);
            }
            Err(error) => return Err(error),
        },
        Err(error) if mode == McpManagementExecutionMode::Shadow => {
            tracing_stdout(
                format!(
                    "MCP Management shadow session request failed; legacy command approval tools remain active: {error}"
                )
                .as_str(),
            );
            return Ok(ApprovalMcpResolution::Legacy);
        }
        Err(error) => {
            return Err(anyhow!(
                "resolve command approval MCP Management runtime session failed: {error}"
            ));
        }
    };
    tracing_stdout(
        format!(
            "command approval MCP Management session resolved: session_id={}, route_revision={}, configured_mcps={}, exposed_tools={}, mode={mode:?}",
            session.session_id,
            session.route_revision,
            session.configured_mcp_count,
            session.exposed_tool_count,
        )
        .as_str(),
    );
    let close = ApprovalMcpSessionClose {
        http,
        service_base_url,
        access_token,
        session_id: session.session_id.clone(),
        project_id: project_id.to_string(),
        run_id: run_id.to_string(),
    };
    if mode == McpManagementExecutionMode::Shadow {
        if let Err(error) = close.close().await {
            tracing_stdout(
                format!("close command approval MCP Management shadow session failed: {error:#}")
                    .as_str(),
            );
        }
        return Ok(ApprovalMcpResolution::Legacy);
    }

    let server = match gateway_server(&session, close.service_base_url.as_str(), tool_timeout()) {
        Ok(server) => server,
        Err(error) => {
            let _ = close.close().await;
            return Err(error);
        }
    };
    let executor = match McpExecutor::builder()
        .with_http_server(server)
        .build_initialized()
        .await
    {
        Ok(executor) => executor,
        Err(error) => {
            let _ = close.close().await;
            return Err(anyhow!(
                "initialize command approval MCP Management tools failed: {error}"
            ));
        }
    };
    let executor = match ApprovalMcpGatewayToolExecutor::new(executor, decision) {
        Ok(executor) => executor,
        Err(error) => {
            let _ = close.close().await;
            return Err(error);
        }
    };
    let provider_skills_prompt = session.provider_skills_prompt;
    Ok(ApprovalMcpResolution::Gateway(Box::new(
        ApprovalMcpGateway {
            executor,
            close,
            provider_skills_prompt,
        },
    )))
}

fn shadow_or_error(
    mode: McpManagementExecutionMode,
    message: &str,
) -> Result<ApprovalMcpResolution> {
    if mode == McpManagementExecutionMode::Shadow {
        tracing_stdout(format!("MCP Management shadow session skipped: {message}").as_str());
        Ok(ApprovalMcpResolution::Legacy)
    } else {
        Err(anyhow!(message.to_string()))
    }
}

async fn decode_session_response(response: reqwest::Response) -> Result<RuntimeSessionResponse> {
    let status = response.status();
    if status.is_success() {
        return response
            .json::<RuntimeSessionResponse>()
            .await
            .context("decode command approval MCP Management runtime session");
    }
    let detail = response.text().await.unwrap_or_default();
    Err(anyhow!(
        "resolve command approval MCP Management runtime session was rejected: {status}: {detail}"
    ))
}

async fn ensure_success(response: reqwest::Response, action: &str) -> Result<()> {
    let status = response.status();
    if status.is_success() {
        return Ok(());
    }
    let detail = response.text().await.unwrap_or_default();
    Err(anyhow!("{action} was rejected: {status}: {detail}"))
}

fn gateway_server(
    session: &RuntimeSessionResponse,
    service_base_url: &str,
    timeout: Duration,
) -> Result<McpHttpServer> {
    if session.runtime_token.trim().is_empty() {
        return Err(anyhow!(
            "MCP Management runtime session returned an empty runtime token"
        ));
    }
    let mcp_server_url = resolve_mcp_server_url(session.mcp_server_url.as_str(), service_base_url)?;
    Ok(McpHttpServer::new(GATEWAY_SERVER_NAME, mcp_server_url)
        .with_headers(HashMap::from([(
            "authorization".to_string(),
            format!("Bearer {}", session.runtime_token),
        )]))
        .with_timeout(timeout)
        .with_preserved_tool_names()
        .with_fail_on_unavailable())
}

fn resolve_mcp_server_url(value: &str, service_base_url: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(anyhow!(
            "MCP Management runtime session returned an empty MCP URL"
        ));
    }
    if let Ok(url) = reqwest::Url::parse(value) {
        if matches!(url.scheme(), "http" | "https") {
            return Ok(url.to_string());
        }
        return Err(anyhow!("MCP Management MCP URL must use http or https"));
    }
    if !value.starts_with('/') || value.starts_with("//") {
        return Err(anyhow!(
            "MCP Management MCP URL must be absolute or a same-origin path"
        ));
    }
    let base = reqwest::Url::parse(service_base_url)
        .context("Local Connector cloud service base URL is invalid")?;
    if !matches!(base.scheme(), "http" | "https") {
        return Err(anyhow!(
            "Local Connector cloud service base URL must use http or https"
        ));
    }
    base.join(value)
        .map(|url| url.to_string())
        .context("resolve MCP Management facade URL")
}

fn tool_timeout() -> Duration {
    Duration::from_millis(
        std::env::var("LOCAL_CONNECTOR_COMMAND_APPROVAL_MCP_MANAGEMENT_TOOL_TIMEOUT_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(DEFAULT_TOOL_TIMEOUT_MS)
            .clamp(1_000, 2 * 60 * 60 * 1_000),
    )
}

#[cfg(test)]
mod tests;
