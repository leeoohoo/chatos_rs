// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::trace_context::InternalTraceContextExt;

use super::{
    LocalSandboxProvider, ProviderCallError, SandboxExecutionTarget, CALLER_SERVICE, TOKEN_AUDIENCE,
};

impl LocalSandboxProvider {
    pub(in crate::providers) fn authenticated(
        &self,
        request: reqwest::RequestBuilder,
        scope: &str,
        owner_user_id: &str,
    ) -> Result<reqwest::RequestBuilder, ProviderCallError> {
        let secret = self.internal_secret.as_deref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "Local Sandbox Provider internal secret is not configured",
            )
        })?;
        let owner_user_id = owner_user_id.trim();
        if owner_user_id.is_empty() {
            return Err(ProviderCallError::provider_unavailable(
                "Local Sandbox Provider owner identity is empty",
            ));
        }
        let token = chatos_service_runtime::issue_internal_service_token(
            secret,
            CALLER_SERVICE,
            TOKEN_AUDIENCE,
            scope,
            60,
        )
        .map_err(ProviderCallError::provider_unavailable)?;
        Ok(request
            .header("x-local-connector-caller", CALLER_SERVICE)
            .header("x-local-connector-internal-token", token)
            .header("x-local-connector-owner-user-id", owner_user_id)
            .with_internal_trace_context())
    }

    pub(in crate::providers) fn sandbox_url(
        &self,
        pairing_id: &str,
        sandbox_id: &str,
        suffix: Option<&str>,
    ) -> String {
        let mut url = format!(
            "{}/api/local-connectors/sandbox-facade/{}/api/sandboxes/{}",
            self.base_url,
            urlencoding::encode(pairing_id.trim()),
            urlencoding::encode(sandbox_id.trim())
        );
        if let Some(suffix) = suffix {
            url.push('/');
            url.push_str(suffix);
        }
        url
    }
}

pub(super) fn required_pairing_id(
    target: &SandboxExecutionTarget,
) -> Result<&str, ProviderCallError> {
    target
        .pairing_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "Local Sandbox target is missing its pairing id",
            )
        })
}
