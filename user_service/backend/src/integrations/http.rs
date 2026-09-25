// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_service_runtime::{http_client_builder, HttpClientTimeouts};
use serde_json::Value;

pub(super) fn build_harness_client_with_timeout(
    timeout_ms: i64,
) -> Result<reqwest::Client, String> {
    http_client_builder(HttpClientTimeouts::new(std::time::Duration::from_millis(
        timeout_ms.max(300) as u64,
    )))
    // Harness requests carry passwords or bearer tokens. Never replay them at
    // a redirect destination, even on the same origin.
    .redirect(reqwest::redirect::Policy::none())
    .build()
    .map_err(harness_client_build_error)
}

fn harness_client_build_error(_error: impl std::fmt::Display) -> String {
    // Client diagnostics are not part of the public or persisted error
    // contract. Keep this boundary fixed even if a dependency later includes
    // configuration or credential material in its Display implementation.
    "build harness client failed".to_string()
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

#[cfg(test)]
mod tests {
    use super::harness_client_build_error;

    #[test]
    fn harness_client_build_errors_are_fixed_and_credential_free() {
        let credential = "test-only-harness-client-credential";
        let error = harness_client_build_error(format!("builder rejected {credential}"));
        assert_eq!(error, "build harness client failed");
        assert!(!error.contains(credential));
    }
}
