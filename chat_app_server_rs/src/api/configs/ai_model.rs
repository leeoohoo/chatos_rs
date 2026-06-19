use axum::Json;
use axum::extract::{Path, Query};
use axum::http::StatusCode;
use serde_json::{Value, json};
use std::time::Duration;

use crate::core::ai_model_config_access::{
    ensure_owned_ai_model_config, map_ai_model_config_access_error,
};
use crate::core::auth::AuthUser;
use crate::models::ai_model_config::AiModelConfig;
use crate::repositories::ai_model_configs;
use crate::utils::model_config::{
    default_base_url_for_provider, normalize_provider, normalize_thinking_level,
};

use super::{AiModelConfigRequest, UserQuery};

fn normalize_provider_input(provider: Option<String>) -> Result<String, String> {
    let raw = provider.unwrap_or_else(|| "gpt".to_string());
    let provider = normalize_provider(&raw);

    match provider.as_str() {
        "gpt" | "deepseek" | "kimi" | "minimax" | "openai_compatible" => Ok(provider),
        _ => Err("provider 仅支持 gpt / deepseek / kimi / minimax / openai_compatible".to_string()),
    }
}

fn normalize_thinking_level_input(
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

fn normalize_optional_input(value: Option<String>) -> Option<String> {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
}

fn resolve_api_key_input(
    req: &AiModelConfigRequest,
    existing_api_key: Option<String>,
    require_api_key: bool,
) -> Result<Option<String>, String> {
    let provided_api_key = normalize_optional_input(req.api_key.clone());
    let clear_api_key = req.clear_api_key.unwrap_or(false);
    let resolved_api_key = if clear_api_key {
        None
    } else {
        provided_api_key.or(existing_api_key)
    };

    if require_api_key
        && resolved_api_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none()
    {
        return Err("api_key 为必填项".to_string());
    }

    Ok(resolved_api_key)
}

fn to_response_value(cfg: &AiModelConfig) -> Value {
    json!({
        "id": cfg.id,
        "name": cfg.name,
        "provider": cfg.provider,
        "model": cfg.model,
        "model_name": cfg.model,
        "thinking_level": cfg.thinking_level,
        "has_api_key": cfg.has_api_key
            || cfg.api_key
                .as_deref()
                .map(str::trim)
                .is_some_and(|value| !value.is_empty()),
        "base_url": cfg.base_url,
        "enabled": cfg.enabled,
        "supports_images": cfg.supports_images,
        "supports_reasoning": cfg.supports_reasoning,
        "supports_responses": cfg.supports_responses,
        "created_at": cfg.created_at,
        "updated_at": cfg.updated_at
    })
}

fn normalize_base_url_for_models(provider: &str, base_url: Option<&str>) -> String {
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

async fn fetch_provider_models(profile: &AiModelConfig) -> Result<Vec<Value>, String> {
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

fn fallback_model_list(profile: &AiModelConfig) -> Vec<Value> {
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

fn build_model_config(
    user_id: String,
    id: String,
    req: AiModelConfigRequest,
    existing_api_key: Option<String>,
    require_api_key: bool,
) -> Result<AiModelConfig, String> {
    let Some(name) = normalize_optional_input(req.name.clone()) else {
        return Err("name 为必填项".to_string());
    };
    let Some(model) = normalize_optional_input(req.model.clone()) else {
        return Err("model 为必填项".to_string());
    };

    let provider = normalize_provider_input(req.provider.clone())?;
    let thinking_level =
        normalize_thinking_level_input(provider.as_str(), req.thinking_level.clone())?;
    let api_key = resolve_api_key_input(&req, existing_api_key, require_api_key)?;
    let has_api_key = api_key
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| !value.is_empty());

    Ok(AiModelConfig {
        id,
        user_id: Some(user_id),
        name,
        provider,
        model,
        base_url: normalize_optional_input(req.base_url),
        api_key,
        has_api_key,
        enabled: req.enabled.unwrap_or(true),
        thinking_level,
        supports_images: req.supports_images.unwrap_or(false),
        supports_reasoning: req.supports_reasoning.unwrap_or(false),
        supports_responses: req.supports_responses.unwrap_or(false),
        created_at: crate::core::time::now_rfc3339(),
        updated_at: crate::core::time::now_rfc3339(),
    })
}

pub(super) async fn list_ai_model_configs(
    auth: AuthUser,
    Query(query): Query<UserQuery>,
) -> (StatusCode, Json<Value>) {
    if query
        .user_id
        .as_deref()
        .is_some_and(|value| value != auth.user_id.as_str())
    {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"error": "user_id 与登录用户不一致"})),
        );
    }

    match ai_model_configs::list_ai_model_configs(Some(auth.user_id.as_str())).await {
        Ok(items) => {
            let out = items
                .into_iter()
                .map(|item| to_response_value(&item))
                .collect::<Vec<_>>();
            (StatusCode::OK, Json(Value::Array(out)))
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "获取 AI 模型配置失败", "detail": err})),
        ),
    }
}

