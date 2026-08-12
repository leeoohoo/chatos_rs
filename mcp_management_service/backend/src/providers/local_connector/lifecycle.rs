// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_management_sdk::{
    ProjectExecutionContext, RuntimeRunTerminalStatus, WorkspaceProviderKind,
};
use serde_json::{json, Value};

use super::{
    LocalConnectorProvider, ProviderCallError, CALLER_SERVICE, LOCAL_CONNECTOR_PROJECT_ID_HEADER,
    MCP_MANAGEMENT_RUN_ID_HEADER, MCP_MANAGEMENT_SCOPE_GENERATION_HEADER, MCP_RELAY_SCOPE,
    TOKEN_AUDIENCE,
};

impl LocalConnectorProvider {
    pub(in crate::providers) async fn finalize_run(
        &self,
        context: &ProjectExecutionContext,
        owner_user_id: &str,
        project_id: &str,
        run_id: &str,
        generation: i64,
        status: RuntimeRunTerminalStatus,
    ) -> Result<(), ProviderCallError> {
        if context.workspace_provider != WorkspaceProviderKind::LocalConnector {
            return Ok(());
        }
        let workspace = context.workspace.as_ref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "Local Connector execution context is missing workspace binding",
            )
        })?;
        let device_id = workspace.device_id.as_deref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "Local Connector execution context is missing device binding",
            )
        })?;
        let secret = self.internal_secret.as_deref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "Local Connector Provider internal secret is not configured",
            )
        })?;
        let token = chatos_service_runtime::issue_internal_service_token_for_owner(
            secret,
            CALLER_SERVICE,
            TOKEN_AUDIENCE,
            MCP_RELAY_SCOPE,
            60,
            owner_user_id,
        )
        .map_err(ProviderCallError::provider_unavailable)?;
        let mut url = reqwest::Url::parse(
            format!(
                "{}/api/local-connectors/relay/{}/mcp",
                self.base_url,
                urlencoding::encode(device_id)
            )
            .as_str(),
        )
        .map_err(|error| {
            ProviderCallError::provider_unavailable(format!(
                "build Local Connector lifecycle URL failed: {error}"
            ))
        })?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("workspace_id", workspace.workspace_id.as_str());
            if let Some(relative_root) = workspace.relative_root.as_deref() {
                query.append_pair("cwd", relative_root);
            }
        }
        let response = self
            .http
            .post(url)
            .header("x-local-connector-caller", CALLER_SERVICE)
            .header("x-local-connector-internal-token", token)
            .header("x-local-connector-owner-user-id", owner_user_id)
            .header(LOCAL_CONNECTOR_PROJECT_ID_HEADER, project_id)
            .header(MCP_MANAGEMENT_RUN_ID_HEADER, run_id)
            .header(MCP_MANAGEMENT_SCOPE_GENERATION_HEADER, generation)
            .json(&json!({
                "jsonrpc": "2.0",
                "id": format!("finalize-{run_id}"),
                "method": "local_connector/execution_scope/finalize",
                "params": { "status": status, "generation": generation },
            }))
            .timeout(self.request_timeout)
            .send()
            .await
            .map_err(|error| {
                ProviderCallError::provider_unavailable(format!(
                    "finalize Local Connector execution scope failed: {error}"
                ))
            })?;
        let http_status = response.status();
        let body = response.json::<Value>().await.map_err(|error| {
            ProviderCallError::provider_unavailable(format!(
                "decode Local Connector lifecycle response failed: {error}"
            ))
        })?;
        if !http_status.is_success() || body.get("error").is_some() {
            return Err(ProviderCallError::provider_unavailable(format!(
                "Local Connector lifecycle request failed with HTTP {http_status}: {body}"
            )));
        }
        Ok(())
    }
}
