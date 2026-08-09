// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::trace_context::InternalTraceContextExt;

use super::{
    CloudSandboxProvider, ProviderCallError, CALLER_SERVICE, INTERNAL_SCOPE, TOKEN_AUDIENCE,
};

impl CloudSandboxProvider {
    pub(in crate::providers) fn authenticated(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<reqwest::RequestBuilder, ProviderCallError> {
        let secret = self.internal_secret.as_deref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "Sandbox Manager Provider internal secret is not configured",
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
