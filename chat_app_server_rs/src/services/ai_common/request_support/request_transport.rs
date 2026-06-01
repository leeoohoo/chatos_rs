use std::future::Future;

use reqwest::RequestBuilder;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::utils::abort_registry;

pub(crate) fn normalize_reasoning_effort(
    provider: Option<&str>,
    level: Option<&str>,
) -> Option<String> {
    let provider = provider.unwrap_or("gpt");
    crate::utils::model_config::normalize_thinking_level(provider, level).unwrap_or_default()
}

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

fn build_bearer_post_request(
    client: &reqwest::Client,
    url: &str,
    api_key: &str,
    force_identity_encoding: bool,
) -> RequestBuilder {
    let mut req = client.post(url).bearer_auth(api_key);
    if force_identity_encoding {
        req = req
            .header(reqwest::header::ACCEPT_ENCODING, "identity")
            .header(reqwest::header::CONNECTION, "close")
            .version(reqwest::Version::HTTP_11);
    }
    req
}

pub(crate) async fn send_bearer_json_request(
    client: &reqwest::Client,
    url: &str,
    api_key: &str,
    payload: &Value,
    token: Option<CancellationToken>,
    force_identity_encoding: bool,
) -> Result<reqwest::Response, String> {
    let req = build_bearer_post_request(client, url, api_key, force_identity_encoding);
    await_with_optional_abort(req.json(payload).send(), token).await
}

pub(crate) async fn read_error_response_text(
    response: reqwest::Response,
) -> Result<String, String> {
    let status = response.status();
    let raw = response.text().await.map_err(|err| err.to_string())?;
    Ok(format_error_response(status, raw.as_str()))
}

pub(crate) fn format_error_response(status: reqwest::StatusCode, raw: &str) -> String {
    format!("status {}: {}", status, truncate_log(raw, 2000))
}

pub(crate) fn validate_request_payload_size(payload: &Value, env_key: &str) -> Result<(), String> {
    let bytes = serde_json::to_vec(payload).map_err(|err| err.to_string())?;
    let max_bytes = request_payload_max_bytes(env_key);
    if bytes.len() > max_bytes {
        return Err(format!(
            "request body too large (precheck): payload_bytes={}, limit_bytes={}",
            bytes.len(),
            max_bytes
        ));
    }
    Ok(())
}

fn request_payload_max_bytes(env_key: &str) -> usize {
    std::env::var(env_key)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(1_500_000)
}

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
