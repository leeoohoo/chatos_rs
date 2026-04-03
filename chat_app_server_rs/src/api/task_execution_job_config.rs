use axum::http::StatusCode;
use axum::{extract::Query, routing::get, Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::core::auth::AuthUser;
use crate::core::user_scope::resolve_user_id;
use crate::models::task_execution_job_config::{
    TaskExecutionRollupJobConfig, TaskExecutionSummaryJobConfig,
};
use crate::modules::session_summary_job::types::{
    MIN_JOB_INTERVAL_SECONDS, MIN_ROUND_LIMIT, MIN_TARGET_SUMMARY_TOKENS, MIN_TOKEN_LIMIT,
};
use crate::services::memory_server_client;

#[derive(Debug, Deserialize)]
struct UserQuery {
    #[serde(alias = "userId")]
    user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TaskExecutionSummaryJobConfigRequest {
    user_id: Option<String>,
    enabled: Option<bool>,
    summary_model_config_id: Option<Option<String>>,
    token_limit: Option<i64>,
    round_limit: Option<i64>,
    target_summary_tokens: Option<i64>,
    job_interval_seconds: Option<i64>,
    max_scopes_per_tick: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct TaskExecutionRollupJobConfigRequest {
    user_id: Option<String>,
    enabled: Option<bool>,
    summary_model_config_id: Option<Option<String>>,
    token_limit: Option<i64>,
    round_limit: Option<i64>,
    target_summary_tokens: Option<i64>,
    job_interval_seconds: Option<i64>,
    keep_raw_level0_count: Option<i64>,
    max_level: Option<i64>,
    max_scopes_per_tick: Option<i64>,
}

pub fn router() -> Router {
    Router::new()
        .route(
            "/api/task-execution-summary-job-config",
            get(get_task_execution_summary_config)
                .put(put_task_execution_summary_config)
                .patch(patch_task_execution_summary_config),
        )
        .route(
            "/api/task-execution-rollup-job-config",
            get(get_task_execution_rollup_config)
                .put(put_task_execution_rollup_config)
                .patch(patch_task_execution_rollup_config),
        )
}

async fn get_task_execution_summary_config(
    auth: AuthUser,
    Query(query): Query<UserQuery>,
) -> (StatusCode, Json<Value>) {
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

    match memory_server_client::get_task_execution_summary_job_config(&user_id).await {
        Ok(config) => (
            StatusCode::OK,
            Json(to_task_execution_summary_json(
                normalize_task_execution_summary_config(map_task_execution_summary_job_config(
                    config,
                )),
            )),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "获取任务执行总结配置失败", "detail": err})),
        ),
    }
}

async fn put_task_execution_summary_config(
    auth: AuthUser,
    Json(req): Json<TaskExecutionSummaryJobConfigRequest>,
) -> (StatusCode, Json<Value>) {
    upsert_task_execution_summary_config(auth, req).await
}

async fn patch_task_execution_summary_config(
    auth: AuthUser,
    Json(req): Json<TaskExecutionSummaryJobConfigRequest>,
) -> (StatusCode, Json<Value>) {
    upsert_task_execution_summary_config(auth, req).await
}

async fn upsert_task_execution_summary_config(
    auth: AuthUser,
    req: TaskExecutionSummaryJobConfigRequest,
) -> (StatusCode, Json<Value>) {
    let user_id = match resolve_user_id(
        req.user_id
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        &auth,
    ) {
        Ok(user_id) => user_id,
        Err(err) => return err,
    };

    let req_body = memory_server_client::UpsertTaskExecutionSummaryJobConfigRequestDto {
        user_id,
        enabled: req.enabled,
        summary_model_config_id: req.summary_model_config_id.map(normalize_model_id),
        token_limit: req.token_limit,
        round_limit: req.round_limit,
        target_summary_tokens: req.target_summary_tokens,
        job_interval_seconds: req.job_interval_seconds,
        max_scopes_per_tick: req.max_scopes_per_tick,
    };

    match memory_server_client::upsert_task_execution_summary_job_config(&req_body).await {
        Ok(saved) => (
            StatusCode::OK,
            Json(to_task_execution_summary_json(
                normalize_task_execution_summary_config(map_task_execution_summary_job_config(
                    saved,
                )),
            )),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "保存任务执行总结配置失败", "detail": err})),
        ),
    }
}

