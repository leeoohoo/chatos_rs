// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

use super::{ProviderCallError, ProviderCancelOutcome};

pub(crate) fn decode_cancel_notification_response(
    status: reqwest::StatusCode,
    bytes: &[u8],
    provider_label: &str,
) -> Result<ProviderCancelOutcome, ProviderCallError> {
    if !status.is_success() {
        return Err(ProviderCallError::provider_unavailable(format!(
            "{provider_label} rejected cancellation with HTTP {}",
            status.as_u16()
        )));
    }
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Ok(ProviderCancelOutcome::CancelRequested);
    }
    let envelope = serde_json::from_slice::<Value>(bytes).map_err(|error| {
        ProviderCallError::invalid_response(format!(
            "{provider_label} returned invalid cancellation JSON: {error}"
        ))
    })?;
    if envelope.pointer("/error/code").and_then(Value::as_i64)
        == Some(i64::from(chatos_mcp_service::MCP_ERROR_METHOD_NOT_FOUND))
    {
        return Ok(ProviderCancelOutcome::NotSupported);
    }
    match envelope
        .pointer("/result/status")
        .and_then(Value::as_str)
        .map(str::trim)
    {
        Some("cancelled" | "invocation_cancelled") => Ok(ProviderCancelOutcome::Cancelled),
        _ => Ok(ProviderCancelOutcome::CancelRequested),
    }
}
