// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_service_runtime::http_body::read_response_bytes_limited;
use serde_json::Value;

use super::{PluginComponentProvider, CALLER_SERVICE, PLUGIN_RELAY_SCOPE, TOKEN_AUDIENCE};
use crate::providers::ProviderCallError;
use crate::trace_context::InternalTraceContextExt;

impl PluginComponentProvider {
    pub(super) async fn request_local(
        &self,
        owner_user_id: &str,
        device_id: &str,
        workspace_id: &str,
        action: &str,
        body: Value,
    ) -> Result<Vec<u8>, ProviderCallError> {
        let secret = self.internal_secret.as_deref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "Plugin Component Provider internal secret is not configured",
            )
        })?;
        let token = chatos_service_runtime::issue_internal_service_token(
            secret,
            CALLER_SERVICE,
            TOKEN_AUDIENCE,
            PLUGIN_RELAY_SCOPE,
            60,
        )
        .map_err(ProviderCallError::provider_unavailable)?;
        let mut url = reqwest::Url::parse(
            format!(
                "{}/api/local-connectors/relay/{}/plugins/{action}",
                self.base_url,
                urlencoding::encode(device_id)
            )
            .as_str(),
        )
        .map_err(|error| {
            ProviderCallError::provider_unavailable(format!(
                "build Plugin Component Provider URL failed: {error}"
            ))
        })?;
        url.query_pairs_mut()
            .append_pair("workspace_id", workspace_id);
        let response = self
            .http
            .post(url)
            .header("x-local-connector-caller", CALLER_SERVICE)
            .header("x-local-connector-internal-token", token)
            .header("x-local-connector-owner-user-id", owner_user_id)
            .with_internal_trace_context()
            .json(&body)
            .timeout(self.request_timeout)
            .send()
            .await
            .map_err(|error| {
                ProviderCallError::provider_unavailable(format!(
                    "Plugin Component Provider request failed: {error}"
                ))
            })?;
        let status = response.status();
        let bytes = read_response_bytes_limited(response, self.response_limit_bytes)
            .await
            .map_err(|error| {
                ProviderCallError::invalid_response(format!(
                    "Plugin Component Provider response could not be read: {error}"
                ))
            })?;
        if !status.is_success() {
            return Err(ProviderCallError::provider_unavailable(format!(
                "Plugin Component Provider rejected {action} with HTTP {}",
                status.as_u16()
            )));
        }
        Ok(bytes.to_vec())
    }
}
