// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_service::{MCP_ERROR_AUTH_REQUIRED, MCP_ERROR_INTERNAL, MCP_ERROR_INVALID_PARAMS};
use serde_json::Value;

use super::ProviderCallError;

pub(in crate::providers) fn decode_jsonrpc_response(
    bytes: &[u8],
    invocation_id: &str,
    provider_label: &str,
) -> Result<Value, ProviderCallError> {
    let envelope = serde_json::from_slice::<Value>(bytes).map_err(|err| {
        ProviderCallError::invalid_response(format!(
            "{provider_label} returned invalid JSON: {err}"
        ))
    })?;
    let object = envelope.as_object().ok_or_else(|| {
        ProviderCallError::invalid_response(format!(
            "{provider_label} returned a non-object JSON-RPC response"
        ))
    })?;
    if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Err(ProviderCallError::invalid_response(format!(
            "{provider_label} returned an invalid JSON-RPC version"
        )));
    }
    if object.get("id").and_then(Value::as_str) != Some(invocation_id) {
        return Err(ProviderCallError::invalid_response(format!(
            "{provider_label} response id does not match the invocation"
        )));
    }
    if let Some(error) = object.get("error").filter(|value| !value.is_null()) {
        let code = error
            .get("code")
            .and_then(Value::as_i64)
            .and_then(|value| i32::try_from(value).ok())
            .unwrap_or(MCP_ERROR_INTERNAL);
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("Provider returned an MCP error");
        return Err(ProviderCallError {
            code: match code {
                MCP_ERROR_AUTH_REQUIRED | MCP_ERROR_INVALID_PARAMS | MCP_ERROR_INTERNAL => code,
                _ => MCP_ERROR_INTERNAL,
            },
            message: message.to_string(),
        });
    }
    object.get("result").cloned().ok_or_else(|| {
        ProviderCallError::invalid_response(format!(
            "{provider_label} response is missing result and error"
        ))
    })
}