async fn get_task_execution_rollup_config(
    auth: AuthUser,
    Query(query): Query<UserQuery>,
) -> (StatusCode, Json<Value>) {
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

    match memory_server_client::get_task_execution_rollup_job_config(&user_id).await {
        Ok(config) => (
            StatusCode::OK,
            Json(to_task_execution_rollup_json(
                normalize_task_execution_rollup_config(map_task_execution_rollup_job_config(
                    config,
                )),
            )),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "获取任务执行 rollup 配置失败", "detail": err})),
        ),
    }
}

async fn put_task_execution_rollup_config(
    auth: AuthUser,
    Json(req): Json<TaskExecutionRollupJobConfigRequest>,
) -> (StatusCode, Json<Value>) {
    upsert_task_execution_rollup_config(auth, req).await
}

async fn patch_task_execution_rollup_config(
    auth: AuthUser,
    Json(req): Json<TaskExecutionRollupJobConfigRequest>,
) -> (StatusCode, Json<Value>) {
    upsert_task_execution_rollup_config(auth, req).await
}

async fn upsert_task_execution_rollup_config(
    auth: AuthUser,
    req: TaskExecutionRollupJobConfigRequest,
) -> (StatusCode, Json<Value>) {
    let user_id = match resolve_user_id(
        req.user_id
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        &auth,
    ) {
        Ok(user_id) => user_id,
        Err(err) => return err,
    };

    let req_body = memory_server_client::UpsertTaskExecutionRollupJobConfigRequestDto {
        user_id,
        enabled: req.enabled,
        summary_model_config_id: req.summary_model_config_id.map(normalize_model_id),
        token_limit: req.token_limit,
        round_limit: req.round_limit,
        target_summary_tokens: req.target_summary_tokens,
        job_interval_seconds: req.job_interval_seconds,
        keep_raw_level0_count: req.keep_raw_level0_count,
        max_level: req.max_level,
        max_scopes_per_tick: req.max_scopes_per_tick,
    };

    match memory_server_client::upsert_task_execution_rollup_job_config(&req_body).await {
        Ok(saved) => (
            StatusCode::OK,
            Json(to_task_execution_rollup_json(
                normalize_task_execution_rollup_config(map_task_execution_rollup_job_config(saved)),
            )),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "保存任务执行 rollup 配置失败", "detail": err})),
        ),
    }
}

fn normalize_model_id(value: Option<String>) -> Option<String> {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
}

fn clamp_with_fallback(value: i64, fallback: i64, min_value: i64) -> i64 {
    let candidate = if value > 0 { value } else { fallback };
    candidate.max(min_value)
}

fn to_task_execution_summary_json(config: TaskExecutionSummaryJobConfig) -> Value {
    json!({
        "user_id": config.user_id,
        "enabled": config.enabled,
        "summary_model_config_id": config.summary_model_config_id,
        "token_limit": config.token_limit,
        "round_limit": config.round_limit,
        "target_summary_tokens": config.target_summary_tokens,
        "job_interval_seconds": config.job_interval_seconds,
        "max_scopes_per_tick": config.max_scopes_per_tick,
        "limits": build_task_execution_summary_limits_json(),
        "updated_at": config.updated_at,
    })
}

fn to_task_execution_rollup_json(config: TaskExecutionRollupJobConfig) -> Value {
    json!({
        "user_id": config.user_id,
        "enabled": config.enabled,
        "summary_model_config_id": config.summary_model_config_id,
        "token_limit": config.token_limit,
        "round_limit": config.round_limit,
        "target_summary_tokens": config.target_summary_tokens,
        "job_interval_seconds": config.job_interval_seconds,
        "keep_raw_level0_count": config.keep_raw_level0_count,
        "max_level": config.max_level,
        "max_scopes_per_tick": config.max_scopes_per_tick,
        "limits": build_task_execution_rollup_limits_json(),
        "updated_at": config.updated_at,
    })
}

