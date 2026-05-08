use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::models::{AiModelConfig, UpsertAiModelConfigRequest};
use crate::services::memory_engine_client;

use super::{build_ai_client, require_auth, resolve_scope_user_id, SharedState};

#[derive(Debug, Deserialize)]
pub(super) struct UserIdQuery {
    user_id: Option<String>,
}

pub(super) async fn list_model_configs(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(q): Query<UserIdQuery>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let _user_id = resolve_scope_user_id(&auth, q.user_id);

    match memory_engine_client::list_global_model_profiles(&state.config).await {
        Ok(items) => (StatusCode::OK, Json(json!({"items": items}))),
        Err(err) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": "list model configs failed", "detail": err})),
        ),
    }
}

pub(super) async fn create_model_config(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(mut req): Json<UpsertAiModelConfigRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    req.user_id = resolve_scope_user_id(&auth, Some(req.user_id.clone()));

    let req = match normalize_model_config_request(req) {
        Ok(v) => v,
        Err(err) => return (StatusCode::BAD_REQUEST, Json(json!({"error": err}))),
    };

    let dto = AiModelConfig {
        id: String::new(),
        user_id: req.user_id,
        name: req.name,
        provider: req.provider,
        model: req.model,
        base_url: req.base_url,
        api_key: req.api_key,
        supports_images: if req.supports_images.unwrap_or(false) { 1 } else { 0 },
        supports_reasoning: if req.supports_reasoning.unwrap_or(false) { 1 } else { 0 },
        supports_responses: if req.supports_responses.unwrap_or(false) { 1 } else { 0 },
        temperature: req.temperature,
        thinking_level: req.thinking_level,
        enabled: if req.enabled.unwrap_or(true) { 1 } else { 0 },
        created_at: String::new(),
        updated_at: String::new(),
    };

    match memory_engine_client::create_global_model_profile(&state.config, &dto).await {
        Ok(item) => (StatusCode::OK, Json(json!(item))),
        Err(err) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": "create model config failed", "detail": err})),
        ),
    }
}

pub(super) async fn update_model_config(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(model_id): Path<String>,
    Json(mut req): Json<UpsertAiModelConfigRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if !auth.is_admin() {
        return (StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"})));
    }
    req.user_id = resolve_scope_user_id(&auth, Some(req.user_id.clone()));

    let req = match normalize_model_config_request(req) {
        Ok(v) => v,
        Err(err) => return (StatusCode::BAD_REQUEST, Json(json!({"error": err}))),
    };

    let dto = AiModelConfig {
        id: model_id.clone(),
        user_id: req.user_id,
        name: req.name,
        provider: req.provider,
        model: req.model,
        base_url: req.base_url,
        api_key: req.api_key,
        supports_images: if req.supports_images.unwrap_or(false) { 1 } else { 0 },
        supports_reasoning: if req.supports_reasoning.unwrap_or(false) { 1 } else { 0 },
        supports_responses: if req.supports_responses.unwrap_or(false) { 1 } else { 0 },
        temperature: req.temperature,
        thinking_level: req.thinking_level,
        enabled: if req.enabled.unwrap_or(true) { 1 } else { 0 },
        created_at: String::new(),
        updated_at: String::new(),
    };

    match memory_engine_client::update_global_model_profile(&state.config, model_id.as_str(), &dto).await {
        Ok(Some(item)) => (StatusCode::OK, Json(json!(item))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "model config not found"})),
        ),
        Err(err) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": "update model config failed", "detail": err})),
        ),
    }
}

fn normalize_model_config_request(
    mut req: UpsertAiModelConfigRequest,
) -> Result<UpsertAiModelConfigRequest, String> {
    req.provider = normalize_provider_input(req.provider.as_str())?;
    if req.model.trim().is_empty() {
        return Err("model is required".to_string());
    }
    if req.name.trim().is_empty() {
        return Err("name is required".to_string());
    }

    req.thinking_level =
        normalize_thinking_level_input(req.provider.as_str(), req.thinking_level.as_deref())?;

    if let Some(v) = req.temperature {
        req.temperature = Some(v.clamp(0.0, 2.0));
    }

    Ok(req)
}

fn normalize_provider_input(raw: &str) -> Result<String, String> {
    let normalized = raw.trim().to_lowercase();
    let provider = if normalized == "openai" {
        "gpt".to_string()
    } else {
        normalized
    };

    match provider.as_str() {
        "gpt" | "deepseek" | "kimik2" | "minimax" => Ok(provider),
        _ => Err("provider only supports gpt/deepseek/kimik2/minimax".to_string()),
    }
}

fn normalize_thinking_level_input(
    provider: &str,
    level: Option<&str>,
) -> Result<Option<String>, String> {
    let level = level
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|v| v.to_lowercase());

    if level.is_none() {
        return Ok(None);
    }

    if provider != "gpt" {
        return Err("thinking_level only works with gpt provider".to_string());
    }

    let Some(level) = level else {
        return Ok(None);
    };
    let allowed = ["none", "minimal", "low", "medium", "high", "xhigh"];
    if !allowed.contains(&level.as_str()) {
        return Err("thinking_level only supports none/minimal/low/medium/high/xhigh".to_string());
    }

    Ok(Some(level))
}

pub(super) async fn delete_model_config(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(model_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if !auth.is_admin() {
        return (StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"})));
    }

    match memory_engine_client::delete_global_model_profile(&state.config, model_id.as_str()).await {
        Ok(true) => (StatusCode::OK, Json(json!({"success": true}))),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "model config not found"})),
        ),
        Err(err) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": "delete model config failed", "detail": err})),
        ),
    }
}

pub(super) async fn test_model_config(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(model_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if !auth.is_admin() {
        return (StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"})));
    }

    let cfg = match memory_engine_client::list_global_model_profiles(&state.config).await {
        Ok(items) => items.into_iter().find(|item| item.id == model_id),
        Err(err) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": "load model config failed", "detail": err})),
            )
        }
    };
    let Some(cfg) = cfg else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "model config not found"})),
        );
    };

    let ai = match build_ai_client(&state) {
        Ok(client) => client,
        Err(err) => return err,
    };

    match ai
        .summarize(
            Some(&cfg),
            128,
            "模型连通性测试",
            &["这是一段连通性测试文本，请返回简短摘要。".to_string()],
            None,
        )
        .await
    {
        Ok(output) => (StatusCode::OK, Json(json!({"ok": true, "output": output}))),
        Err(err) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({"ok": false, "error": err})),
        ),
    }
}
