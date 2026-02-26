use axum::extract::{Path, Query};
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::models::ai_model_config::AiModelConfig;
use crate::repositories::ai_model_configs as ai_repo;

use super::{AiModelConfigRequest, UserQuery};

pub(super) async fn list_ai_model_configs(
    Query(query): Query<UserQuery>,
) -> (StatusCode, Json<Value>) {
    let configs = match ai_repo::list_ai_model_configs(query.user_id).await {
        Ok(list) => list,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "获取AI模型配置失败", "detail": err})),
            )
        }
    };
    let out = configs
        .into_iter()
        .map(|cfg| {
            json!({
                "id": cfg.id,
                "name": cfg.name,
                "provider": cfg.provider,
                "model": cfg.model,
                "thinking_level": cfg.thinking_level,
                "api_key": cfg.api_key,
                "base_url": cfg.base_url,
                "user_id": cfg.user_id,
                "enabled": cfg.enabled,
                "supports_images": cfg.supports_images,
                "supports_reasoning": cfg.supports_reasoning,
                "supports_responses": cfg.supports_responses,
                "created_at": cfg.created_at,
                "updated_at": cfg.updated_at
            })
        })
        .collect();
    (StatusCode::OK, Json(Value::Array(out)))
}

pub(super) async fn create_ai_model_config(
    Json(req): Json<AiModelConfigRequest>,
) -> (StatusCode, Json<Value>) {
    let Some(name) = req.name else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "创建AI模型配置失败"})),
        );
    };
    let Some(model) = req.model else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "创建AI模型配置失败"})),
        );
    };
    let id = Uuid::new_v4().to_string();
    let provider = match normalize_provider_input(req.provider.clone()) {
        Ok(p) => p,
        Err(err) => return (StatusCode::BAD_REQUEST, Json(json!({"error": err}))),
    };
    let thinking_level = match normalize_thinking_level_input(&provider, req.thinking_level.clone())
    {
        Ok(v) => v,
        Err(err) => return (StatusCode::BAD_REQUEST, Json(json!({"error": err}))),
    };
    let cfg = AiModelConfig {
        id: id.clone(),
        name,
        provider,
        model,
        thinking_level,
        api_key: req.api_key,
        base_url: req.base_url,
        user_id: req.user_id,
        enabled: req.enabled.unwrap_or(true),
        supports_images: req.supports_images.unwrap_or(false),
        supports_reasoning: req.supports_reasoning.unwrap_or(false),
        supports_responses: req.supports_responses.unwrap_or(false),
        created_at: crate::core::time::now_rfc3339(),
        updated_at: crate::core::time::now_rfc3339(),
    };
    if let Err(err) = ai_repo::create_ai_model_config(&cfg).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "创建AI模型配置失败", "detail": err})),
        );
    }
    (StatusCode::CREATED, Json(ai_model_config_value(&cfg)))
}

