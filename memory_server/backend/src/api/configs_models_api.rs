use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::models::UpsertAiModelConfigRequest;
use crate::repositories::configs;

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
    let user_id = resolve_scope_user_id(&auth, q.user_id);

    match configs::list_model_configs(&state.pool, user_id.as_str()).await {
        Ok(items) => (StatusCode::OK, Json(json!({"items": items}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
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

    match configs::create_model_config(&state.pool, req).await {
        Ok(item) => (StatusCode::OK, Json(json!(item))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "create model config failed", "detail": err})),
        ),
    }
}

pub(super) async fn get_model_config(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(model_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let cfg = match configs::get_model_config_by_id(&state.pool, model_id.as_str()).await {
        Ok(Some(cfg)) => cfg,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "model config not found"})),
            )
        }
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "load model config failed", "detail": err})),
            )
        }
    };
    if !auth.is_admin() && cfg.user_id != auth.user_id {
        return (StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"})));
    }
    (StatusCode::OK, Json(json!(cfg)))
}

pub(super) async fn internal_get_model_config(
    State(state): State<SharedState>,
    Path(model_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    match configs::get_model_config_by_id(&state.pool, model_id.as_str()).await {
        Ok(Some(cfg)) => (StatusCode::OK, Json(json!(cfg))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "model config not found"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "load model config failed", "detail": err})),
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
    let existing = match configs::get_model_config_by_id(&state.pool, model_id.as_str()).await {
        Ok(Some(cfg)) => cfg,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "model config not found"})),
            )
        }
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "load model config failed", "detail": err})),
            )
        }
    };
    if !auth.is_admin() && existing.user_id != auth.user_id {
        return (StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"})));
    }
    req.user_id = existing.user_id;

    let req = match normalize_model_config_request(req) {
        Ok(v) => v,
        Err(err) => return (StatusCode::BAD_REQUEST, Json(json!({"error": err}))),
    };

    match configs::update_model_config(&state.pool, model_id.as_str(), req).await {
        Ok(Some(item)) => (StatusCode::OK, Json(json!(item))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "model config not found"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
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
        "gpt" | "deepseek" | "kimik2" => Ok(provider),
        _ => Err("provider only supports gpt/deepseek/kimik2".to_string()),
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

    let level = level.expect("checked");
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
    let existing = match configs::get_model_config_by_id(&state.pool, model_id.as_str()).await {
        Ok(Some(cfg)) => cfg,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "model config not found"})),
            )
        }
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "load model config failed", "detail": err})),
            )
        }
    };
    if !auth.is_admin() && existing.user_id != auth.user_id {
        return (StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"})));
    }

    match configs::delete_model_config(&state.pool, model_id.as_str()).await {
        Ok(true) => (StatusCode::OK, Json(json!({"success": true}))),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "model config not found"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
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

    let cfg = match configs::get_model_config_by_id(&state.pool, model_id.as_str()).await {
        Ok(Some(cfg)) => cfg,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "model config not found"})),
            )
        }
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "load model config failed", "detail": err})),
            )
        }
    };
    if !auth.is_admin() && cfg.user_id != auth.user_id {
        return (StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"})));
    }

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
