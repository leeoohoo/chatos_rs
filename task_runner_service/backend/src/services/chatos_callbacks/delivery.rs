// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::http_body::{read_response_text_limited_or_message, ERROR_BODY_PREVIEW_LIMIT_BYTES};
use reqwest::StatusCode;
use std::sync::OnceLock;

static CHATOS_CALLBACK_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
const CHATOS_CALLBACK_RETRY_DELAYS_MS: [u64; 3] = [0, 250, 750];

pub(super) async fn send_chatos_task_callback(
    config: AppConfig,
    payload: ChatosTaskCallbackPayload,
) -> Result<(), String> {
    let Some(url) = config.chatos_callback_url.clone() else {
        return Err("TASK_RUNNER_CHATOS_CALLBACK_URL not configured".to_string());
    };
    let mut last_error = None;
    for (attempt_index, delay_ms) in CHATOS_CALLBACK_RETRY_DELAYS_MS.into_iter().enumerate() {
        if delay_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
        }
        let mut request = chatos_callback_client()
            .post(url.as_str())
            .timeout(config.callback_timeout)
            .json(&payload);
        if let Some(secret) = config.chatos_callback_secret.as_deref() {
            request = request.header("X-Task-Runner-Callback-Secret", secret);
        }
        match request.send().await {
            Ok(response) if response.status().is_success() => return Ok(()),
            Ok(response) => {
                let status = response.status();
                let body =
                    read_response_text_limited_or_message(response, ERROR_BODY_PREVIEW_LIMIT_BYTES)
                        .await;
                let error = format!("callback request failed: {status} {body}");
                if !callback_status_is_retryable(status) {
                    return Err(error);
                }
                last_error = Some(error);
            }
            Err(err) => {
                last_error = Some(err.to_string());
            }
        }
        if attempt_index + 1 < CHATOS_CALLBACK_RETRY_DELAYS_MS.len() {
            warn!(
                attempt = attempt_index + 1,
                max_attempts = CHATOS_CALLBACK_RETRY_DELAYS_MS.len(),
                "task callback delivery failed; retrying"
            );
        }
    }
    Err(last_error.unwrap_or_else(|| "callback request failed".to_string()))
}

fn chatos_callback_client() -> &'static reqwest::Client {
    CHATOS_CALLBACK_CLIENT.get_or_init(reqwest::Client::new)
}

fn callback_status_is_retryable(status: StatusCode) -> bool {
    status.is_server_error()
        || status == StatusCode::REQUEST_TIMEOUT
        || status == StatusCode::TOO_MANY_REQUESTS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn callback_retries_only_transient_http_failures() {
        assert!(callback_status_is_retryable(StatusCode::BAD_GATEWAY));
        assert!(callback_status_is_retryable(
            StatusCode::SERVICE_UNAVAILABLE
        ));
        assert!(callback_status_is_retryable(StatusCode::REQUEST_TIMEOUT));
        assert!(callback_status_is_retryable(StatusCode::TOO_MANY_REQUESTS));
        assert!(!callback_status_is_retryable(StatusCode::BAD_REQUEST));
        assert!(!callback_status_is_retryable(StatusCode::UNAUTHORIZED));
        assert!(!callback_status_is_retryable(StatusCode::NOT_FOUND));
    }
}
