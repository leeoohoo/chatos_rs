// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp::{AskUserResponseSubmission, SystemMcpKey};
use chatos_mcp_management_sdk::ResolvedMcpRoute;
use chatos_mcp_service::METHOD_TOOLS_CALL;
use serde_json::{json, Value};

use crate::providers::managed_tool_call_params;
use crate::runtime::RuntimeSessionSnapshot;

use super::{
    ProviderCallError, ProviderWaitingForUser, TaskRunnerProvider, TaskRunnerRequestBinding,
};

impl TaskRunnerProvider {
    pub(in crate::providers) async fn start_waiting_user_call(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        route: &ResolvedMcpRoute,
        original_tool_name: &str,
        arguments: Value,
        invocation_id: &str,
    ) -> Result<ProviderWaitingForUser, ProviderCallError> {
        let secret = self.internal_secret.as_deref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "Task Runner Provider internal secret is not configured",
            )
        })?;
        let endpoint = format!(
            "{}/internal/mcp-management/mcp/{}/start",
            self.base_url,
            urlencoding::encode(SystemMcpKey::AskUser.as_str())
        );
        let binding = TaskRunnerRequestBinding::from(snapshot);
        let response = self
            .bound_request(
                &binding,
                endpoint,
                self.request_timeout,
                secret,
                super::TASK_RUNNER_MCP_SCOPE,
            )?
            .json(&json!({
                "jsonrpc": "2.0",
                "id": invocation_id,
                "method": METHOD_TOOLS_CALL,
                "params": managed_tool_call_params(
                    original_tool_name,
                    arguments,
                    snapshot.tool_result_max_chars,
                ),
            }))
            .send()
            .await
            .map_err(|error| {
                ProviderCallError::provider_unavailable(format!(
                    "Task Runner Ask User start request failed: {error}"
                ))
            })?;
        let value = response.json::<Value>().await.map_err(|error| {
            ProviderCallError::invalid_response(format!(
                "Task Runner Ask User start response is invalid: {error}"
            ))
        })?;
        let prompt_id = value
            .pointer("/result/prompt_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                ProviderCallError::invalid_response(format!(
                    "Task Runner Ask User start response has no prompt_id: {value}"
                ))
            })?;
        let _ = route;
        Ok(ProviderWaitingForUser {
            prompt_id: prompt_id.to_string(),
        })
    }

    pub(in crate::providers) async fn resolve_waiting_user_call(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        prompt_id: &str,
        invocation_id: &str,
    ) -> Result<Option<Value>, ProviderCallError> {
        let secret = self.internal_secret.as_deref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "Task Runner Provider internal secret is not configured",
            )
        })?;
        let endpoint = format!(
            "{}/internal/mcp-management/mcp/{}/prompts/{}",
            self.base_url,
            urlencoding::encode(SystemMcpKey::AskUser.as_str()),
            urlencoding::encode(prompt_id)
        );
        let binding = TaskRunnerRequestBinding::from(snapshot);
        let response = self
            .bound_request(
                &binding,
                endpoint,
                self.request_timeout,
                secret,
                super::TASK_RUNNER_MCP_SCOPE,
            )?
            .send()
            .await
            .map_err(|error| {
                ProviderCallError::provider_unavailable(format!(
                    "Task Runner Ask User resolution request failed: {error}"
                ))
            })?;
        let value = response.json::<Value>().await.map_err(|error| {
            ProviderCallError::invalid_response(format!(
                "Task Runner Ask User resolution response is invalid: {error}"
            ))
        })?;
        if value.get("pending").and_then(Value::as_bool) == Some(true) {
            return Ok(None);
        }
        let response = serde_json::from_value::<AskUserResponseSubmission>(
            value.get("response").cloned().unwrap_or(Value::Null),
        )
        .map_err(|error| {
            ProviderCallError::invalid_response(format!(
                "Task Runner Ask User response payload is invalid: {error}"
            ))
        })?;
        let kind = value
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let result = ask_user_result(kind, response);
        let _ = invocation_id;
        Ok(Some(result))
    }
}

fn ask_user_result(kind: &str, response: AskUserResponseSubmission) -> Value {
    let payload = match kind {
        "choice" => json!({
            "status": response.status,
            "selection": response.selection.unwrap_or(Value::Null),
        }),
        "mixed" => json!({
            "status": response.status,
            "values": response.values.unwrap_or_else(|| json!({})),
            "selection": response.selection.unwrap_or(Value::Null),
        }),
        _ => json!({
            "status": response.status,
            "values": response.values.unwrap_or_else(|| json!({})),
        }),
    };
    mcp_text_result(payload)
}

fn mcp_text_result(payload: Value) -> Value {
    let text = serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string());
    json!({
        "content": [{"type": "text", "text": text}],
        "_structured_result": payload,
    })
}
