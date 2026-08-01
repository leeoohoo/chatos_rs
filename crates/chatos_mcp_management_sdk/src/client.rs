// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use reqwest::{Method, StatusCode};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::fmt;

use crate::config::McpManagementClientConfig;
use crate::dto::{
    CloseRuntimeSessionResponse, CreateRuntimeSessionRequest, McpCatalogResponse,
    ResolveMcpRoutesRequest, ResolveMcpRoutesResponse, RuntimeSessionResponse,
    RuntimeSessionRoutesResponse,
};
use crate::error::McpManagementClientError;

const INTERNAL_SECRET_HEADER: &str = "x-mcp-management-internal-secret";
const INTERNAL_TOKEN_HEADER: &str = "x-mcp-management-internal-token";
const CALLER_SERVICE_HEADER: &str = "x-mcp-management-caller-service";
const INTERNAL_TOKEN_AUDIENCE: &str = "mcp-management-service";
const CATALOG_READ_SCOPE: &str = "catalog.read";
const ROUTES_RESOLVE_SCOPE: &str = "routes.resolve";
const RUNTIME_SESSIONS_RESOLVE_SCOPE: &str = "runtime.sessions.resolve";
const RUNTIME_SESSIONS_READ_SCOPE: &str = "runtime.sessions.read";
const RUNTIME_SESSIONS_CLOSE_SCOPE: &str = "runtime.sessions.close";

#[derive(Clone)]
pub struct McpManagementClient {
    http: reqwest::Client,
    config: McpManagementClientConfig,
}

#[derive(Clone)]
pub struct McpManagementRuntimeSessionHandle {
    client: McpManagementClient,
    session_id: String,
}

impl fmt::Debug for McpManagementRuntimeSessionHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("McpManagementRuntimeSessionHandle")
            .field("session_id", &self.session_id)
            .finish_non_exhaustive()
    }
}

impl McpManagementRuntimeSessionHandle {
    pub fn new(client: McpManagementClient, session_id: impl Into<String>) -> Self {
        Self {
            client,
            session_id: session_id.into(),
        }
    }

    pub fn session_id(&self) -> &str {
        self.session_id.as_str()
    }

    pub async fn close(self) -> Result<CloseRuntimeSessionResponse, McpManagementClientError> {
        self.client
            .close_runtime_session(self.session_id.as_str())
            .await
    }
}

impl McpManagementClient {
    pub fn new(config: McpManagementClientConfig) -> Result<Self, McpManagementClientError> {
        let base_url = reqwest::Url::parse(config.base_url.as_str())
            .map_err(|err| McpManagementClientError::InvalidBaseUrl(err.to_string()))?;
        if !matches!(base_url.scheme(), "http" | "https") {
            return Err(McpManagementClientError::InvalidBaseUrl(
                "MCP Management base URL must use http or https".to_string(),
            ));
        }
        let http = reqwest::Client::builder()
            .timeout(config.request_timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(Self { http, config })
    }

    pub fn config(&self) -> &McpManagementClientConfig {
        &self.config
    }

    pub async fn catalog(&self) -> Result<McpCatalogResponse, McpManagementClientError> {
        let url = format!("{}/api/internal/catalog", self.config.base_url);
        let response = self
            .internal_request(Method::GET, url, CATALOG_READ_SCOPE)?
            .send()
            .await?;
        parse_response(response).await
    }

    pub async fn resolve_routes(
        &self,
        request: &ResolveMcpRoutesRequest,
    ) -> Result<ResolveMcpRoutesResponse, McpManagementClientError> {
        let url = format!("{}/api/internal/routes/resolve", self.config.base_url);
        let response = self
            .internal_request(Method::POST, url, ROUTES_RESOLVE_SCOPE)?
            .json(request)
            .send()
            .await?;
        parse_response(response).await
    }

    pub async fn resolve_runtime_session(
        &self,
        request: &CreateRuntimeSessionRequest,
    ) -> Result<RuntimeSessionResponse, McpManagementClientError> {
        let url = format!(
            "{}/api/internal/runtime/sessions/resolve",
            self.config.base_url
        );
        let response = self
            .internal_request(Method::POST, url, RUNTIME_SESSIONS_RESOLVE_SCOPE)?
            .json(request)
            .send()
            .await?;
        parse_response(response).await
    }

    pub async fn runtime_session_routes(
        &self,
        session_id: &str,
    ) -> Result<RuntimeSessionRoutesResponse, McpManagementClientError> {
        let url = format!(
            "{}/api/internal/runtime/sessions/{}/routes",
            self.config.base_url,
            urlencoding::encode(session_id.trim())
        );
        let response = self
            .internal_request(Method::GET, url, RUNTIME_SESSIONS_READ_SCOPE)?
            .send()
            .await?;
        parse_response(response).await
    }

    pub async fn close_runtime_session(
        &self,
        session_id: &str,
    ) -> Result<CloseRuntimeSessionResponse, McpManagementClientError> {
        let url = format!(
            "{}/api/internal/runtime/sessions/{}/close",
            self.config.base_url,
            urlencoding::encode(session_id.trim())
        );
        let response = self
            .internal_request(Method::POST, url, RUNTIME_SESSIONS_CLOSE_SCOPE)?
            .send()
            .await?;
        parse_response(response).await
    }

    fn internal_request(
        &self,
        method: Method,
        url: String,
        scope: &str,
    ) -> Result<reqwest::RequestBuilder, McpManagementClientError> {
        let secret = self
            .config
            .internal_api_secret
            .as_deref()
            .ok_or(McpManagementClientError::MissingInternalSecret)?;
        let token = chatos_service_runtime::issue_internal_service_token(
            secret,
            self.config.caller_service.as_str(),
            INTERNAL_TOKEN_AUDIENCE,
            scope,
            60,
        )
        .map_err(McpManagementClientError::InternalToken)?;
        Ok(self
            .http
            .request(method, url)
            .header(INTERNAL_SECRET_HEADER, secret)
            .header(INTERNAL_TOKEN_HEADER, token)
            .header(CALLER_SERVICE_HEADER, self.config.caller_service.as_str()))
    }
}

#[derive(Debug, Deserialize)]
struct ErrorResponse {
    error: Option<String>,
}

async fn parse_response<T>(response: reqwest::Response) -> Result<T, McpManagementClientError>
where
    T: DeserializeOwned,
{
    let status = response.status();
    if status.is_success() {
        return response
            .json::<T>()
            .await
            .map_err(McpManagementClientError::Transport);
    }
    let status_code = status.as_u16();
    let body = response.text().await.unwrap_or_default();
    let message = serde_json::from_str::<ErrorResponse>(body.as_str())
        .ok()
        .and_then(|value| value.error)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| default_error_message(status));
    Err(McpManagementClientError::Rejected {
        status: status_code,
        message,
    })
}

fn default_error_message(status: StatusCode) -> String {
    status
        .canonical_reason()
        .unwrap_or("MCP management request failed")
        .to_string()
}