pub(super) async fn update_ai_model_config(
    Path(config_id): Path<String>,
    Json(req): Json<AiModelConfigRequest>,
) -> (StatusCode, Json<Value>) {
    let existing = match ai_repo::get_ai_model_config_by_id(&config_id).await {
        Ok(cfg) => cfg,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "更新AI模型配置失败", "detail": err})),
            )
        }
    };
    if existing.is_none() {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "AI模型配置不存在"})),
        );
    }
    let mut cfg = existing.unwrap();
    if let Some(v) = req.name {
        cfg.name = v;
    }
    let provider = if let Some(v) = req.provider {
        match normalize_provider_input(Some(v)) {
            Ok(p) => p,
            Err(err) => return (StatusCode::BAD_REQUEST, Json(json!({"error": err}))),
        }
    } else {
        normalize_provider_input(Some(cfg.provider.clone())).unwrap_or_else(|_| "gpt".to_string())
    };
    cfg.provider = provider.clone();
    if let Some(v) = req.model {
        cfg.model = v;
    }
    if provider != "gpt" {
        cfg.thinking_level = None;
    } else if let Some(v) = req.thinking_level {
        cfg.thinking_level = Some(v);
    }
    if let Some(v) = req.api_key {
        cfg.api_key = Some(v);
    }
    if let Some(v) = req.base_url {
        cfg.base_url = Some(v);
    }
    if let Some(v) = req.enabled {
        cfg.enabled = v;
    }
    if let Some(v) = req.supports_images {
        cfg.supports_images = v;
    }
    if let Some(v) = req.supports_reasoning {
        cfg.supports_reasoning = v;
    }
    if let Some(v) = req.supports_responses {
        cfg.supports_responses = v;
    }
    match normalize_thinking_level_input(&provider, cfg.thinking_level.clone()) {
        Ok(v) => cfg.thinking_level = v,
        Err(err) => return (StatusCode::BAD_REQUEST, Json(json!({"error": err}))),
    }
    if let Err(err) = ai_repo::update_ai_model_config(&config_id, &cfg).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "更新AI模型配置失败", "detail": err})),
        );
    }
    let updated = match ai_repo::get_ai_model_config_by_id(&config_id).await {
        Ok(Some(cfg)) => cfg,
        Ok(None) => return (StatusCode::OK, Json(Value::Null)),
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "更新AI模型配置失败", "detail": err})),
            )
        }
    };
    (StatusCode::OK, Json(ai_model_config_value(&updated)))
}

pub(super) async fn delete_ai_model_config(
    Path(config_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let existing = match ai_repo::get_ai_model_config_by_id(&config_id).await {
        Ok(cfg) => cfg,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "删除AI模型配置失败", "detail": err})),
            )
        }
    };
    if existing.is_none() {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "AI模型配置不存在"})),
        );
    }
    if let Err(err) = ai_repo::delete_ai_model_config(&config_id).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "删除AI模型配置失败", "detail": err})),
        );
    }
    (
        StatusCode::OK,
        Json(json!({ "message": "AI模型配置删除成功" })),
    )
}

fn ai_model_config_value(cfg: &AiModelConfig) -> Value {
    json!({
        "id": cfg.id.clone(),
        "name": cfg.name.clone(),
        "provider": cfg.provider.clone(),
        "model": cfg.model.clone(),
        "thinking_level": cfg.thinking_level.clone(),
        "api_key": cfg.api_key.clone(),
        "base_url": cfg.base_url.clone(),
        "user_id": cfg.user_id.clone(),
        "enabled": cfg.enabled,
        "supports_images": cfg.supports_images,
        "supports_reasoning": cfg.supports_reasoning,
        "supports_responses": cfg.supports_responses,
        "created_at": cfg.created_at.clone(),
        "updated_at": cfg.updated_at.clone()
    })
}

fn normalize_provider_input(provider: Option<String>) -> Result<String, String> {
    let raw = provider.unwrap_or_else(|| "gpt".to_string());
    let p = raw.trim().to_lowercase();
    let p = if p == "openai" { "gpt".to_string() } else { p };
    match p.as_str() {
        "gpt" | "deepseek" | "kimik2" => Ok(p),
        _ => Err("provider 仅支持 gpt / deepseek / kimik2".to_string()),
    }
}

fn normalize_thinking_level_input(
    provider: &str,
    level: Option<String>,
) -> Result<Option<String>, String> {
    let level = level
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    if level.is_none() {
        return Ok(None);
    }
    let provider = normalize_provider_input(Some(provider.to_string()))?;
    if provider != "gpt" {
        return Err("只有 gpt 支持思考等级".to_string());
    }
    let lvl = level.unwrap().to_lowercase();
    let allowed = ["none", "minimal", "low", "medium", "high", "xhigh"];
    if !allowed.contains(&lvl.as_str()) {
        return Err("思考等级仅支持 none/minimal/low/medium/high/xhigh".to_string());
    }
    Ok(Some(lvl))
}
