#[cfg(test)]
use std::future::Future;

use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::utils::abort_registry;

pub(crate) fn truncate_log(value: &str, max_len: usize) -> String {
    if value.len() <= max_len {
        return value.to_string();
    }

    let mut out = value[..max_len].to_string();
    out.push_str("...[truncated]");
    out
}

pub(crate) fn build_abort_token(
    session_id: Option<&str>,
    turn_id: Option<&str>,
) -> Option<CancellationToken> {
    let session_id = session_id
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let token = CancellationToken::new();
    abort_registry::set_controller(session_id, turn_id, token.clone());
    Some(token)
}

#[cfg(test)]
pub(crate) fn format_error_response(status: reqwest::StatusCode, raw: &str) -> String {
    format!("status {}: {}", status, truncate_log(raw, 2000))
}

pub(crate) fn validate_request_payload_size(
    payload: &Value,
    env_key: &str,
    explicit_max_bytes: Option<usize>,
) -> Result<(), String> {
    let bytes = serde_json::to_vec(payload).map_err(|err| err.to_string())?;
    let max_bytes = request_payload_max_bytes(env_key, explicit_max_bytes);
    if bytes.len() > max_bytes {
        return Err(format!(
            "request body too large (precheck): payload_bytes={}, limit_bytes={}",
            bytes.len(),
            max_bytes
        ));
    }
    Ok(())
}

fn request_payload_max_bytes(env_key: &str, explicit_max_bytes: Option<usize>) -> usize {
    if let Some(value) = explicit_max_bytes.filter(|value| *value > 0) {
        return value;
    }

    std::env::var(env_key)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(
            crate::core::ai_settings::request_body_limit_bytes_for_attachment_total(
                crate::core::ai_settings::DEFAULT_ATTACHMENT_TOTAL_MAX_BYTES,
            ),
        )
}

#[cfg(test)]
pub(crate) async fn await_with_optional_abort<F, T, E>(
    future: F,
    token: Option<CancellationToken>,
) -> Result<T, String>
where
    F: Future<Output = Result<T, E>>,
    E: ToString,
{
    if let Some(token) = token {
        tokio::select! {
            _ = token.cancelled() => Err("aborted".to_string()),
            value = future => value.map_err(|err| err.to_string()),
        }
    } else {
        future.await.map_err(|err| err.to_string())
    }
}
