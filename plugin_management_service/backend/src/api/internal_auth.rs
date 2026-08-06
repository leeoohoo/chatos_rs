// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(super) const INTERNAL_TOKEN_AUDIENCE: &str = "plugin-management-service";
pub(super) const CAPABILITIES_RESOLVE_SCOPE: &str = "capabilities.resolve";
pub(super) const AGENT_PROMPTS_RESOLVE_SCOPE: &str = "agent-prompts.resolve";
pub(super) const AGENT_PROMPTS_SYNC_SCOPE: &str = "agent-prompts.sync";
pub(super) const LOCAL_CONNECTOR_READ_SCOPE: &str = "local-connector.read";
pub(super) const LOCAL_CONNECTOR_WRITE_SCOPE: &str = "local-connector.write";
pub(super) const PLUGIN_INSTALL_MANAGE_SCOPE: &str = "plugin.install.manage";
pub(super) const PLUGIN_OAUTH_MANAGE_SCOPE: &str = "plugin.oauth.manage";
pub(super) const PLUGIN_CLOUD_READ_SCOPE: &str = "plugin.cloud.read";
pub(super) const PLUGIN_CLOUD_CREDENTIALS_RESOLVE_SCOPE: &str = "plugin.cloud.credentials.resolve";
pub(super) const SYSTEM_STATS_READ_SCOPE: &str = "system.stats.read";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PluginManagementInternalIdentity {
    pub caller_service: String,
    pub scope: String,
    pub trace_id: String,
}

pub(super) struct PluginManagementInternalAuditGuard {
    event: chatos_service_runtime::InternalResourceAccessAudit,
    recorded: bool,
}

impl PluginManagementInternalAuditGuard {
    pub fn new(
        identity: &PluginManagementInternalIdentity,
        represented_user_id: Option<&str>,
        resource_type: &str,
        resource_id: &str,
        action: &str,
    ) -> Self {
        Self {
            event: chatos_service_runtime::InternalResourceAccessAudit {
                caller_service: identity.caller_service.clone(),
                audience_service: INTERNAL_TOKEN_AUDIENCE.to_string(),
                scope: identity.scope.clone(),
                trace_id: identity.trace_id.clone(),
                represented_user_id: normalized_optional(represented_user_id),
                tenant_id: None,
                project_id: None,
                resource_type: resource_type.to_string(),
                resource_id: resource_id.to_string(),
                resource_name: None,
                action: action.to_string(),
                outcome: "failed".to_string(),
            },
            recorded: false,
        }
    }

    pub fn resource_name(&mut self, value: Option<&str>) {
        self.event.resource_name = normalized_optional(value);
    }

    pub fn succeeded(mut self) {
        self.event.outcome = "succeeded".to_string();
        self.record();
    }

    fn record(&mut self) {
        if self.recorded {
            return;
        }
        if let Err(error) = chatos_service_runtime::record_internal_resource_access(&self.event) {
            tracing::error!(
                target: "chatos_internal_audit",
                trace_id = self.event.trace_id.as_str(),
                error = error.as_str(),
                "plugin management internal resource audit validation failed"
            );
        }
        self.recorded = true;
    }
}

impl Drop for PluginManagementInternalAuditGuard {
    fn drop(&mut self) {
        self.record();
    }
}

pub(super) fn require_local_connector_internal_request(
    state: &AppState,
    headers: &HeaderMap,
    required_scope: &str,
) -> Result<PluginManagementInternalIdentity, ApiError> {
    let caller = require_internal_caller_service(headers)?;
    if caller != "local-connector-service" {
        return Err(ApiError::forbidden(
            "local connector MCP sync requires local-connector-service caller",
        ));
    }
    require_internal_api_secret(state, headers, caller, required_scope)
}

pub(super) fn require_internal_caller_service(headers: &HeaderMap) -> Result<&str, ApiError> {
    let caller_service = headers
        .get("x-plugin-management-caller-service")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::bad_request("caller service is required"))?;
    if !ALLOWED_INTERNAL_CALLER_SERVICES.contains(&caller_service) {
        return Err(ApiError::forbidden("caller service is not allowed"));
    }
    Ok(caller_service)
}

pub(super) fn require_internal_api_secret(
    state: &AppState,
    headers: &HeaderMap,
    caller_service: &str,
    required_scope: &str,
) -> Result<PluginManagementInternalIdentity, ApiError> {
    let expected = state
        .config
        .internal_api_secrets
        .get(caller_service)
        .map(String::as_str)
        .ok_or_else(|| ApiError::unauthorized("plugin management internal API is disabled"))?;
    let token = headers
        .get("x-plugin-management-internal-token")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ApiError::unauthorized("signed plugin management internal API token is required")
        })?;
    let claims = chatos_service_runtime::verify_internal_service_token(
        token,
        expected,
        caller_service,
        INTERNAL_TOKEN_AUDIENCE,
        required_scope,
    )
    .map_err(|error| {
        tracing::warn!(
            caller_service,
            required_scope,
            error = error.as_str(),
            "plugin management internal token verification failed"
        );
        ApiError::unauthorized("invalid plugin management internal API token")
    })?;
    Ok(PluginManagementInternalIdentity {
        caller_service: caller_service.to_string(),
        scope: required_scope.to_string(),
        trace_id: claims.trace_id,
    })
}

fn normalized_optional(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}
