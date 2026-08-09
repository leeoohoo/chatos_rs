// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_management_sdk::{
    ProjectExecutionContext, SandboxProviderKind, WorkspaceProviderKind,
};
use chatos_service_runtime::http_body::read_response_bytes_limited;

use super::records::SandboxPairingRecord;
use super::{LocalSandboxProvider, ProviderCallError, SANDBOX_ROUTING_SCOPE};

impl LocalSandboxProvider {
    pub(in crate::providers) async fn resolve_active_pairing(
        &self,
        context: &ProjectExecutionContext,
    ) -> Result<Option<String>, ProviderCallError> {
        if context.sandbox_provider != SandboxProviderKind::LocalConnector {
            return Ok(None);
        }
        if context.workspace_provider != WorkspaceProviderKind::LocalConnector {
            return Err(ProviderCallError::provider_unavailable(
                "Local Sandbox requires a Local Connector workspace",
            ));
        }
        let workspace = context.workspace.as_ref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "Local Sandbox Project Context is missing its workspace target",
            )
        })?;
        let device_id = workspace
            .device_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable(
                    "Local Sandbox Project Context is missing its device id",
                )
            })?;
        let workspace_id = workspace.workspace_id.trim();
        if workspace_id.is_empty() {
            return Err(ProviderCallError::provider_unavailable(
                "Local Sandbox Project Context is missing its workspace id",
            ));
        }
        let mut url = reqwest::Url::parse(
            format!("{}/api/local-connectors/sandbox-pairings", self.base_url).as_str(),
        )
        .map_err(|error| {
            ProviderCallError::provider_unavailable(format!(
                "build Local Sandbox pairing URL failed: {error}"
            ))
        })?;
        url.query_pairs_mut()
            .append_pair("active_only", "true")
            .append_pair("device_id", device_id)
            .append_pair("workspace_id", workspace_id);
        let response = self
            .authenticated(
                self.http.get(url),
                SANDBOX_ROUTING_SCOPE,
                context.owner_user_id.as_str(),
            )?
            .timeout(self.request_timeout)
            .send()
            .await
            .map_err(|error| {
                ProviderCallError::provider_unavailable(format!(
                    "Local Sandbox pairing request failed: {error}"
                ))
            })?;
        let status = response.status();
        let bytes = read_response_bytes_limited(response, self.response_limit_bytes)
            .await
            .map_err(|error| {
                ProviderCallError::invalid_response(format!(
                    "Local Sandbox pairing response could not be read: {error}"
                ))
            })?;
        if !status.is_success() {
            return Err(ProviderCallError::provider_unavailable(format!(
                "Local Sandbox pairing request returned HTTP {}",
                status.as_u16()
            )));
        }
        let pairings = serde_json::from_slice::<Vec<SandboxPairingRecord>>(bytes.as_slice())
            .map_err(|error| {
                ProviderCallError::invalid_response(format!(
                    "Local Sandbox pairing response is invalid: {error}"
                ))
            })?;
        Ok(pairings
            .into_iter()
            .find(|pairing| {
                pairing.enabled
                    && pairing.device_id == device_id
                    && pairing.workspace_id == workspace_id
                    && pairing
                        .sandbox_readiness
                        .trim()
                        .eq_ignore_ascii_case("ready")
            })
            .map(|pairing| pairing.id))
    }
}
