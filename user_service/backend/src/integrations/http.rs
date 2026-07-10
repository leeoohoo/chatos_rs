// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

use crate::state::AppState;

pub(super) fn build_client(state: &AppState) -> Result<reqwest::Client, String> {
    build_client_with_timeout(state.config.downstream_request_timeout_ms)
}

pub(super) fn build_client_with_timeout(timeout_ms: i64) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(timeout_ms.max(300) as u64))
        .build()
        .map_err(|err| err.to_string())
}

pub(super) fn normalized_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub(super) fn normalized_url(value: Option<&str>) -> Option<String> {
    normalized_text(value).map(|value| value.trim_end_matches('/').to_string())
}

pub(super) fn extract_error_message(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("error")
                .and_then(Value::as_str)
                .or_else(|| value.get("message").and_then(Value::as_str))
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| body.trim().to_string())
}
