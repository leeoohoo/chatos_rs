use crate::modules::session_summary_job::types::{
    MIN_JOB_INTERVAL_SECONDS, MIN_ROUND_LIMIT, MIN_TARGET_SUMMARY_TOKENS, MIN_TOKEN_LIMIT,
};
use axum::http::StatusCode;
use axum::{extract::Query, routing::get, Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::core::auth::AuthUser;
use crate::core::user_scope::resolve_user_id;
use crate::models::session_summary_job_config::SessionSummaryJobConfig;
use crate::services::memory_server_client;

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

async fn get_config(auth: AuthUser, Query(query): Query<UserQuery>) -> (StatusCode, Json<Value>) {
    let user_id = match resolve_user_id(
        query
            .user_id
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        &auth,
    ) {
        Ok(user_id) => user_id,
        Err(err) => return err,
    };
    let defaults = crate::modules::session_summary_job::types::SummaryJobDefaults::from_env();

    match memory_server_client::get_summary_job_config(&user_id).await {
        Ok(config) => (
            StatusCode::OK,
            Json(to_json(normalize_config(
                map_memory_summary_job_config(config),
                &defaults,
            ))),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "获取会话总结配置失败", "detail": err})),
        ),
    }
}

async fn put_config(
    auth: AuthUser,
    Json(req): Json<SummaryJobConfigRequest>,
) -> (StatusCode, Json<Value>) {
    upsert_config(auth, req).await
}

async fn patch_config(
    auth: AuthUser,
    Json(req): Json<SummaryJobConfigRequest>,
) -> (StatusCode, Json<Value>) {
    upsert_config(auth, req).await
}

async fn upsert_config(auth: AuthUser, req: SummaryJobConfigRequest) -> (StatusCode, Json<Value>) {
    let SummaryJobConfigRequest {
        user_id,
        enabled,
        summary_model_config_id,
        token_limit,
        message_count_limit,
        round_limit,
        target_summary_tokens,
        job_interval_seconds,
    } = req;

    let user_id = match resolve_user_id(
        user_id
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        &auth,
    ) {
        Ok(user_id) => user_id,
        Err(err) => return err,
    };
    let defaults = crate::modules::session_summary_job::types::SummaryJobDefaults::from_env();

    let message_count_limit = message_count_limit.or(round_limit);

    let req_body = memory_server_client::UpsertSummaryJobConfigRequestDto {
        user_id: user_id.clone(),
        enabled,
        summary_model_config_id: summary_model_config_id.map(|model_id| {
            model_id
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        }),
        token_limit,
        round_limit: message_count_limit,
        target_summary_tokens,
        job_interval_seconds,
    };

    match memory_server_client::upsert_summary_job_config(&req_body).await {
        Ok(saved) => (
            StatusCode::OK,
            Json(to_json(normalize_config(
                map_memory_summary_job_config(saved),
                &defaults,
            ))),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "保存会话总结配置失败", "detail": err})),
        ),
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
        "limits": build_limits_json(),
        "updated_at": config.updated_at,
    })
}

fn clamp_with_fallback(value: i64, fallback: i64, min_value: i64) -> i64 {
    let candidate = if value > 0 { value } else { fallback };
    candidate.max(min_value)
}

fn build_limits_json() -> Value {
    json!({
        "token_limit": {
            "min": MIN_TOKEN_LIMIT,
        },
        "message_count_limit": {
            "min": MIN_ROUND_LIMIT,
        },
        "round_limit": {
            "min": MIN_ROUND_LIMIT,
        },
        "target_summary_tokens": {
            "min": MIN_TARGET_SUMMARY_TOKENS,
        },
        "job_interval_seconds": {
            "min": MIN_JOB_INTERVAL_SECONDS,
        }
    })
}

fn normalize_config(
    mut config: SessionSummaryJobConfig,
    defaults: &crate::modules::session_summary_job::types::SummaryJobDefaults,
) -> SessionSummaryJobConfig {
    config.token_limit =
        clamp_with_fallback(config.token_limit, defaults.token_limit, MIN_TOKEN_LIMIT);
    config.round_limit =
        clamp_with_fallback(config.round_limit, defaults.round_limit, MIN_ROUND_LIMIT);
    config.target_summary_tokens = clamp_with_fallback(
        config.target_summary_tokens,
        defaults.target_summary_tokens,
        MIN_TARGET_SUMMARY_TOKENS,
    );
    config.job_interval_seconds = clamp_with_fallback(
        config.job_interval_seconds,
        defaults.job_interval_seconds,
        MIN_JOB_INTERVAL_SECONDS,
    );
    config
}

fn map_memory_summary_job_config(
    source: memory_server_client::SummaryJobConfigDto,
) -> SessionSummaryJobConfig {
    SessionSummaryJobConfig {
        user_id: source.user_id,
        enabled: source.enabled == 1,
        summary_model_config_id: source.summary_model_config_id,
        token_limit: source.token_limit,
        round_limit: source.round_limit,
        target_summary_tokens: source.target_summary_tokens,
        job_interval_seconds: source.job_interval_seconds,
        updated_at: source.updated_at,
    }
}
