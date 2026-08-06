// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_management_sdk::ProjectExecutionContext;
use reqwest::{Method, StatusCode};
use serde::Deserialize;

use crate::trace_context::InternalTraceContextExt;

const CALLER_SERVICE: &str = "mcp-management-service";
const TOKEN_AUDIENCE: &str = "project-service";
const EXECUTION_CONTEXT_SCOPE: &str = "project.execution_context.read";

#[derive(Clone)]
pub struct ProjectContextClient {
    http: reqwest::Client,
    base_url: String,
    internal_api_secret: Option<String>,
}

impl ProjectContextClient {
    pub fn new(
        http: reqwest::Client,
        base_url: impl Into<String>,
        internal_api_secret: Option<String>,
    ) -> Result<Self, String> {
        let base_url = base_url.into();
        reqwest::Url::parse(base_url.as_str())
            .map_err(|err| format!("project service base URL is invalid: {err}"))?;
        Ok(Self {
            http,
            base_url: base_url.trim().trim_end_matches('/').to_string(),
            internal_api_secret,
        })
    }

    pub async fn resolve(
        &self,
        project_id: &str,
        owner_user_id: &str,
    ) -> Result<ProjectExecutionContext, String> {
        let secret = self
            .internal_api_secret
            .as_deref()
            .ok_or_else(|| "project context internal API secret is not configured".to_string())?;
        let token = chatos_service_runtime::issue_internal_service_token(
            secret,
            CALLER_SERVICE,
            TOKEN_AUDIENCE,
            EXECUTION_CONTEXT_SCOPE,
            60,
        )?;
        let url = format!(
            "{}/api/internal/projects/{}/execution-context",
            self.base_url,
            urlencoding::encode(project_id.trim())
        );
        let response = self
            .http
            .request(Method::GET, url)
            .query(&[("owner_user_id", owner_user_id.trim())])
            .header("x-project-service-caller", CALLER_SERVICE)
            .header("x-project-service-internal-token", token)
            .with_internal_trace_context()
            .send()
            .await
            .map_err(|err| format!("project context request failed: {err}"))?;
        parse_response(response).await
    }
}

#[derive(Deserialize)]
struct ErrorResponse {
    error: Option<String>,
}

async fn parse_response(response: reqwest::Response) -> Result<ProjectExecutionContext, String> {
    let status = response.status();
    if status.is_success() {
        return response
            .json::<ProjectExecutionContext>()
            .await
            .map_err(|err| format!("parse project execution context failed: {err}"));
    }
    let body = response.text().await.unwrap_or_default();
    let message = serde_json::from_str::<ErrorResponse>(body.as_str())
        .ok()
        .and_then(|value| value.error)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| default_error_message(status));
    Err(format!(
        "project execution context was rejected with status {}: {message}",
        status.as_u16()
    ))
}

fn default_error_message(status: StatusCode) -> String {
    status
        .canonical_reason()
        .unwrap_or("project execution context request failed")
        .to_string()
}
