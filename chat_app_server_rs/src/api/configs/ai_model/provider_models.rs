use serde_json::{json, Value};
use std::time::Duration;

use crate::models::ai_model_config::AiModelConfig;
use crate::utils::model_config::{default_base_url_for_provider, normalize_provider};

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
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|err| err.to_string())?;
    let mut last_error = None;
    for url in model_list_urls(profile.provider.as_str(), profile.base_url.as_deref()) {
        match client.get(url.as_str()).bearer_auth(api_key).send().await {
            Ok(response) => {
                let status = response.status();
                let raw_text = response.text().await.map_err(|err| err.to_string())?;
                if !status.is_success() {
                    last_error = Some(format!("{}: {}", status, raw_text));
                    continue;
                }
                let raw: Value = serde_json::from_str(raw_text.as_str())
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
