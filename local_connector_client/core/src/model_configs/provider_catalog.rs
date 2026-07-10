// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

use super::types::LocalProviderModelRecord;

pub(super) fn normalize_provider(value: Option<String>) -> String {
    match value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("gpt")
        .to_ascii_lowercase()
        .replace('-', "_")
        .as_str()
    {
        "openai" | "gpt" => "gpt".to_string(),
        "deepseek" => "deepseek".to_string(),
        "kimi" | "kimik2" | "moonshot" => "kimi".to_string(),
        "minimax" => "minimax".to_string(),
        "openai_compatible" | "compatible" => "openai_compatible".to_string(),
        other => other.to_string(),
    }
}

pub(super) fn default_base_url_for_provider(provider: &str) -> String {
    match normalize_provider(Some(provider.to_string())).as_str() {
        "deepseek" => "https://api.deepseek.com/v1".to_string(),
        "kimi" => "https://api.moonshot.cn/v1".to_string(),
        "minimax" => "https://api.minimax.chat/v1".to_string(),
        _ => "https://api.openai.com/v1".to_string(),
    }
}

pub(super) fn runtime_provider_for_model(provider: &str, base_url: &str) -> String {
    let provider = normalize_provider(Some(provider.to_string()));
    if provider == "gpt"
        && !base_url
            .trim()
            .to_ascii_lowercase()
            .contains("api.openai.com")
    {
        "openai_compatible".to_string()
    } else {
        provider
    }
}

pub(super) async fn fetch_provider_models(
    http_client: &reqwest::Client,
    provider: &str,
    base_url: &str,
    api_key: &str,
) -> Result<Vec<LocalProviderModelRecord>, String> {
    let mut errors = Vec::new();
    for url in model_list_urls(provider, base_url) {
        match http_client
            .get(url.as_str())
            .bearer_auth(api_key)
            .send()
            .await
        {
            Ok(response) => {
                let status = response.status();
                let text = response.text().await.unwrap_or_default();
                if !status.is_success() {
                    let message = format!(
                        "模型列表接口 {} 返回 {}: {}",
                        url,
                        status.as_u16(),
                        preview_text(text.as_str(), 800)
                    );
                    if matches!(
                        status,
                        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN
                    ) {
                        return Err(message);
                    }
                    errors.push(message);
                    continue;
                }
                let raw = serde_json::from_str::<Value>(text.as_str())
                    .map_err(|err| format!("解析模型列表失败: {err}"))?;
                return Ok(normalize_provider_models(provider, &raw));
            }
            Err(err) => {
                errors.push(format!("请求模型列表接口 {url} 失败: {err}"));
            }
        }
    }
    Err(if errors.is_empty() {
        "获取模型列表失败".to_string()
    } else {
        errors.join("；")
    })
}

fn model_list_urls(provider: &str, base_url: &str) -> Vec<String> {
    let base_url = base_url.trim().trim_end_matches('/');
    let mut urls = Vec::new();
    push_unique_url(&mut urls, format!("{base_url}/models"));
    if base_url.ends_with("/v1") {
        let fallback = base_url.trim_end_matches("/v1");
        push_unique_url(&mut urls, format!("{fallback}/models"));
    }
    if normalize_provider(Some(provider.to_string())) == "deepseek" && !base_url.ends_with("/v1") {
        push_unique_url(&mut urls, format!("{base_url}/v1/models"));
    }
    urls
}

fn push_unique_url(urls: &mut Vec<String>, url: String) {
    if !urls.iter().any(|existing| existing == &url) {
        urls.push(url);
    }
}

fn normalize_provider_models(provider: &str, raw: &Value) -> Vec<LocalProviderModelRecord> {
    raw.get("data")
        .and_then(Value::as_array)
        .or_else(|| raw.as_array())
        .into_iter()
        .flatten()
        .filter_map(|item| normalize_provider_model_item(provider, item))
        .collect()
}

fn normalize_provider_model_item(provider: &str, item: &Value) -> Option<LocalProviderModelRecord> {
    let id = item
        .get("id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    let provider = normalize_provider(Some(provider.to_string()));
    Some(LocalProviderModelRecord {
        id,
        owned_by: item
            .get("owned_by")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        context_length: read_provider_model_i64_field(
            item,
            &["context_length", "max_context_length", "max_tokens"],
        ),
        supports_images: read_provider_model_bool_field(
            item,
            &["supports_images", "supports_image_in", "vision", "image"],
        ),
        supports_reasoning: read_provider_model_bool_field(
            item,
            &["supports_reasoning", "reasoning"],
        ),
        supports_responses: read_provider_model_bool_field(item, &["supports_responses"])
            || provider == "gpt",
    })
}

fn read_provider_model_bool_field(item: &Value, keys: &[&str]) -> bool {
    keys.iter()
        .find_map(|key| item.get(*key).and_then(Value::as_bool))
        .unwrap_or(false)
}

fn read_provider_model_i64_field(item: &Value, keys: &[&str]) -> Option<i64> {
    keys.iter()
        .find_map(|key| item.get(*key).and_then(Value::as_i64))
}

pub(super) fn fallback_model_list(model: Option<&str>) -> Vec<LocalProviderModelRecord> {
    model
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|id| {
            vec![LocalProviderModelRecord {
                id: id.to_string(),
                owned_by: None,
                context_length: None,
                supports_images: false,
                supports_reasoning: false,
                supports_responses: false,
            }]
        })
        .unwrap_or_default()
}

fn preview_text(value: &str, max_chars: usize) -> String {
    let mut output = value.trim().chars().take(max_chars).collect::<String>();
    if value.trim().chars().count() > max_chars {
        output.push_str("...");
    }
    output
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn normalizes_openai_style_model_catalog() {
        let raw = json!({
            "data": [
                {
                    "id": "gpt-4.1",
                    "owned_by": "openai",
                    "context_length": 128000,
                    "supports_images": true,
                    "supports_reasoning": false,
                    "supports_responses": true
                }
            ]
        });

        let models = normalize_provider_models("gpt", &raw);

        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "gpt-4.1");
        assert_eq!(models[0].owned_by.as_deref(), Some("openai"));
        assert_eq!(models[0].context_length, Some(128000));
        assert!(models[0].supports_images);
        assert!(models[0].supports_responses);
        assert!(!models[0].supports_reasoning);
    }

    #[test]
    fn fallback_model_list_keeps_current_selection() {
        let models = fallback_model_list(Some("deepseek-chat"));

        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "deepseek-chat");
        assert!(!models[0].supports_images);
        assert!(!models[0].supports_reasoning);
    }

    #[test]
    fn model_list_urls_try_root_models_for_v1_base_url() {
        assert_eq!(
            model_list_urls("gpt", "https://newapi.example.com/v1"),
            vec![
                "https://newapi.example.com/v1/models".to_string(),
                "https://newapi.example.com/models".to_string(),
            ]
        );
    }

    #[test]
    fn model_list_urls_try_v1_models_for_deepseek_root_base_url() {
        assert_eq!(
            model_list_urls("deepseek", "https://api.deepseek.com"),
            vec![
                "https://api.deepseek.com/models".to_string(),
                "https://api.deepseek.com/v1/models".to_string(),
            ]
        );
    }
}
