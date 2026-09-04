// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use reqwest::{Method, StatusCode};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::fmt;
use std::fs;

use crate::config::McpManagementClientConfig;
use crate::dto::{
    CloseRuntimeSessionResponse, CreateRuntimeSessionRequest, McpCatalogResponse,
    ResolveMcpRoutesRequest, ResolveMcpRoutesResponse, RuntimeInvocationResponse,
    RuntimeSessionResponse, RuntimeSessionRoutesResponse,
};
use crate::error::McpManagementClientError;

const INTERNAL_TOKEN_HEADER: &str = "x-mcp-management-internal-token";
const CALLER_SERVICE_HEADER: &str = "x-mcp-management-caller-service";
const INTERNAL_TOKEN_AUDIENCE: &str = "mcp-management-service";
const CATALOG_READ_SCOPE: &str = "catalog.read";
const ROUTES_RESOLVE_SCOPE: &str = "routes.resolve";
const RUNTIME_SESSIONS_RESOLVE_SCOPE: &str = "runtime.sessions.resolve";
const RUNTIME_SESSIONS_READ_SCOPE: &str = "runtime.sessions.read";
const RUNTIME_SESSIONS_CLOSE_SCOPE: &str = "runtime.sessions.close";
const RUNTIME_SESSION_TERMINAL_STATUS_HEADER: &str = "x-mcp-management-terminal-status";
const RUNTIME_INVOCATIONS_READ_SCOPE: &str = "runtime.invocations.read";
const RUNTIME_INVOCATIONS_RESOLVE_USER_SCOPE: &str = "runtime.invocations.resolve_user";

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

    pub async fn routes(&self) -> Result<RuntimeSessionRoutesResponse, McpManagementClientError> {
        self.client
            .runtime_session_routes(self.session_id.as_str())
            .await
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
        if base_url.scheme() != "https" {
            return Err(McpManagementClientError::InvalidBaseUrl(
                "MCP Management internal base URL must use https because mTLS is mandatory"
                    .to_string(),
            ));
        }
        let ca_pem = fs::read(config.mtls_ca_cert_path.as_path()).map_err(|err| {
            McpManagementClientError::InvalidMtlsConfiguration(format!(
                "read CA certificate {} failed: {err}",
                config.mtls_ca_cert_path.display()
            ))
        })?;
        let identity_pem = fs::read(config.mtls_client_identity_path.as_path()).map_err(|err| {
            McpManagementClientError::InvalidMtlsConfiguration(format!(
                "read client identity {} failed: {err}",
                config.mtls_client_identity_path.display()
            ))
        })?;
        let ca = reqwest::Certificate::from_pem(ca_pem.as_slice()).map_err(|err| {
            McpManagementClientError::InvalidMtlsConfiguration(format!(
                "parse CA certificate failed: {err}"
            ))
        })?;
        let identity = reqwest::Identity::from_pem(identity_pem.as_slice()).map_err(|err| {
            McpManagementClientError::InvalidMtlsConfiguration(format!(
                "parse client identity failed: {err}"
            ))
        })?;
        let http = reqwest::Client::builder()
            .use_rustls_tls()
            .https_only(true)
            .timeout(config.request_timeout)
            .redirect(reqwest::redirect::Policy::none())
            .add_root_certificate(ca)
            .identity(identity)
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
            .runtime_session_request(Method::POST, url, RUNTIME_SESSIONS_RESOLVE_SCOPE)?
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
        self.close_runtime_session_with_status(session_id, "closed")
            .await
    }

    pub async fn close_runtime_session_with_status(
        &self,
        session_id: &str,
        terminal_status: &str,
    ) -> Result<CloseRuntimeSessionResponse, McpManagementClientError> {
        let url = format!(
            "{}/api/internal/runtime/sessions/{}/close",
            self.config.base_url,
            urlencoding::encode(session_id.trim())
        );
        let response = self
            .internal_request(Method::POST, url, RUNTIME_SESSIONS_CLOSE_SCOPE)?
            .header(
                RUNTIME_SESSION_TERMINAL_STATUS_HEADER,
                terminal_status.trim(),
            )
            .send()
            .await?;
        parse_response(response).await
    }

    pub async fn runtime_invocation(
        &self,
        invocation_id: &str,
    ) -> Result<RuntimeInvocationResponse, McpManagementClientError> {
        let url = format!(
            "{}/api/internal/runtime/invocations/{}",
            self.config.base_url,
            urlencoding::encode(invocation_id.trim())
        );
        let response = self
            .internal_request(Method::GET, url, RUNTIME_INVOCATIONS_READ_SCOPE)?
            .send()
            .await?;
        parse_response(response).await
    }

    pub async fn notify_waiting_user_resolved(
        &self,
        prompt_id: &str,
    ) -> Result<(), McpManagementClientError> {
        let url = format!(
            "{}/api/internal/runtime/invocations/waiting-user/{}/resolved",
            self.config.base_url,
            urlencoding::encode(prompt_id.trim())
        );
        let response = self
            .internal_request(Method::POST, url, RUNTIME_INVOCATIONS_RESOLVE_USER_SCOPE)?
            .send()
            .await?;
        if response.status().is_success() {
            Ok(())
        } else {
            parse_response::<serde_json::Value>(response)
                .await
                .map(|_| ())
        }
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
            .header(INTERNAL_TOKEN_HEADER, token)
            .header(CALLER_SERVICE_HEADER, self.config.caller_service.as_str()))
    }

    fn runtime_session_request(
        &self,
        method: Method,
        url: String,
        scope: &str,
    ) -> Result<reqwest::RequestBuilder, McpManagementClientError> {
        Ok(self
            .internal_request(method, url, scope)?
            .timeout(self.config.runtime_session_request_timeout))
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::process::Command;
    use std::time::Duration;

    use super::*;

    #[test]
    fn internal_request_sends_only_signed_identity_headers() {
        let material_dir = std::env::temp_dir().join(format!(
            "chatos-mcp-management-sdk-test-{}",
            std::process::id()
        ));
        let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../scripts/generate-mcp-management-mtls.sh");
        assert!(Command::new(script)
            .arg(&material_dir)
            .status()
            .unwrap()
            .success());
        let secret = "a-long-mcp-management-test-secret";
        let caller_service = "task-runner";
        let client = McpManagementClient::new(McpManagementClientConfig {
            base_url: "https://mcp-management.test".to_string(),
            request_timeout: Duration::from_secs(1),
            runtime_session_request_timeout: Duration::from_secs(5),
            internal_api_secret: Some(secret.to_string()),
            caller_service: caller_service.to_string(),
            mtls_ca_cert_path: material_dir.join("ca.crt"),
            mtls_client_identity_path: material_dir.join("task-runner.identity.pem"),
        })
        .expect("valid client configuration");

        let request = client
            .internal_request(
                Method::POST,
                "https://mcp-management.test/api/internal/test".to_string(),
                ROUTES_RESOLVE_SCOPE,
            )
            .expect("signed internal request")
            .build()
            .expect("build internal request");

        assert!(request
            .headers()
            .get("x-mcp-management-internal-secret")
            .is_none());
        assert_eq!(
            request
                .headers()
                .get(CALLER_SERVICE_HEADER)
                .and_then(|value| value.to_str().ok()),
            Some(caller_service)
        );
        let token = request
            .headers()
            .get(INTERNAL_TOKEN_HEADER)
            .and_then(|value| value.to_str().ok())
            .expect("signed token header");
        let claims = chatos_service_runtime::verify_internal_service_token(
            token,
            secret,
            caller_service,
            INTERNAL_TOKEN_AUDIENCE,
            ROUTES_RESOLVE_SCOPE,
        )
        .expect("valid signed token");
        assert_eq!(claims.caller, caller_service);
        assert!(!claims.trace_id.is_empty());

        let runtime_session_request = client
            .runtime_session_request(
                Method::POST,
                "https://mcp-management.test/api/internal/runtime/sessions/resolve".to_string(),
                RUNTIME_SESSIONS_RESOLVE_SCOPE,
            )
            .expect("runtime session request")
            .build()
            .expect("build runtime session request");
        assert_eq!(client.config().request_timeout, Duration::from_secs(1));
        assert_eq!(
            runtime_session_request.timeout(),
            Some(&Duration::from_secs(5))
        );
        let _ = std::fs::remove_dir_all(material_dir);
    }

    #[test]
    fn plaintext_or_missing_mtls_material_is_rejected() {
        let plaintext = McpManagementClientConfig {
            base_url: "http://127.0.0.1:39282".to_string(),
            request_timeout: Duration::from_secs(1),
            runtime_session_request_timeout: Duration::from_secs(5),
            internal_api_secret: Some("test-secret".to_string()),
            caller_service: "task-runner".to_string(),
            mtls_ca_cert_path: PathBuf::new(),
            mtls_client_identity_path: PathBuf::new(),
        };
        assert!(matches!(
            McpManagementClient::new(plaintext),
            Err(McpManagementClientError::InvalidBaseUrl(_))
        ));

        let tls_without_material = McpManagementClientConfig {
            base_url: "https://127.0.0.1:39282".to_string(),
            request_timeout: Duration::from_secs(1),
            runtime_session_request_timeout: Duration::from_secs(5),
            internal_api_secret: Some("test-secret".to_string()),
            caller_service: "task-runner".to_string(),
            mtls_ca_cert_path: PathBuf::new(),
            mtls_client_identity_path: PathBuf::new(),
        };
        assert!(matches!(
            McpManagementClient::new(tls_without_material),
            Err(McpManagementClientError::InvalidMtlsConfiguration(_))
        ));
    }
}