fn build_task_execution_summary_limits_json() -> Value {
    json!({
        "token_limit": {
            "min": MIN_TOKEN_LIMIT,
        },
        "round_limit": {
            "min": MIN_ROUND_LIMIT,
        },
        "target_summary_tokens": {
            "min": MIN_TARGET_SUMMARY_TOKENS,
        },
        "job_interval_seconds": {
            "min": MIN_JOB_INTERVAL_SECONDS,
        },
        "max_scopes_per_tick": {
            "min": 1,
        }
    })
}

fn build_task_execution_rollup_limits_json() -> Value {
    json!({
        "token_limit": {
            "min": MIN_TOKEN_LIMIT,
        },
        "round_limit": {
            "min": 3,
        },
        "target_summary_tokens": {
            "min": MIN_TARGET_SUMMARY_TOKENS,
        },
        "job_interval_seconds": {
            "min": MIN_JOB_INTERVAL_SECONDS,
        },
        "keep_raw_level0_count": {
            "min": 0,
        },
        "max_level": {
            "min": 1,
        },
        "max_scopes_per_tick": {
            "min": 1,
        }
    })
}

fn map_task_execution_summary_job_config(
    source: memory_server_client::TaskExecutionSummaryJobConfigDto,
) -> TaskExecutionSummaryJobConfig {
    TaskExecutionSummaryJobConfig {
        user_id: source.user_id,
        enabled: source.enabled == 1,
        summary_model_config_id: source.summary_model_config_id,
        token_limit: source.token_limit,
        round_limit: source.round_limit,
        target_summary_tokens: source.target_summary_tokens,
        job_interval_seconds: source.job_interval_seconds,
        max_scopes_per_tick: source.max_scopes_per_tick,
        updated_at: source.updated_at,
    }
}

fn normalize_task_execution_summary_config(
    mut config: TaskExecutionSummaryJobConfig,
) -> TaskExecutionSummaryJobConfig {
    config.token_limit = clamp_with_fallback(config.token_limit, 6000, MIN_TOKEN_LIMIT);
    config.round_limit = clamp_with_fallback(config.round_limit, 8, MIN_ROUND_LIMIT);
    config.target_summary_tokens =
        clamp_with_fallback(config.target_summary_tokens, 700, MIN_TARGET_SUMMARY_TOKENS);
    config.job_interval_seconds =
        clamp_with_fallback(config.job_interval_seconds, 30, MIN_JOB_INTERVAL_SECONDS);
    config.max_scopes_per_tick = clamp_with_fallback(config.max_scopes_per_tick, 50, 1);
    config
}

fn map_task_execution_rollup_job_config(
    source: memory_server_client::TaskExecutionRollupJobConfigDto,
) -> TaskExecutionRollupJobConfig {
    TaskExecutionRollupJobConfig {
        user_id: source.user_id,
        enabled: source.enabled == 1,
        summary_model_config_id: source.summary_model_config_id,
        token_limit: source.token_limit,
        round_limit: source.round_limit,
        target_summary_tokens: source.target_summary_tokens,
        job_interval_seconds: source.job_interval_seconds,
        keep_raw_level0_count: source.keep_raw_level0_count,
        max_level: source.max_level,
        max_scopes_per_tick: source.max_scopes_per_tick,
        updated_at: source.updated_at,
    }
}

fn normalize_task_execution_rollup_config(
    mut config: TaskExecutionRollupJobConfig,
) -> TaskExecutionRollupJobConfig {
    config.token_limit = clamp_with_fallback(config.token_limit, 6000, MIN_TOKEN_LIMIT);
    config.round_limit = clamp_with_fallback(config.round_limit, 50, 3);
    config.target_summary_tokens =
        clamp_with_fallback(config.target_summary_tokens, 700, MIN_TARGET_SUMMARY_TOKENS);
    config.job_interval_seconds =
        clamp_with_fallback(config.job_interval_seconds, 60, MIN_JOB_INTERVAL_SECONDS);
    config.keep_raw_level0_count = config.keep_raw_level0_count.max(0);
    config.max_level = clamp_with_fallback(config.max_level, 4, 1);
    config.max_scopes_per_tick = clamp_with_fallback(config.max_scopes_per_tick, 50, 1);
    config
}
