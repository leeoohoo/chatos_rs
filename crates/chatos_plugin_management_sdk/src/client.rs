// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use reqwest::{Method, StatusCode};
use serde::de::DeserializeOwned;
use serde::Deserialize;

use crate::config::PluginManagementClientConfig;
use crate::dto::{
    AgentPromptBundle, AgentPromptBundleManifest, LocalConnectorMcpListResponse,
    LocalConnectorMcpStatusBatchRequest, LocalConnectorMcpStatusRequest,
    LocalConnectorMcpSyncRequest, LocalConnectorSkillInventoryRequest, McpRecord,
    ResolveAgentCapabilitiesRequest, ResolveAgentPromptRequest, ResolvedAgentCapabilities,
    ResolvedAgentPrompt, ResourceCheckRecord, SkillInstallationRecord,
    UpdateUserSkillPreferenceRequest, UserSkillCatalogItem, UserSkillCatalogResponse,
};
use crate::error::PluginManagementClientError;
use crate::plugin_runtime::{
    PluginInstallSource, PluginInstallSourceList, PluginInstallationRecord,
    PluginInstallationSyncPayload, UpdateUserPluginPreferenceRequest,
    UpdateUserPluginPreferenceResponse,
};
use crate::plugin_runtime::{PluginOAuthConnectionRecord, PluginOAuthStatusSyncPayload};

const INTERNAL_TOKEN_HEADER: &str = "x-plugin-management-internal-token";
const CALLER_SERVICE_HEADER: &str = "x-plugin-management-caller-service";
const INTERNAL_TOKEN_AUDIENCE: &str = "plugin-management-service";
const CAPABILITIES_RESOLVE_SCOPE: &str = "capabilities.resolve";
const AGENT_PROMPTS_RESOLVE_SCOPE: &str = "agent-prompts.resolve";
const AGENT_PROMPTS_SYNC_SCOPE: &str = "agent-prompts.sync";
const LOCAL_CONNECTOR_READ_SCOPE: &str = "local-connector.read";
const LOCAL_CONNECTOR_WRITE_SCOPE: &str = "local-connector.write";
const PLUGIN_OAUTH_MANAGE_SCOPE: &str = "plugin.oauth.manage";
const PLUGIN_INSTALL_MANAGE_SCOPE: &str = "plugin.install.manage";

#[derive(Clone)]
pub struct PluginManagementClient {
    public_http: reqwest::Client,
    internal_http: reqwest::Client,
    config: PluginManagementClientConfig,
}

impl PluginManagementClient {
    pub fn new(config: PluginManagementClientConfig) -> Result<Self, PluginManagementClientError> {
        reqwest::Url::parse(config.public_base_url.as_str())
            .map_err(|err| PluginManagementClientError::InvalidBaseUrl(err.to_string()))?;
        let internal_url = reqwest::Url::parse(config.internal_base_url.as_str())
            .map_err(|err| PluginManagementClientError::InvalidBaseUrl(err.to_string()))?;
        if internal_url.scheme() != "https" {
            return Err(PluginManagementClientError::InvalidBaseUrl(
                "Plugin Management internal base URL must use https".to_string(),
            ));
        }
        let public_http = reqwest::Client::builder()
            .timeout(config.request_timeout)
            .build()?;
        let internal_http = config.internal_http.clone();
        Ok(Self {
            public_http,
            internal_http,
            config,
        })
    }

    pub fn config(&self) -> &PluginManagementClientConfig {
        &self.config
    }

    pub async fn resolve_for_user(
        &self,
        request: &ResolveAgentCapabilitiesRequest,
        bearer_token: &str,
    ) -> Result<ResolvedAgentCapabilities, PluginManagementClientError> {
        let url = format!(
            "{}/api/runtime/agent-capabilities",
            self.config.public_base_url
        );
        let token = bearer_token
            .trim()
            .strip_prefix("Bearer ")
            .unwrap_or(bearer_token.trim());
        let mut query = vec![
            ("agent_key", request.agent_key.as_str()),
            ("owner_user_id", request.owner_user_id.as_str()),
            (
                "include_unavailable",
                if request.include_unavailable {
                    "true"
                } else {
                    "false"
                },
            ),
        ];
        if let Some(value) = request.task_profile.as_deref() {
            query.push(("task_profile", value));
        }
        if let Some(value) = request.runtime_provider.as_deref() {
            query.push(("runtime_provider", value));
        }
        if let Some(value) = request.schedule_mode.as_deref() {
            query.push(("schedule_mode", value));
        }
        if let Some(value) = request.device_id.as_deref() {
            query.push(("device_id", value));
        }
        let response = self
            .public_http
            .request(Method::GET, url)
            .bearer_auth(token)
            .query(&query)
            .send()
            .await?;
        parse_response(response).await
    }

