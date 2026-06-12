use std::time::Duration;

use chatos_ai_runtime::model_config::{
    default_base_url_for_provider, normalize_provider, normalize_thinking_level,
};
use serde_json::Value;
use tracing::{info, warn};

use crate::models::{now_rfc3339, ModelCatalogResponse, ModelConfigRecord, ProviderModelRecord};

use super::normalized_optional;

pub(super) fn normalize_model_provider_input(provider: &str) -> Result<String, String> {
    let raw = provider.trim();
    if raw.is_empty() {
        return Err("provider 为必填项".to_string());
    }
    let normalized = normalize_provider(raw);
    let provider = match normalized.as_str() {
        "gpt" | "openai_compatible" => "openai",
        "deepseek" => "deepseek",
        "kimi" => "kimik2",
        "custom_gateway" => "openai",
        "kiminik2" => "kimik2",
        other => other,
    };
    match provider {
        "openai" | "deepseek" | "kimik2" => Ok(provider.to_string()),
        _ => Err("provider 仅支持 openai / deepseek / kimik2".to_string()),
    }
}

pub(super) fn normalize_model_thinking_level_input(
    provider: &str,
    level: Option<String>,
) -> Result<Option<String>, String> {
    let level = level
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let Some(level) = level else {
        return Ok(None);
    };
    normalize_thinking_level(provider, Some(level.as_str()))
        .map_err(|_| "思考等级仅支持 none/auto/minimal/low/medium/high/xhigh/max".to_string())
}

pub(super) fn normalize_model_base_url_input(provider: &str, base_url: Option<String>) -> String {
    base_url
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default_base_url_for_provider(provider, "https://api.openai.com/v1"))
        .trim_end_matches('/')
        .to_string()
}

pub(super) fn normalize_model_config_record(
    mut record: ModelConfigRecord,
) -> Result<ModelConfigRecord, String> {
    let provider = normalize_model_provider_input(&record.provider)?;
    record.thinking_level =
        normalize_model_thinking_level_input(provider.as_str(), record.thinking_level.clone())?;
    record.base_url = normalize_model_base_url_input(provider.as_str(), Some(record.base_url));
    record.provider = provider;
    record.usage_scenario = normalized_optional(record.usage_scenario);
    record.instructions = normalized_optional(record.instructions);
    record.request_cwd = normalized_optional(record.request_cwd);
    Ok(record)
}

fn model_list_urls(provider: &str, base_url: &str) -> Vec<String> {
    let mut urls = vec![format!("{}/models", base_url.trim_end_matches('/'))];
    if provider == "deepseek" && base_url.ends_with("/v1") {
        let fallback = base_url.trim_end_matches("/v1");
        urls.push(format!("{fallback}/models"));
    }
    urls
}

fn read_provider_model_bool_field(item: &Value, keys: &[&str]) -> bool {
    keys.iter()
        .find_map(|key| item.get(*key).and_then(|value| value.as_bool()))
        .unwrap_or(false)
}

fn read_provider_model_i64_field(item: &Value, keys: &[&str]) -> Option<i64> {
    keys.iter()
        .find_map(|key| item.get(*key).and_then(|value| value.as_i64()))
}

fn normalize_provider_model_item(provider: &str, item: &Value) -> Option<ProviderModelRecord> {
    let id = item
        .get("id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    let supports_images = read_provider_model_bool_field(
        item,
        &["supports_images", "supports_image_in", "vision", "image"],
    );
    let supports_video =
        read_provider_model_bool_field(item, &["supports_video", "supports_video_in"]);
    let supports_reasoning =
        read_provider_model_bool_field(item, &["supports_reasoning", "reasoning"]);
    let supports_responses =
        read_provider_model_bool_field(item, &["supports_responses"]) || provider == "openai";
    Some(ProviderModelRecord {
        id,
        owned_by: item
            .get("owned_by")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned),
        context_length: read_provider_model_i64_field(
            item,
            &["context_length", "max_context_length", "max_tokens"],
        ),
        supports_images,
        supports_video,
        supports_reasoning,
        supports_responses,
        raw: Some(item.clone()),
    })
}

fn normalize_provider_models(provider: &str, raw: &Value) -> Vec<ProviderModelRecord> {
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

async fn fetch_provider_models(
    profile: &ModelConfigRecord,
) -> Result<Vec<ProviderModelRecord>, String> {
    let api_key = profile.api_key.trim();
    if api_key.is_empty() {
        return Err("当前供应商配置未提供 API Key".to_string());
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|err| err.to_string())?;
    let mut last_error = None;
    for url in model_list_urls(profile.provider.as_str(), profile.base_url.as_str()) {
        info!(
            provider = profile.provider.as_str(),
            model_config_id = profile.id.as_str(),
            model = profile.model.as_str(),
            url = url.as_str(),
            "task runner requesting provider model catalog"
        );
        match client.get(url.as_str()).bearer_auth(api_key).send().await {
            Ok(response) => {
                let status = response.status();
                let raw_text = response.text().await.map_err(|err| err.to_string())?;
                if !status.is_success() {
                    warn!(
                        provider = profile.provider.as_str(),
                        model_config_id = profile.id.as_str(),
                        model = profile.model.as_str(),
                        url = url.as_str(),
                        status = status.as_u16(),
                        response_body = raw_text.as_str(),
                        "task runner provider model catalog request failed"
                    );
                    last_error = Some(format!("{status}: {raw_text}"));
                    continue;
                }
                let raw: Value = serde_json::from_str(raw_text.as_str())
                    .map_err(|err| format!("解析模型列表失败: {err}"))?;
                let models = normalize_provider_models(profile.provider.as_str(), &raw);
                info!(
                    provider = profile.provider.as_str(),
                    model_config_id = profile.id.as_str(),
                    model = profile.model.as_str(),
                    url = url.as_str(),
                    model_count = models.len(),
                    "task runner received provider model catalog"
                );
                return Ok(models);
            }
            Err(err) => {
                let err_text = err.to_string();
                warn!(
                    provider = profile.provider.as_str(),
                    model_config_id = profile.id.as_str(),
                    model = profile.model.as_str(),
                    url = url.as_str(),
                    error = err_text.as_str(),
                    "task runner provider model catalog request errored"
                );
                last_error = Some(err_text);
            }
        }
    }
    Err(last_error.unwrap_or_else(|| "获取模型列表失败".to_string()))
}

fn fallback_model_list(profile: &ModelConfigRecord) -> Vec<ProviderModelRecord> {
    let model = profile.model.trim();
    if model.is_empty() {
        return Vec::new();
    }
    vec![ProviderModelRecord {
        id: model.to_string(),
        owned_by: Some(profile.provider.clone()),
        context_length: None,
        supports_images: false,
        supports_video: false,
        supports_reasoning: false,
        supports_responses: profile.supports_responses,
        raw: None,
    }]
}

pub(super) async fn fetch_model_catalog_for_record(
    provider_config_id: Option<String>,
    profile: &ModelConfigRecord,
) -> ModelCatalogResponse {
    match fetch_provider_models(profile).await {
        Ok(models) => ModelCatalogResponse {
            provider_config_id,
            provider: profile.provider.clone(),
            base_url: profile.base_url.clone(),
            source: "live".to_string(),
            fetched_at: Some(now_rfc3339()),
            models,
            error: None,
        },
        Err(error) => ModelCatalogResponse {
            provider_config_id,
            provider: profile.provider.clone(),
            base_url: profile.base_url.clone(),
            source: "fallback".to_string(),
            fetched_at: None,
            models: fallback_model_list(profile),
            error: Some(error),
        },
    }
}
