use sqlx::SqlitePool;
use uuid::Uuid;

use crate::models::{
    AiModelConfig, SummaryJobConfig, SummaryRollupJobConfig, UpsertAiModelConfigRequest,
    UpsertSummaryJobConfigRequest, UpsertSummaryRollupJobConfigRequest,
};

use super::{auth::ADMIN_USER_ID, now_rfc3339};

pub async fn list_model_configs(pool: &SqlitePool, user_id: &str) -> Result<Vec<AiModelConfig>, String> {
    sqlx::query_as::<_, AiModelConfig>(
        "SELECT * FROM ai_model_configs WHERE user_id = ? ORDER BY updated_at DESC",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())
}

pub async fn get_model_config_by_id(pool: &SqlitePool, id: &str) -> Result<Option<AiModelConfig>, String> {
    sqlx::query_as::<_, AiModelConfig>("SELECT * FROM ai_model_configs WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())
}

pub async fn create_model_config(
    pool: &SqlitePool,
    req: UpsertAiModelConfigRequest,
) -> Result<AiModelConfig, String> {
    let id = Uuid::new_v4().to_string();
    let now = now_rfc3339();

    sqlx::query(
        "INSERT INTO ai_model_configs (id, user_id, name, provider, model, base_url, api_key, supports_images, supports_reasoning, supports_responses, temperature, thinking_level, enabled, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(req.user_id)
    .bind(req.name)
    .bind(req.provider)
    .bind(req.model)
    .bind(req.base_url)
    .bind(req.api_key)
    .bind(if req.supports_images.unwrap_or(false) { 1 } else { 0 })
    .bind(if req.supports_reasoning.unwrap_or(false) { 1 } else { 0 })
    .bind(if req.supports_responses.unwrap_or(false) { 1 } else { 0 })
    .bind(req.temperature)
    .bind(req.thinking_level)
    .bind(if req.enabled.unwrap_or(true) { 1 } else { 0 })
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    get_model_config_by_id(pool, &id)
        .await?
        .ok_or_else(|| "created model config not found".to_string())
}

pub async fn update_model_config(
    pool: &SqlitePool,
    id: &str,
    req: UpsertAiModelConfigRequest,
) -> Result<Option<AiModelConfig>, String> {
    let existing = get_model_config_by_id(pool, id).await?;
    let Some(existing) = existing else {
        return Ok(None);
    };

    let now = now_rfc3339();

    sqlx::query(
        "UPDATE ai_model_configs SET user_id = ?, name = ?, provider = ?, model = ?, base_url = ?, api_key = ?, supports_images = ?, supports_reasoning = ?, supports_responses = ?, temperature = ?, thinking_level = ?, enabled = ?, updated_at = ? WHERE id = ?",
    )
    .bind(req.user_id)
    .bind(req.name)
    .bind(req.provider)
    .bind(req.model)
    .bind(req.base_url.or(existing.base_url))
    .bind(req.api_key.or(existing.api_key))
    .bind(if req.supports_images.unwrap_or(existing.supports_images == 1) {
        1
    } else {
        0
    })
    .bind(if req
        .supports_reasoning
        .unwrap_or(existing.supports_reasoning == 1)
    {
        1
    } else {
        0
    })
    .bind(if req.supports_responses.unwrap_or(existing.supports_responses == 1) {
        1
    } else {
        0
    })
    .bind(req.temperature.or(existing.temperature))
    .bind(req.thinking_level.or(existing.thinking_level))
    .bind(if req.enabled.unwrap_or(existing.enabled == 1) {
        1
    } else {
        0
    })
    .bind(now)
    .bind(id)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    get_model_config_by_id(pool, id).await
}

pub async fn delete_model_config(pool: &SqlitePool, id: &str) -> Result<bool, String> {
    let result = sqlx::query("DELETE FROM ai_model_configs WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(result.rows_affected() > 0)
}

pub async fn get_summary_job_config(
    pool: &SqlitePool,
    user_id: &str,
) -> Result<SummaryJobConfig, String> {
    if let Some(cfg) = fetch_summary_job_config(pool, user_id).await? {
        return Ok(cfg);
    }

    if user_id != ADMIN_USER_ID {
        if let Some(admin_cfg) = fetch_summary_job_config(pool, ADMIN_USER_ID).await? {
            return Ok(SummaryJobConfig {
                user_id: user_id.to_string(),
                enabled: admin_cfg.enabled,
                summary_model_config_id: admin_cfg.summary_model_config_id,
                token_limit: admin_cfg.token_limit,
                round_limit: admin_cfg.round_limit,
                target_summary_tokens: admin_cfg.target_summary_tokens,
                job_interval_seconds: admin_cfg.job_interval_seconds,
                max_sessions_per_tick: admin_cfg.max_sessions_per_tick,
                updated_at: admin_cfg.updated_at,
            });
        }
    }

    create_default_summary_job_config(pool, user_id).await?;
    sqlx::query_as::<_, SummaryJobConfig>("SELECT * FROM summary_job_configs WHERE user_id = ?")
        .bind(user_id)
        .fetch_one(pool)
        .await
        .map_err(|e| e.to_string())
}

async fn fetch_summary_job_config(
    pool: &SqlitePool,
    user_id: &str,
) -> Result<Option<SummaryJobConfig>, String> {
    sqlx::query_as::<_, SummaryJobConfig>("SELECT * FROM summary_job_configs WHERE user_id = ?")
        .bind(user_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())
}

async fn create_default_summary_job_config(pool: &SqlitePool, user_id: &str) -> Result<(), String> {
    let now = now_rfc3339();
    sqlx::query("INSERT INTO summary_job_configs (user_id, enabled, summary_model_config_id, token_limit, round_limit, target_summary_tokens, job_interval_seconds, max_sessions_per_tick, updated_at) VALUES (?, 1, NULL, 6000, 8, 700, 30, 50, ?)")
        .bind(user_id)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn upsert_summary_job_config(
    pool: &SqlitePool,
    req: UpsertSummaryJobConfigRequest,
) -> Result<SummaryJobConfig, String> {
    let mut current = get_summary_job_config(pool, req.user_id.as_str()).await?;

    if let Some(v) = req.enabled {
        current.enabled = if v { 1 } else { 0 };
    }
    if let Some(v) = req.summary_model_config_id {
        current.summary_model_config_id = v.filter(|s| !s.trim().is_empty());
    }
    if let Some(v) = req.token_limit {
        current.token_limit = v.max(500);
    }
    if let Some(v) = req.round_limit {
        current.round_limit = v.max(1);
    }
    if let Some(v) = req.target_summary_tokens {
        current.target_summary_tokens = v.max(200);
    }
    if let Some(v) = req.job_interval_seconds {
        current.job_interval_seconds = v.max(10);
    }
    if let Some(v) = req.max_sessions_per_tick {
        current.max_sessions_per_tick = v.max(1);
    }

    current.updated_at = now_rfc3339();

    sqlx::query("INSERT INTO summary_job_configs (user_id, enabled, summary_model_config_id, token_limit, round_limit, target_summary_tokens, job_interval_seconds, max_sessions_per_tick, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(user_id) DO UPDATE SET enabled = excluded.enabled, summary_model_config_id = excluded.summary_model_config_id, token_limit = excluded.token_limit, round_limit = excluded.round_limit, target_summary_tokens = excluded.target_summary_tokens, job_interval_seconds = excluded.job_interval_seconds, max_sessions_per_tick = excluded.max_sessions_per_tick, updated_at = excluded.updated_at")
        .bind(&current.user_id)
        .bind(current.enabled)
        .bind(&current.summary_model_config_id)
        .bind(current.token_limit)
        .bind(current.round_limit)
        .bind(current.target_summary_tokens)
        .bind(current.job_interval_seconds)
        .bind(current.max_sessions_per_tick)
        .bind(&current.updated_at)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

    Ok(current)
}

pub async fn get_summary_rollup_job_config(
    pool: &SqlitePool,
    user_id: &str,
) -> Result<SummaryRollupJobConfig, String> {
    if let Some(cfg) = fetch_summary_rollup_job_config(pool, user_id).await? {
        return Ok(cfg);
    }

    if user_id != ADMIN_USER_ID {
        if let Some(admin_cfg) = fetch_summary_rollup_job_config(pool, ADMIN_USER_ID).await? {
            return Ok(SummaryRollupJobConfig {
                user_id: user_id.to_string(),
                enabled: admin_cfg.enabled,
                summary_model_config_id: admin_cfg.summary_model_config_id,
                token_limit: admin_cfg.token_limit,
                round_limit: admin_cfg.round_limit,
                target_summary_tokens: admin_cfg.target_summary_tokens,
                job_interval_seconds: admin_cfg.job_interval_seconds,
                keep_raw_level0_count: admin_cfg.keep_raw_level0_count,
                max_level: admin_cfg.max_level,
                max_sessions_per_tick: admin_cfg.max_sessions_per_tick,
                updated_at: admin_cfg.updated_at,
            });
        }
    }

    create_default_summary_rollup_job_config(pool, user_id).await?;
    sqlx::query_as::<_, SummaryRollupJobConfig>(
        "SELECT * FROM summary_rollup_job_configs WHERE user_id = ?",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())
}

async fn fetch_summary_rollup_job_config(
    pool: &SqlitePool,
    user_id: &str,
) -> Result<Option<SummaryRollupJobConfig>, String> {
    sqlx::query_as::<_, SummaryRollupJobConfig>(
        "SELECT * FROM summary_rollup_job_configs WHERE user_id = ?",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())
}

async fn create_default_summary_rollup_job_config(
    pool: &SqlitePool,
    user_id: &str,
) -> Result<(), String> {
    let now = now_rfc3339();
    sqlx::query("INSERT INTO summary_rollup_job_configs (user_id, enabled, summary_model_config_id, token_limit, round_limit, target_summary_tokens, job_interval_seconds, keep_raw_level0_count, max_level, max_sessions_per_tick, updated_at) VALUES (?, 1, NULL, 6000, 50, 700, 60, 5, 4, 50, ?)")
        .bind(user_id)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn upsert_summary_rollup_job_config(
    pool: &SqlitePool,
    req: UpsertSummaryRollupJobConfigRequest,
) -> Result<SummaryRollupJobConfig, String> {
    let mut current = get_summary_rollup_job_config(pool, req.user_id.as_str()).await?;

    if let Some(v) = req.enabled {
        current.enabled = if v { 1 } else { 0 };
    }
    if let Some(v) = req.summary_model_config_id {
        current.summary_model_config_id = v.filter(|s| !s.trim().is_empty());
    }
    if let Some(v) = req.token_limit {
        current.token_limit = v.max(500);
    }
    if let Some(v) = req.round_limit {
        current.round_limit = v.max(10);
    }
    if let Some(v) = req.target_summary_tokens {
        current.target_summary_tokens = v.max(200);
    }
    if let Some(v) = req.job_interval_seconds {
        current.job_interval_seconds = v.max(10);
    }
    if let Some(v) = req.keep_raw_level0_count {
        current.keep_raw_level0_count = v.max(0);
    }
    if let Some(v) = req.max_level {
        current.max_level = v.max(1);
    }
    if let Some(v) = req.max_sessions_per_tick {
        current.max_sessions_per_tick = v.max(1);
    }

    current.updated_at = now_rfc3339();

    sqlx::query("INSERT INTO summary_rollup_job_configs (user_id, enabled, summary_model_config_id, token_limit, round_limit, target_summary_tokens, job_interval_seconds, keep_raw_level0_count, max_level, max_sessions_per_tick, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(user_id) DO UPDATE SET enabled = excluded.enabled, summary_model_config_id = excluded.summary_model_config_id, token_limit = excluded.token_limit, round_limit = excluded.round_limit, target_summary_tokens = excluded.target_summary_tokens, job_interval_seconds = excluded.job_interval_seconds, keep_raw_level0_count = excluded.keep_raw_level0_count, max_level = excluded.max_level, max_sessions_per_tick = excluded.max_sessions_per_tick, updated_at = excluded.updated_at")
        .bind(&current.user_id)
        .bind(current.enabled)
        .bind(&current.summary_model_config_id)
        .bind(current.token_limit)
        .bind(current.round_limit)
        .bind(current.target_summary_tokens)
        .bind(current.job_interval_seconds)
        .bind(current.keep_raw_level0_count)
        .bind(current.max_level)
        .bind(current.max_sessions_per_tick)
        .bind(&current.updated_at)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

    Ok(current)
}