    pub async fn resolve_for_service(
        &self,
        request: &ResolveAgentCapabilitiesRequest,
    ) -> Result<ResolvedAgentCapabilities, PluginManagementClientError> {
        let url = format!(
            "{}/api/internal/runtime/agent-capabilities/resolve",
            self.config.internal_base_url
        );
        let response = self
            .internal_request(Method::POST, url, CAPABILITIES_RESOLVE_SCOPE)?
            .json(request)
            .send()
            .await?;
        parse_response(response).await
    }

    pub async fn resolve_agent_prompt_for_service(
        &self,
        request: &ResolveAgentPromptRequest,
    ) -> Result<ResolvedAgentPrompt, PluginManagementClientError> {
        let url = format!(
            "{}/api/internal/runtime/agent-prompts/resolve",
            self.config.internal_base_url
        );
        let response = self
            .internal_request(Method::POST, url, AGENT_PROMPTS_RESOLVE_SCOPE)?
            .json(request)
            .send()
            .await?;
        parse_response(response).await
    }

    pub async fn get_agent_prompt_bundle_manifest_for_service(
        &self,
    ) -> Result<AgentPromptBundleManifest, PluginManagementClientError> {
        let url = format!(
            "{}/api/internal/runtime/agent-prompts/manifest",
            self.config.internal_base_url
        );
        let response = self
            .internal_request(Method::GET, url, AGENT_PROMPTS_SYNC_SCOPE)?
            .send()
            .await?;
        parse_response(response).await
    }

    pub async fn get_agent_prompt_bundle_for_service(
        &self,
    ) -> Result<AgentPromptBundle, PluginManagementClientError> {
        let url = format!(
            "{}/api/internal/runtime/agent-prompts/bundle",
            self.config.internal_base_url
        );
        let response = self
            .internal_request(Method::GET, url, AGENT_PROMPTS_SYNC_SCOPE)?
            .send()
            .await?;
        parse_response(response).await
    }

    pub async fn list_local_connector_mcps(
        &self,
        owner_user_id: &str,
        device_id: &str,
    ) -> Result<LocalConnectorMcpListResponse, PluginManagementClientError> {
        let url = format!(
            "{}/api/internal/local-connector/mcps",
            self.config.internal_base_url
        );
        let response = self
            .internal_request(Method::GET, url, LOCAL_CONNECTOR_READ_SCOPE)?
            .query(&[("owner_user_id", owner_user_id), ("device_id", device_id)])
            .send()
            .await?;
        parse_response(response).await
    }

    pub async fn sync_local_connector_mcp(
        &self,
        request: &LocalConnectorMcpSyncRequest,
    ) -> Result<McpRecord, PluginManagementClientError> {
        let url = format!(
            "{}/api/internal/local-connector/mcps",
            self.config.internal_base_url
        );
        let response = self
            .internal_request(Method::POST, url, LOCAL_CONNECTOR_WRITE_SCOPE)?
            .json(request)
            .send()
            .await?;
        parse_response(response).await
    }

    pub async fn update_local_connector_mcp(
        &self,
        mcp_id: &str,
        request: &LocalConnectorMcpSyncRequest,
    ) -> Result<McpRecord, PluginManagementClientError> {
        let url = format!(
            "{}/api/internal/local-connector/mcps/{}",
            self.config.internal_base_url,
            urlencoding::encode(mcp_id)
        );
        let response = self
            .internal_request(Method::PATCH, url, LOCAL_CONNECTOR_WRITE_SCOPE)?
            .json(request)
            .send()
            .await?;
        parse_response(response).await
    }

    pub async fn delete_local_connector_mcp(
        &self,
        mcp_id: &str,
        owner_user_id: &str,
        device_id: &str,
        manifest_id: &str,
    ) -> Result<(), PluginManagementClientError> {
        let url = format!(
            "{}/api/internal/local-connector/mcps/{}",
            self.config.internal_base_url,
            urlencoding::encode(mcp_id)
        );
        let response = self
            .internal_request(Method::DELETE, url, LOCAL_CONNECTOR_WRITE_SCOPE)?
            .query(&[
                ("owner_user_id", owner_user_id),
                ("device_id", device_id),
                ("manifest_id", manifest_id),
            ])
            .send()
            .await?;
        parse_empty_response(response).await
    }

