// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use reqwest::{Method, StatusCode};
use serde::de::DeserializeOwned;
use serde::Deserialize;

use crate::config::McpManagementClientConfig;
use crate::dto::{McpCatalogResponse, ResolveMcpRoutesRequest, ResolveMcpRoutesResponse};
use crate::error::McpManagementClientError;

const INTERNAL_SECRET_HEADER: &str = "x-mcp-management-internal-secret";
const INTERNAL_TOKEN_HEADER: &str = "x-mcp-management-internal-token";
const CALLER_SERVICE_HEADER: &str = "x-mcp-management-caller-service";
const INTERNAL_TOKEN_AUDIENCE: &str = "mcp-management-service";
const CATALOG_READ_SCOPE: &str = "catalog.read";
const ROUTES_RESOLVE_SCOPE: &str = "routes.resolve";

#[derive(Clone)]
pub struct McpManagementClient {
    http: reqwest::Client,
    config: McpManagementClientConfig,
}

impl McpManagementClient {
    pub fn new(config: McpManagementClientConfig) -> Result<Self, McpManagementClientError> {
        reqwest::Url::parse(config.base_url.as_str())
            .map_err(|err| McpManagementClientError::InvalidBaseUrl(err.to_string()))?;
        let http = reqwest::Client::builder()
            .timeout(config.request_timeout)
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
