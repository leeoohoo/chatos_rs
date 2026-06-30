use bytes::BytesMut;
use futures::StreamExt;
use serde_json::{json, Value};
use std::sync::OnceLock;
use std::time::Duration;

use crate::models::ai_model_config::AiModelConfig;
use crate::utils::model_config::{default_base_url_for_provider, normalize_provider};

static PROVIDER_MODELS_HTTP_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

const PROVIDER_MODELS_RESPONSE_LIMIT_BYTES: usize = 2 * 1024 * 1024;
const PROVIDER_MODELS_ERROR_BODY_PREVIEW_BYTES: usize = 16 * 1024;

pub(super) fn normalize_base_url_for_models(provider: &str, base_url: Option<&str>) -> String {
    base_url
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| default_base_url_for_provider(provider, "https://api.openai.com/v1"))
        .trim_end_matches('/')
        .to_string()
}

fn model_list_urls(provider: &str, base_url: Option<&str>) -> Vec<String> {
    let base = normalize_base_url_for_models(provider, base_url);
    let mut urls = vec![format!("{base}/models")];
    if normalize_provider(provider) == "deepseek" && base.ends_with("/v1") {
        let fallback = base.trim_end_matches("/v1");
        urls.push(format!("{fallback}/models"));
    }
    urls
}

fn read_bool_field(item: &Value, keys: &[&str]) -> bool {
    keys.iter()
        .find_map(|key| item.get(*key).and_then(|value| value.as_bool()))
        .unwrap_or(false)
}

fn read_i64_field(item: &Value, keys: &[&str]) -> Option<i64> {
    keys.iter()
        .find_map(|key| item.get(*key).and_then(|value| value.as_i64()))
}

fn normalize_provider_model_item(provider: &str, item: &Value) -> Option<Value> {
    let id = item
        .get("id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let provider = normalize_provider(provider);
    let supports_images = read_bool_field(
        item,
        &["supports_images", "supports_image_in", "vision", "image"],
    );
    let supports_video = read_bool_field(item, &["supports_video", "supports_video_in"]);
    let supports_reasoning = read_bool_field(item, &["supports_reasoning", "reasoning"]);
    let supports_responses = read_bool_field(item, &["supports_responses"])
        || provider == "gpt"
        || provider == "openai_compatible";
    Some(json!({
        "id": id,
        "owned_by": item.get("owned_by").and_then(|value| value.as_str()),
        "context_length": read_i64_field(item, &["context_length", "max_context_length", "max_tokens"]),
        "supports_images": supports_images,
        "supports_video": supports_video,
        "supports_reasoning": supports_reasoning,
        "supports_responses": supports_responses,
        "raw": item,
    }))
}

fn normalize_provider_models(provider: &str, raw: &Value) -> Vec<Value> {
    let items = raw
        .get("data")
        .and_then(|value| value.as_array())
        .or_else(|| raw.as_array())
        .cloned()
        .unwrap_or_default();
    items
        .iter()
        .filter_map(|item| normalize_provider_model_item(provider, item))
        .collect()
}

pub(super) async fn fetch_provider_models(profile: &AiModelConfig) -> Result<Vec<Value>, String> {
    let Some(api_key) = profile
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Err("当前供应商配置未保存 API Key".to_string());
    };
    let mut last_error = None;
    for url in model_list_urls(profile.provider.as_str(), profile.base_url.as_deref()) {
        match provider_models_http_client()
            .get(url.as_str())
            .bearer_auth(api_key)
            .timeout(Duration::from_secs(20))
            .send()
            .await
        {
            Ok(response) => {
                let status = response.status();
                if !status.is_success() {
                    let raw_text = read_provider_models_body_limited(
                        response,
                        PROVIDER_MODELS_ERROR_BODY_PREVIEW_BYTES,
                    )
                    .await
                    .map(|bytes| String::from_utf8_lossy(bytes.as_ref()).into_owned())
                    .unwrap_or_else(|err| err);
                    last_error = Some(format!("{}: {}", status, raw_text));
                    continue;
                }
                let raw_body = read_provider_models_body_limited(
                    response,
                    PROVIDER_MODELS_RESPONSE_LIMIT_BYTES,
                )
                .await?;
                let raw: Value = serde_json::from_slice(raw_body.as_ref())
                    .map_err(|err| format!("解析模型列表失败: {err}"))?;
                return Ok(normalize_provider_models(profile.provider.as_str(), &raw));
            }
            Err(err) => {
                last_error = Some(err.to_string());
            }
        }
    }
    Err(last_error.unwrap_or_else(|| "获取模型列表失败".to_string()))
}

fn provider_models_http_client() -> &'static reqwest::Client {
    PROVIDER_MODELS_HTTP_CLIENT.get_or_init(reqwest::Client::new)
}

async fn read_provider_models_body_limited(
    response: reqwest::Response,
    limit_bytes: usize,
) -> Result<bytes::Bytes, String> {
    if let Some(content_length) = response.content_length() {
        ensure_provider_models_body_within_limit(content_length as usize, limit_bytes)?;
    }

    let mut body = BytesMut::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|err| err.to_string())?;
        let next_len = body.len().saturating_add(chunk.len());
        ensure_provider_models_body_within_limit(next_len, limit_bytes)?;
        body.extend_from_slice(chunk.as_ref());
    }
    Ok(body.freeze())
}

fn ensure_provider_models_body_within_limit(
    actual_bytes: usize,
    limit_bytes: usize,
) -> Result<(), String> {
    if actual_bytes > limit_bytes {
        return Err(format!(
            "provider model list response exceeded limit: {actual_bytes} bytes > {limit_bytes} bytes"
        ));
    }
    Ok(())
}

pub(super) fn fallback_model_list(profile: &AiModelConfig) -> Vec<Value> {
    let model = profile.model.trim();
    if model.is_empty() {
        return Vec::new();
    }
    vec![json!({
        "id": model,
        "owned_by": profile.provider,
        "context_length": null,
        "supports_images": profile.supports_images,
        "supports_video": false,
        "supports_reasoning": profile.supports_reasoning,
        "supports_responses": profile.supports_responses,
        "raw": null,
    })]
}

#[cfg(test)]
mod tests {
    use super::ensure_provider_models_body_within_limit;

    #[test]
    fn provider_models_body_limit_accepts_boundary_size() {
        assert!(ensure_provider_models_body_within_limit(1024, 1024).is_ok());
    }

    #[test]
    fn provider_models_body_limit_rejects_oversized_body() {
        let err = ensure_provider_models_body_within_limit(1025, 1024)
            .expect_err("oversized body should fail");

        assert!(err.contains("exceeded limit"));
        assert!(err.contains("1025 bytes > 1024 bytes"));
    }
}