    pub async fn update_local_connector_mcp_status(
        &self,
        mcp_id: &str,
        request: &LocalConnectorMcpStatusRequest,
    ) -> Result<ResourceCheckRecord, PluginManagementClientError> {
        let url = format!(
            "{}/api/internal/local-connector/mcps/{}/status",
            self.config.internal_base_url,
            urlencoding::encode(mcp_id)
        );
        let response = self
            .internal_request(Method::PUT, url, LOCAL_CONNECTOR_WRITE_SCOPE)?
            .json(request)
            .send()
            .await?;
        parse_response(response).await
    }

    pub async fn update_local_connector_mcp_status_batch(
        &self,
        request: &LocalConnectorMcpStatusBatchRequest,
    ) -> Result<Vec<ResourceCheckRecord>, PluginManagementClientError> {
        let url = format!(
            "{}/api/internal/local-connector/mcps/status/batch",
            self.config.internal_base_url
        );
        let response = self
            .internal_request(Method::PUT, url, LOCAL_CONNECTOR_WRITE_SCOPE)?
            .json(request)
            .send()
            .await?;
        parse_response(response).await
    }

    pub async fn list_user_skill_catalog(
        &self,
        owner_user_id: &str,
        device_id: Option<&str>,
    ) -> Result<UserSkillCatalogResponse, PluginManagementClientError> {
        let url = format!(
            "{}/api/internal/local-connector/skills/catalog",
            self.config.internal_base_url
        );
        let mut query = vec![("owner_user_id", owner_user_id)];
        if let Some(device_id) = device_id {
            query.push(("device_id", device_id));
        }
        let response = self
            .internal_request(Method::GET, url, LOCAL_CONNECTOR_READ_SCOPE)?
            .query(&query)
            .send()
            .await?;
        parse_response(response).await
    }

    pub async fn update_user_skill_preference(
        &self,
        skill_id: &str,
        request: &UpdateUserSkillPreferenceRequest,
    ) -> Result<UserSkillCatalogItem, PluginManagementClientError> {
        let url = format!(
            "{}/api/internal/local-connector/skills/{}/preference",
            self.config.internal_base_url,
            urlencoding::encode(skill_id)
        );
        let response = self
            .internal_request(Method::PUT, url, LOCAL_CONNECTOR_WRITE_SCOPE)?
            .json(request)
            .send()
            .await?;
        parse_response(response).await
    }

    pub async fn sync_local_connector_skill_inventory(
        &self,
        request: &LocalConnectorSkillInventoryRequest,
    ) -> Result<Vec<SkillInstallationRecord>, PluginManagementClientError> {
        let url = format!(
            "{}/api/internal/local-connector/skills/inventory",
            self.config.internal_base_url
        );
        let response = self
            .internal_request(Method::PUT, url, LOCAL_CONNECTOR_WRITE_SCOPE)?
            .json(request)
            .send()
            .await?;
        parse_response(response).await
    }

    pub async fn sync_plugin_oauth_status(
        &self,
        request: &PluginOAuthStatusSyncPayload,
    ) -> Result<PluginOAuthConnectionRecord, PluginManagementClientError> {
        let url = format!(
            "{}/api/internal/local-connector/plugins/oauth",
            self.config.internal_base_url
        );
        let response = self
            .internal_request(Method::PUT, url, PLUGIN_OAUTH_MANAGE_SCOPE)?
            .json(request)
            .send()
            .await?;
        parse_response(response).await
    }

    pub async fn sync_plugin_installation(
        &self,
        request: &PluginInstallationSyncPayload,
    ) -> Result<PluginInstallationRecord, PluginManagementClientError> {
        let url = format!(
            "{}/api/internal/local-connector/plugins/installations",
            self.config.internal_base_url
        );
        let response = self
            .internal_request(Method::PUT, url, PLUGIN_INSTALL_MANAGE_SCOPE)?
            .json(request)
            .send()
            .await?;
        parse_response(response).await
    }

    pub async fn list_plugin_install_sources_for_service(
        &self,
        owner_user_id: &str,
    ) -> Result<PluginInstallSourceList, PluginManagementClientError> {
        let url = format!(
            "{}/api/internal/local-connector/plugins/install-sources",
            self.config.internal_base_url
        );
        let response = self
            .internal_request(Method::GET, url, PLUGIN_INSTALL_MANAGE_SCOPE)?
            .query(&[("owner_user_id", owner_user_id)])
            .send()
            .await?;
        parse_response(response).await
    }

