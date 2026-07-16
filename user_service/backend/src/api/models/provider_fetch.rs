// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_service_runtime::http_body::{
    read_response_preview_text_limited, read_response_text_limited,
    DEFAULT_RESPONSE_BODY_LIMIT_BYTES, ERROR_BODY_PREVIEW_LIMIT_BYTES,
};
use chatos_service_runtime::{build_http_client, HttpClientTimeouts};
use reqwest::Method;
use serde_json::Value;
use tracing::{error, info, warn};

pub(super) async fn fetch_provider_model_names(
    provider: &str,
    base_url: Option<&str>,
    api_key: &str,
    timeout_ms: i64,
) -> Result<Vec<String>, String> {
    let base_url = base_url
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default_base_url_for_provider(provider))
        .trim_end_matches('/')
        .to_string();
    let endpoint = format!("{base_url}/models");
    let started_at = std::time::Instant::now();
    info!(
        provider = %provider,
        base_url = %base_url,
        endpoint = %endpoint,
        timeout_ms = timeout_ms.max(300),
        "provider_models.fetch.start"
    );
    let client = build_http_client(HttpClientTimeouts::new(std::time::Duration::from_millis(
        timeout_ms.max(300) as u64,
    )))
    .map_err(|err| {
        error!(
            provider = %provider,
            base_url = %base_url,
            endpoint = %endpoint,
            error = %err,
            "provider_models.fetch.client_build_failed"
        );
        err.to_string()
    })?;
    let mut request = client.request(Method::GET, endpoint);
    let api_key = api_key.trim();
    if !api_key.is_empty() {
        request = request.bearer_auth(api_key);
    }
    let response = request.send().await.map_err(|err| {
        warn!(
            provider = %provider,
            base_url = %base_url,
            elapsed_ms = started_at.elapsed().as_millis(),
            error = %err,
            "provider_models.fetch.request_failed"
        );
        err.to_string()
    })?;
    let status = response.status();
    if !status.is_success() {
        let body = read_response_preview_text_limited(response, ERROR_BODY_PREVIEW_LIMIT_BYTES)
            .await
            .unwrap_or_else(|err| format!("[response body unavailable: {err}]"));
        warn!(
            provider = %provider,
            base_url = %base_url,
            status = status.as_u16(),
            elapsed_ms = started_at.elapsed().as_millis(),
            body_preview = %log_preview(body.as_str(), 800),
            "provider_models.fetch.http_error"
        );
        return Err(format!(
            "provider models request failed: {} {}",
            status.as_u16(),
            body.trim()
        ));
    }
    let body = read_response_text_limited(response, DEFAULT_RESPONSE_BODY_LIMIT_BYTES).await?;
    let payload: Value = serde_json::from_str(body.as_str()).map_err(|err| {
        warn!(
            provider = %provider,
            base_url = %base_url,
            status = status.as_u16(),
            elapsed_ms = started_at.elapsed().as_millis(),
            body_preview = %log_preview(body.as_str(), 800),
            error = %err,
            "provider_models.fetch.parse_failed"
        );
        err.to_string()
    })?;
    let model_names = extract_model_names(&payload);
    info!(
        provider = %provider,
        base_url = %base_url,
        status = status.as_u16(),
        elapsed_ms = started_at.elapsed().as_millis(),
        model_count = model_names.len(),
        "provider_models.fetch.success"
    );
    Ok(model_names)
}

fn log_preview(value: &str, max_chars: usize) -> String {
    let trimmed = value.trim();
    let mut out = String::new();
    for (index, ch) in trimmed.chars().enumerate() {
        if index >= max_chars {
            out.push_str("...");
            break;
        }
        out.push(ch);
    }
    out
}

fn extract_model_names(payload: &Value) -> Vec<String> {
    let items = payload
        .get("data")
        .and_then(Value::as_array)
        .or_else(|| payload.as_array());
    let mut out = Vec::new();
    if let Some(items) = items {
        for item in items {
            let id = item
                .get("id")
                .and_then(Value::as_str)
                .or_else(|| item.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty());
            if let Some(id) = id {
                let id = id.to_string();
                if !out.iter().any(|existing| existing == &id) {
                    out.push(id);
                }
            }
        }
    }
    out
}

fn default_base_url_for_provider(provider: &str) -> &'static str {
    match provider {
        "deepseek" => "https://api.deepseek.com",
        "kimi" => "https://api.moonshot.ai/v1",
        "minimax" => "https://api.minimax.chat/v1",
        _ => "https://api.openai.com/v1",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extracts_openai_style_model_names_without_duplicates() {
        assert_eq!(
            extract_model_names(&json!({
                "data": [{"id": "gpt-5"}, {"id": "gpt-5"}, {"id": "gpt-4.1"}]
            })),
            vec!["gpt-5".to_string(), "gpt-4.1".to_string()]
        );
    }
}
