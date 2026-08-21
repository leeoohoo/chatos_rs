// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

use super::{PluginLocalProvider, CALLER_SERVICE, PLUGIN_RELAY_SCOPE, TOKEN_AUDIENCE};
use crate::providers::ProviderCallError;
use crate::trace_context::InternalTraceContextExt;

impl PluginLocalProvider {
    pub(super) async fn request(
        &self,
        owner_user_id: &str,
        device_id: &str,
        workspace_id: &str,
        action: &str,
        body: Value,
    ) -> Result<Vec<u8>, ProviderCallError> {
        let secret = self.internal_secret.as_deref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "Plugin Local Provider internal secret is not configured",
            )
        })?;
        let token = chatos_service_runtime::issue_internal_service_token_for_owner(
            secret,
            CALLER_SERVICE,
            TOKEN_AUDIENCE,
            PLUGIN_RELAY_SCOPE,
            60,
            owner_user_id,
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
                "build Plugin Local Provider URL failed: {error}"
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
                    "Plugin Local Provider request failed: {error}"
                ))
            })?;
        let status = response.status();
        let bytes = chatos_service_runtime::http_body::read_response_bytes_limited(
            response,
            self.response_limit_bytes,
        )
        .await
        .map_err(|error| {
            ProviderCallError::invalid_response(format!(
                "Plugin Local Provider response could not be read: {error}"
            ))
        })?;
        if !status.is_success() {
            let detail = relay_error_detail(bytes.as_ref())
                .map(|detail| format!(": {detail}"))
                .unwrap_or_default();
            return Err(ProviderCallError::provider_unavailable(format!(
                "Plugin Local Provider rejected {action} with HTTP {}{detail}",
                status.as_u16(),
            )));
        }
        Ok(bytes.to_vec())
    }
}

pub(super) fn relay_error_detail(bytes: &[u8]) -> Option<String> {
    const MAX_DETAIL_CHARS: usize = 512;
    let value = serde_json::from_slice::<Value>(bytes).ok()?;
    let detail = value
        .get("error")
        .or_else(|| value.get("message"))?
        .as_str()?
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let detail = detail.trim();
    if detail.is_empty() {
        return None;
    }
    Some(detail.chars().take(MAX_DETAIL_CHARS).collect())
}
