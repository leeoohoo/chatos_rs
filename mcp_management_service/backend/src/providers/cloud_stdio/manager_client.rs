// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::Serialize;

use super::{
    CloudStdioProvider, CloudStdioRequestContext, ProviderCallError, SandboxExecutionTarget,
    CALLER_SERVICE, INTERNAL_SCOPE, TOKEN_AUDIENCE,
};
use crate::trace_context::InternalTraceContextExt;

impl CloudStdioProvider {
    pub(super) async fn request<T>(
        &self,
        target: &SandboxExecutionTarget,
        context: &CloudStdioRequestContext<'_>,
        action: &str,
        body: &T,
    ) -> Result<reqwest::Response, ProviderCallError>
    where
        T: Serialize + ?Sized,
    {
        let sandbox_id = urlencoding::encode(target.sandbox_id.trim());
        let prefix = if target.is_environment {
            "sandbox-environments"
        } else {
            "sandboxes"
        };
        let url = format!(
            "{}/api/internal/{prefix}/{sandbox_id}/cloud-stdio-mcp/{action}",
            self.base_url
        );
        let mut request = self.authenticated(self.http.post(url))?;
        if let Some(service_id) = target.service_id.as_deref() {
            request = request.header("x-chatos-service-id", service_id);
        }
        request
            .header("x-chatos-sandbox-lease-id", target.lease_id.as_str())
            .header("x-mcp-management-owner-user-id", context.owner_user_id)
            .header("x-mcp-management-project-id", context.project_id)
            .header(
                "x-mcp-management-run-id",
                context.run_id.unwrap_or_default(),
            )
            .json(body)
            .send()
            .await
            .map_err(|error| {
                ProviderCallError::provider_unavailable(format!(
                    "Cloud stdio MCP runner request failed: {error}"
                ))
            })
    }

    fn authenticated(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<reqwest::RequestBuilder, ProviderCallError> {
        let secret = self.internal_secret.as_deref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "Sandbox Manager cloud stdio internal secret is not configured",
            )
        })?;
        let token = chatos_service_runtime::issue_internal_service_token(
            secret,
            CALLER_SERVICE,
            TOKEN_AUDIENCE,
            INTERNAL_SCOPE,
            60,
        )
        .map_err(ProviderCallError::provider_unavailable)?;
        Ok(request
            .header("x-sandbox-caller", CALLER_SERVICE)
            .header("x-sandbox-internal-token", token)
            .with_internal_trace_context())
    }
}
