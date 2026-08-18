// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_management_sdk::{
    McpProviderKind, ProjectExecutionContext, RuntimeProviderChangedFile,
    RuntimeProviderFinalization, RuntimeProviderFinalizationStatus, WorkspaceProviderKind,
};
use serde_json::{json, Value};

use super::{
    LocalConnectorProvider, ProviderCallError, CALLER_SERVICE, LOCAL_CONNECTOR_PROJECT_ID_HEADER,
    MCP_MANAGEMENT_EXECUTION_GROUP_ID_HEADER, MCP_MANAGEMENT_RUN_ID_HEADER,
    MCP_MANAGEMENT_SCOPE_GENERATION_HEADER, MCP_RELAY_SCOPE, TOKEN_AUDIENCE,
};

impl LocalConnectorProvider {
    pub(in crate::providers) async fn finalize_run(
        &self,
        context: &ProjectExecutionContext,
        owner_user_id: &str,
        project_id: &str,
        run_id: &str,
        execution_group_id: Option<&str>,
        generation: i64,
        status: &str,
    ) -> Result<Option<RuntimeProviderFinalization>, ProviderCallError> {
        if context.workspace_provider != WorkspaceProviderKind::LocalConnector {
            return Ok(None);
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
        let mut request = self
            .http
            .post(url)
            .header("x-local-connector-caller", CALLER_SERVICE)
            .header("x-local-connector-internal-token", token)
            .header("x-local-connector-owner-user-id", owner_user_id)
            .header(LOCAL_CONNECTOR_PROJECT_ID_HEADER, project_id)
            .header(MCP_MANAGEMENT_RUN_ID_HEADER, run_id)
            .header(MCP_MANAGEMENT_SCOPE_GENERATION_HEADER, generation);
        if let Some(execution_group_id) = execution_group_id {
            request = request.header(MCP_MANAGEMENT_EXECUTION_GROUP_ID_HEADER, execution_group_id);
        }
        let response = request
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
        let result = body.get("result").ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "Local Connector lifecycle response is missing result",
            )
        })?;
        let status = match result
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("succeeded")
        {
            "succeeded" | "integrated" => RuntimeProviderFinalizationStatus::Succeeded,
            "no_changes" => RuntimeProviderFinalizationStatus::NoChanges,
            "conflict" => RuntimeProviderFinalizationStatus::Conflict,
            value => {
                return Err(ProviderCallError::provider_unavailable(format!(
                    "Local Connector lifecycle returned unsupported status: {value}"
                )))
            }
        };
        Ok(Some(RuntimeProviderFinalization {
            provider_kind: McpProviderKind::LocalConnector,
            status,
            execution_group_id: result
                .get("execution_group_id")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
                .or_else(|| execution_group_id.map(ToOwned::to_owned)),
            execution_branch_ref: result
                .get("execution_branch_ref")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            base_commit: result
                .get("base_commit")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            result_commit: result
                .get("result_commit")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            integrated_commit: result
                .get("integrated_commit")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            conflict_files: result
                .get("conflict_files")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect(),
            files: result
                .get("files")
                .cloned()
                .map(serde_json::from_value::<Vec<RuntimeProviderChangedFile>>)
                .transpose()
                .map_err(|error| {
                    ProviderCallError::provider_unavailable(format!(
                        "decode Local Connector changed files failed: {error}"
                    ))
                })?
                .unwrap_or_default(),
            message: result
                .get("message")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            patch: result
                .get("patch")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            patch_truncated: result
                .get("patch_truncated")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }))
    }
}