pub(super) async fn create_ai_model_config(
    auth: AuthUser,
    Json(req): Json<AiModelConfigRequest>,
) -> (StatusCode, Json<Value>) {
    let id = req
        .id
        .clone()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let config = match build_model_config(auth.user_id.clone(), id, req, None, true) {
        Ok(config) => config,
        Err(err) => return (StatusCode::BAD_REQUEST, Json(json!({"error": err}))),
    };

    match ai_model_configs::create_ai_model_config(&config).await {
        Ok(item) => (StatusCode::CREATED, Json(to_response_value(&item))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "创建 AI 模型配置失败", "detail": err})),
        ),
    }
}

pub(super) async fn update_ai_model_config(
    auth: AuthUser,
    Path(config_id): Path<String>,
    Json(req): Json<AiModelConfigRequest>,
) -> (StatusCode, Json<Value>) {
    let existing = match ensure_owned_ai_model_config(&config_id, &auth).await {
        Ok(item) => item,
        Err(err) => return map_ai_model_config_access_error(err),
    };
    let config = match build_model_config(
        auth.user_id.clone(),
        existing.id.clone(),
        req,
        existing.api_key.clone(),
        false,
    ) {
        Ok(mut config) => {
            config.created_at = existing.created_at;
            config.updated_at = crate::core::time::now_rfc3339();
            config
        }
        Err(err) => return (StatusCode::BAD_REQUEST, Json(json!({"error": err}))),
    };

    match ai_model_configs::update_ai_model_config(config_id.as_str(), &config).await {
        Ok(()) => (StatusCode::OK, Json(to_response_value(&config))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "更新 AI 模型配置失败", "detail": err})),
        ),
    }
}

