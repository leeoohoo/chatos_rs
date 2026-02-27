use axum::http::StatusCode;
use axum::{extract::Query, routing::get, Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::models::session_summary_job_config::{
    SessionSummaryJobConfig, SessionSummaryJobConfigService,
};

const DEFAULT_USER_ID: &str = "default-user";

#[derive(Debug, Deserialize)]
struct UserQuery {
    #[serde(alias = "userId")]
    user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SummaryJobConfigRequest {
    user_id: Option<String>,
    enabled: Option<bool>,
    summary_model_config_id: Option<Option<String>>,
    token_limit: Option<i64>,
    #[serde(alias = "message_limit", alias = "messageCountLimit")]
    message_count_limit: Option<i64>,
    round_limit: Option<i64>,
    target_summary_tokens: Option<i64>,
    job_interval_seconds: Option<i64>,
}

pub fn router() -> Router {
    Router::new().route(
        "/api/session-summary-job-config",
        get(get_config).put(put_config).patch(patch_config),
    )
}

async fn get_config(Query(query): Query<UserQuery>) -> (StatusCode, Json<Value>) {
    let user_id = resolve_user_id(query.user_id);
    let defaults = crate::modules::session_summary_job::types::SummaryJobDefaults::from_env();

    match SessionSummaryJobConfigService::get_by_user(&user_id).await {
        Ok(Some(config)) => {
            let normalized = normalize_config(config, &defaults);
            (StatusCode::OK, Json(to_json(normalized)))
        }
        Ok(None) => {
            let fallback = default_config_for_user(&user_id, &defaults);
            (StatusCode::OK, Json(to_json(fallback)))
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "获取会话总结配置失败", "detail": err})),
        ),
    }
}

async fn put_config(Json(req): Json<SummaryJobConfigRequest>) -> (StatusCode, Json<Value>) {
    upsert_config(req).await
}

async fn patch_config(Json(req): Json<SummaryJobConfigRequest>) -> (StatusCode, Json<Value>) {
    upsert_config(req).await
}

async fn upsert_config(req: SummaryJobConfigRequest) -> (StatusCode, Json<Value>) {
    let user_id = resolve_user_id(req.user_id);
    let defaults = crate::modules::session_summary_job::types::SummaryJobDefaults::from_env();

    let mut config = match SessionSummaryJobConfigService::get_by_user(&user_id).await {
        Ok(Some(current)) => current,
        Ok(None) => default_config_for_user(&user_id, &defaults),
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "读取会话总结配置失败", "detail": err})),
            )
        }
    };

    if let Some(enabled) = req.enabled {
        config.enabled = enabled;
    }
    if let Some(model_id) = req.summary_model_config_id {
        config.summary_model_config_id = model_id
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
    }
    if let Some(token_limit) = req.token_limit {
        config.token_limit = token_limit.max(500);
    }
    let message_count_limit = req.message_count_limit.or(req.round_limit);
    if let Some(round_limit) = message_count_limit {
        config.round_limit = round_limit.max(1);
    }
    if let Some(target_summary_tokens) = req.target_summary_tokens {
        config.target_summary_tokens = target_summary_tokens.max(200);
    }
    if let Some(job_interval_seconds) = req.job_interval_seconds {
        config.job_interval_seconds = job_interval_seconds.max(10);
    }

    match SessionSummaryJobConfigService::upsert(&config).await {
        Ok(saved) => (
            StatusCode::OK,
            Json(to_json(normalize_config(saved, &defaults))),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "保存会话总结配置失败", "detail": err})),
        ),
    }
}

fn resolve_user_id(input: Option<String>) -> String {
    input
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_USER_ID.to_string())
}

fn default_config_for_user(
    user_id: &str,
    defaults: &crate::modules::session_summary_job::types::SummaryJobDefaults,
) -> SessionSummaryJobConfig {
    SessionSummaryJobConfig {
        user_id: user_id.to_string(),
        enabled: defaults.enabled,
        summary_model_config_id: None,
        token_limit: defaults.token_limit,
        round_limit: defaults.round_limit,
        target_summary_tokens: defaults.target_summary_tokens,
        job_interval_seconds: defaults.job_interval_seconds,
        updated_at: crate::core::time::now_rfc3339(),
    }
}

fn to_json(config: SessionSummaryJobConfig) -> Value {
    json!({
        "user_id": config.user_id,
        "enabled": config.enabled,
        "summary_model_config_id": config.summary_model_config_id,
        "token_limit": config.token_limit,
        "message_count_limit": config.round_limit,
        "round_limit": config.round_limit,
        "target_summary_tokens": config.target_summary_tokens,
        "job_interval_seconds": config.job_interval_seconds,
        "updated_at": config.updated_at,
    })
}

fn normalize_config(
    mut config: SessionSummaryJobConfig,
    defaults: &crate::modules::session_summary_job::types::SummaryJobDefaults,
) -> SessionSummaryJobConfig {
    config.token_limit = if config.token_limit > 0 {
        config.token_limit
    } else {
        defaults.token_limit
    }
    .max(500);
    config.round_limit = if config.round_limit > 0 {
        config.round_limit
    } else {
        defaults.round_limit
    }
    .max(1);
    config.target_summary_tokens = if config.target_summary_tokens > 0 {
        config.target_summary_tokens
    } else {
        defaults.target_summary_tokens
    }
    .max(200);
    config.job_interval_seconds = if config.job_interval_seconds > 0 {
        config.job_interval_seconds
    } else {
        defaults.job_interval_seconds
    }
    .max(10);
    config
}