    pub async fn get_plugin_install_source_for_service(
        &self,
        plugin_id: &str,
        release_id: &str,
        owner_user_id: &str,
    ) -> Result<PluginInstallSource, PluginManagementClientError> {
        let url = format!(
            "{}/api/internal/local-connector/plugins/install-sources/{}/{}",
            self.config.internal_base_url,
            urlencoding::encode(plugin_id),
            urlencoding::encode(release_id),
        );
        let response = self
            .internal_request(Method::GET, url, PLUGIN_INSTALL_MANAGE_SCOPE)?
            .query(&[("owner_user_id", owner_user_id)])
            .send()
            .await?;
        parse_response(response).await
    }

    pub async fn update_user_plugin_preference_for_service(
        &self,
        plugin_id: &str,
        request: &UpdateUserPluginPreferenceRequest,
    ) -> Result<UpdateUserPluginPreferenceResponse, PluginManagementClientError> {
        let url = format!(
            "{}/api/internal/local-connector/plugins/{}/preference",
            self.config.internal_base_url,
            urlencoding::encode(plugin_id),
        );
        let response = self
            .internal_request(Method::PUT, url, PLUGIN_INSTALL_MANAGE_SCOPE)?
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
    ) -> Result<reqwest::RequestBuilder, PluginManagementClientError> {
        let secret = self
            .config
            .internal_api_secret
            .as_deref()
            .ok_or(PluginManagementClientError::MissingInternalSecret)?;
        let token = chatos_service_runtime::issue_internal_service_token(
            secret,
            self.config.caller_service.as_str(),
            INTERNAL_TOKEN_AUDIENCE,
            scope,
            60,
        )
        .map_err(PluginManagementClientError::InternalToken)?;
        Ok(self
            .internal_http
            .request(method, url)
            .header(INTERNAL_TOKEN_HEADER, token)
            .header(CALLER_SERVICE_HEADER, self.config.caller_service.as_str()))
    }
}

#[derive(Debug, Deserialize)]
struct ErrorResponse {
    error: Option<String>,
}

async fn parse_response<T>(response: reqwest::Response) -> Result<T, PluginManagementClientError>
where
    T: DeserializeOwned,
{
    let status = response.status();
    if status.is_success() {
        return response
            .json::<T>()
            .await
            .map_err(PluginManagementClientError::Transport);
    }
    let status_code = status.as_u16();
    let body = response.text().await.unwrap_or_default();
    let message = serde_json::from_str::<ErrorResponse>(body.as_str())
        .ok()
        .and_then(|value| value.error)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| default_error_message(status));
    Err(PluginManagementClientError::Rejected {
        status: status_code,
        message,
    })
}

async fn parse_empty_response(
    response: reqwest::Response,
) -> Result<(), PluginManagementClientError> {
    if response.status().is_success() {
        return Ok(());
    }
    let status = response.status();
    let status_code = status.as_u16();
    let body = response.text().await.unwrap_or_default();
    let message = serde_json::from_str::<ErrorResponse>(body.as_str())
        .ok()
        .and_then(|value| value.error)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| default_error_message(status));
    Err(PluginManagementClientError::Rejected {
        status: status_code,
        message,
    })
}

fn default_error_message(status: StatusCode) -> String {
    status
        .canonical_reason()
        .unwrap_or("unknown plugin management error")
        .to_string()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn internal_request_sends_only_signed_identity_headers() {
        let secret = "a-long-plugin-management-test-secret";
        let caller_service = "task-runner";
        let client = PluginManagementClient::new(
            PluginManagementClientConfig::new(
                "http://plugin-management.test",
                "https://plugin-management.test",
                Duration::from_secs(1),
                Some(secret.to_string()),
                caller_service,
                reqwest::Client::new(),
            )
            .expect("valid client configuration"),
        )
        .expect("valid client configuration");

        let request = client
            .internal_request(
                Method::POST,
                "https://plugin-management.test/api/internal/test".to_string(),
                CAPABILITIES_RESOLVE_SCOPE,
            )
            .expect("signed internal request")
            .build()
            .expect("build internal request");

        assert!(request
            .headers()
            .get("x-plugin-management-internal-secret")
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
            CAPABILITIES_RESOLVE_SCOPE,
        )
        .expect("valid signed token");
        assert_eq!(claims.caller, caller_service);
        assert!(!claims.trace_id.is_empty());
    }
}