pub(super) async fn delete_ai_model_config(
    auth: AuthUser,
    Path(config_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_owned_ai_model_config(&config_id, &auth).await {
        return map_ai_model_config_access_error(err);
    }
    match ai_model_configs::delete_ai_model_config(config_id.as_str()).await {
        Ok(()) => (
            StatusCode::OK,
            Json(json!({"message": "AI 模型配置删除成功"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "删除 AI 模型配置失败", "detail": err})),
        ),
    }
}

pub(super) async fn list_ai_provider_models(
    auth: AuthUser,
    Path(config_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let profile = match ensure_owned_ai_model_config(&config_id, &auth).await {
        Ok(item) => item,
        Err(err) => return map_ai_model_config_access_error(err),
    };

    let base_url =
        normalize_base_url_for_models(profile.provider.as_str(), profile.base_url.as_deref());
    match fetch_provider_models(&profile).await {
        Ok(models) => (
            StatusCode::OK,
            Json(json!({
                "provider_config_id": profile.id,
                "provider": profile.provider,
                "base_url": base_url,
                "source": "live",
                "fetched_at": crate::core::time::now_rfc3339(),
                "models": models,
                "error": null
            })),
        ),
        Err(err) => (
            StatusCode::OK,
            Json(json!({
                "provider_config_id": profile.id,
                "provider": profile.provider,
                "base_url": base_url,
                "source": "fallback",
                "fetched_at": null,
                "models": fallback_model_list(&profile),
                "error": err
            })),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{build_model_config, to_response_value};
    use crate::api::configs::AiModelConfigRequest;
    use crate::models::ai_model_config::AiModelConfig;

    fn sample_request() -> AiModelConfigRequest {
        AiModelConfigRequest {
            id: None,
            name: Some("Model".to_string()),
            provider: Some("gpt".to_string()),
            model: Some("gpt-4o".to_string()),
            thinking_level: Some("medium".to_string()),
            api_key: None,
            clear_api_key: None,
            base_url: Some("https://api.openai.com/v1".to_string()),
            enabled: Some(true),
            supports_images: Some(true),
            supports_reasoning: Some(true),
            supports_responses: Some(true),
        }
    }

    #[test]
    fn response_hides_plaintext_api_key() {
        let value = to_response_value(&AiModelConfig {
            id: "cfg_1".to_string(),
            user_id: Some("user_1".to_string()),
            name: "Model".to_string(),
            provider: "gpt".to_string(),
            model: "gpt-4o".to_string(),
            thinking_level: Some("medium".to_string()),
            api_key: Some("secret".to_string()),
            has_api_key: true,
            base_url: Some("https://api.openai.com/v1".to_string()),
            enabled: true,
            supports_images: true,
            supports_reasoning: true,
            supports_responses: true,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        });

        assert!(value.get("api_key").is_none());
        assert_eq!(
            value.get("has_api_key").and_then(|item| item.as_bool()),
            Some(true)
        );
    }

    #[test]
    fn update_preserves_existing_api_key_when_request_leaves_it_blank() {
        let config = build_model_config(
            "user_1".to_string(),
            "cfg_1".to_string(),
            sample_request(),
            Some("stored-secret".to_string()),
            false,
        )
        .expect("config should build");

        assert_eq!(config.api_key.as_deref(), Some("stored-secret"));
        assert!(config.has_api_key);
    }

    #[test]
    fn create_requires_api_key() {
        let err = build_model_config(
            "user_1".to_string(),
            "cfg_1".to_string(),
            sample_request(),
            None,
            true,
        )
        .expect_err("create should reject missing api key");

        assert!(err.contains("api_key"));
    }

    #[test]
    fn clear_api_key_removes_stored_secret_on_update() {
        let mut request = sample_request();
        request.clear_api_key = Some(true);

        let config = build_model_config(
            "user_1".to_string(),
            "cfg_1".to_string(),
            request,
            Some("stored-secret".to_string()),
            false,
        )
        .expect("config should build");

        assert_eq!(config.api_key, None);
        assert!(!config.has_api_key);
    }

    #[test]
    fn accepts_kimi_alias_provider_with_auto_thinking() {
        let mut request = sample_request();
        request.provider = Some("kimik2".to_string());
        request.model = Some("kimi-k2.5".to_string());
        request.thinking_level = Some("auto".to_string());
        request.api_key = Some("secret".to_string());

        let config = build_model_config(
            "user_1".to_string(),
            "cfg_1".to_string(),
            request,
            None,
            true,
        )
        .expect("config should build");

        assert_eq!(config.provider, "kimi");
        assert_eq!(config.thinking_level.as_deref(), Some("auto"));
    }

    #[test]
    fn accepts_openai_compatible_provider() {
        let mut request = sample_request();
        request.provider = Some("openai-compatible".to_string());
        request.model = Some("custom-model".to_string());
        request.thinking_level = Some("max".to_string());
        request.api_key = Some("secret".to_string());

        let config = build_model_config(
            "user_1".to_string(),
            "cfg_1".to_string(),
            request,
            None,
            true,
        )
        .expect("config should build");

        assert_eq!(config.provider, "openai_compatible");
        assert_eq!(config.thinking_level.as_deref(), Some("xhigh"));
    }
}
