use axum::extract::{Path, Query};
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use crate::core::ai_model_config_access::{
    ensure_owned_ai_model_config, map_ai_model_config_access_error,
};
use crate::core::auth::AuthUser;
use crate::models::ai_model_config::AiModelConfig;
use crate::repositories::ai_model_configs;
use crate::utils::model_config::normalize_provider;

use super::{AiModelConfigRequest, UserQuery};

fn normalize_provider_input(provider: Option<String>) -> Result<String, String> {
    let raw = provider.unwrap_or_else(|| "gpt".to_string());
    let provider = normalize_provider(&raw);

    match provider.as_str() {
        "gpt" | "deepseek" | "kimik2" | "minimax" => Ok(provider),
        _ => Err("provider 仅支持 gpt / deepseek / kimik2 / minimax".to_string()),
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

    if provider != "gpt" {
        return Err("只有 gpt 支持思考等级".to_string());
    }

    let normalized = level.to_lowercase();
    let allowed = ["none", "minimal", "low", "medium", "high", "xhigh"];
    if !allowed.contains(&normalized.as_str()) {
        return Err("思考等级仅支持 none/minimal/low/medium/high/xhigh".to_string());
    }

    Ok(Some(normalized))
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
        assert_eq!(value.get("has_api_key").and_then(|item| item.as_bool()), Some(true));
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
}
