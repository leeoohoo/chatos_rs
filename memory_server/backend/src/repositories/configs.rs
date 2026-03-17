use futures_util::TryStreamExt;
use mongodb::bson::doc;
use mongodb::options::FindOptions;
use uuid::Uuid;

use crate::db::Db;
use crate::models::{
    AgentMemoryJobConfig, AiModelConfig, SummaryJobConfig, SummaryRollupJobConfig,
    UpsertAgentMemoryJobConfigRequest, UpsertAiModelConfigRequest, UpsertSummaryJobConfigRequest,
    UpsertSummaryRollupJobConfigRequest,
};

use super::{auth::ADMIN_USER_ID, now_rfc3339};

fn model_collection(db: &Db) -> mongodb::Collection<AiModelConfig> {
    db.collection::<AiModelConfig>("ai_model_configs")
}

fn summary_job_collection(db: &Db) -> mongodb::Collection<SummaryJobConfig> {
    db.collection::<SummaryJobConfig>("summary_job_configs")
}

fn summary_rollup_collection(db: &Db) -> mongodb::Collection<SummaryRollupJobConfig> {
    db.collection::<SummaryRollupJobConfig>("summary_rollup_job_configs")
}

fn agent_memory_job_collection(db: &Db) -> mongodb::Collection<AgentMemoryJobConfig> {
    db.collection::<AgentMemoryJobConfig>("agent_memory_job_configs")
}

pub async fn list_model_configs(db: &Db, user_id: &str) -> Result<Vec<AiModelConfig>, String> {
    let options = FindOptions::builder().sort(doc! {"updated_at": -1}).build();
    let cursor = model_collection(db)
        .find(doc! {"user_id": user_id})
        .with_options(options)
        .await
        .map_err(|e| e.to_string())?;
    cursor.try_collect().await.map_err(|e| e.to_string())
}

pub async fn get_model_config_by_id(db: &Db, id: &str) -> Result<Option<AiModelConfig>, String> {
    model_collection(db)
        .find_one(doc! {"id": id})
        .await
        .map_err(|e| e.to_string())
}

pub async fn create_model_config(
    db: &Db,
    req: UpsertAiModelConfigRequest,
) -> Result<AiModelConfig, String> {
    let now = now_rfc3339();
    let model = AiModelConfig {
        id: Uuid::new_v4().to_string(),
        user_id: req.user_id,
        name: req.name,
        provider: req.provider,
        model: req.model,
        base_url: req.base_url,
        api_key: req.api_key,
        supports_images: if req.supports_images.unwrap_or(false) {
            1
        } else {
            0
        },
        supports_reasoning: if req.supports_reasoning.unwrap_or(false) {
            1
        } else {
            0
        },
        supports_responses: if req.supports_responses.unwrap_or(false) {
            1
        } else {
            0
        },
        temperature: req.temperature,
        thinking_level: req.thinking_level,
        enabled: if req.enabled.unwrap_or(true) { 1 } else { 0 },
        created_at: now.clone(),
        updated_at: now,
    };

    model_collection(db)
        .insert_one(model.clone())
        .await
        .map_err(|e| e.to_string())?;

    Ok(model)
}

pub async fn update_model_config(
    db: &Db,
    id: &str,
    req: UpsertAiModelConfigRequest,
) -> Result<Option<AiModelConfig>, String> {
    let existing = get_model_config_by_id(db, id).await?;
    let Some(existing) = existing else {
        return Ok(None);
    };

    let updated = AiModelConfig {
        id: existing.id,
        user_id: req.user_id,
        name: req.name,
        provider: req.provider,
        model: req.model,
        base_url: req.base_url.or(existing.base_url),
        api_key: req.api_key.or(existing.api_key),
        supports_images: if req.supports_images.unwrap_or(existing.supports_images == 1) {
            1
        } else {
            0
        },
        supports_reasoning: if req
            .supports_reasoning
            .unwrap_or(existing.supports_reasoning == 1)
        {
            1
        } else {
            0
        },
        supports_responses: if req
            .supports_responses
            .unwrap_or(existing.supports_responses == 1)
        {
            1
        } else {
            0
        },
        temperature: req.temperature.or(existing.temperature),
        thinking_level: req.thinking_level.or(existing.thinking_level),
        enabled: if req.enabled.unwrap_or(existing.enabled == 1) {
            1
        } else {
            0
        },
        created_at: existing.created_at,
        updated_at: now_rfc3339(),
    };

    model_collection(db)
        .replace_one(doc! {"id": id}, updated.clone())
        .await
        .map_err(|e| e.to_string())?;

    Ok(Some(updated))
}

pub async fn delete_model_config(db: &Db, id: &str) -> Result<bool, String> {
    let result = model_collection(db)
        .delete_one(doc! {"id": id})
        .await
        .map_err(|e| e.to_string())?;
    Ok(result.deleted_count > 0)
}

pub async fn delete_user_configs(db: &Db, user_id: &str) -> Result<(), String> {
    model_collection(db)
        .delete_many(doc! {"user_id": user_id})
        .await
        .map_err(|e| e.to_string())?;

    summary_job_collection(db)
        .delete_one(doc! {"user_id": user_id})
        .await
        .map_err(|e| e.to_string())?;

    summary_rollup_collection(db)
        .delete_one(doc! {"user_id": user_id})
        .await
        .map_err(|e| e.to_string())?;

    agent_memory_job_collection(db)
        .delete_one(doc! {"user_id": user_id})
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

fn default_summary_job_config(user_id: &str) -> SummaryJobConfig {
    SummaryJobConfig {
        user_id: user_id.to_string(),
        enabled: 1,
        summary_model_config_id: None,
        token_limit: 6000,
        round_limit: 8,
        target_summary_tokens: 700,
        job_interval_seconds: 30,
        max_sessions_per_tick: 50,
        updated_at: now_rfc3339(),
    }
}

pub async fn get_summary_job_config(
    db: &Db,
    user_id: &str,
) -> Result<Option<SummaryJobConfig>, String> {
    fetch_summary_job_config(db, user_id).await
}

async fn fetch_summary_job_config(
    db: &Db,
    user_id: &str,
) -> Result<Option<SummaryJobConfig>, String> {
    summary_job_collection(db)
        .find_one(doc! {"user_id": user_id})
        .await
        .map_err(|e| e.to_string())
}

pub async fn get_effective_summary_job_config(
    db: &Db,
    user_id: &str,
) -> Result<SummaryJobConfig, String> {
    if let Some(cfg) = fetch_summary_job_config(db, user_id).await? {
        return Ok(cfg);
    }

    if user_id != ADMIN_USER_ID {
        if let Some(admin_cfg) = fetch_summary_job_config(db, ADMIN_USER_ID).await? {
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

    Ok(default_summary_job_config(user_id))
}

pub async fn upsert_summary_job_config(
    db: &Db,
    req: UpsertSummaryJobConfigRequest,
) -> Result<SummaryJobConfig, String> {
    let mut current = fetch_summary_job_config(db, req.user_id.as_str())
        .await?
        .unwrap_or_else(|| default_summary_job_config(req.user_id.as_str()));

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

    summary_job_collection(db)
        .replace_one(doc! {"user_id": &current.user_id}, current.clone())
        .upsert(true)
        .await
        .map_err(|e| e.to_string())?;

    Ok(current)
}

fn default_agent_memory_job_config(user_id: &str) -> AgentMemoryJobConfig {
    AgentMemoryJobConfig {
        user_id: user_id.to_string(),
        enabled: 1,
        summary_model_config_id: None,
        token_limit: 6000,
        round_limit: 20,
        target_summary_tokens: 700,
        job_interval_seconds: 60,
        keep_raw_level0_count: 5,
        max_level: 4,
        max_agents_per_tick: 50,
        updated_at: now_rfc3339(),
    }
}

pub async fn get_agent_memory_job_config(
    db: &Db,
    user_id: &str,
) -> Result<Option<AgentMemoryJobConfig>, String> {
    fetch_agent_memory_job_config(db, user_id).await
}

async fn fetch_agent_memory_job_config(
    db: &Db,
    user_id: &str,
) -> Result<Option<AgentMemoryJobConfig>, String> {
    agent_memory_job_collection(db)
        .find_one(doc! {"user_id": user_id})
        .await
        .map_err(|e| e.to_string())
}

pub async fn get_effective_agent_memory_job_config(
    db: &Db,
    user_id: &str,
) -> Result<AgentMemoryJobConfig, String> {
    if let Some(cfg) = fetch_agent_memory_job_config(db, user_id).await? {
        return Ok(cfg);
    }

    if user_id != ADMIN_USER_ID {
        if let Some(admin_cfg) = fetch_agent_memory_job_config(db, ADMIN_USER_ID).await? {
            return Ok(AgentMemoryJobConfig {
                user_id: user_id.to_string(),
                enabled: admin_cfg.enabled,
                summary_model_config_id: admin_cfg.summary_model_config_id,
                token_limit: admin_cfg.token_limit,
                round_limit: admin_cfg.round_limit,
                target_summary_tokens: admin_cfg.target_summary_tokens,
                job_interval_seconds: admin_cfg.job_interval_seconds,
                keep_raw_level0_count: admin_cfg.keep_raw_level0_count,
                max_level: admin_cfg.max_level,
                max_agents_per_tick: admin_cfg.max_agents_per_tick,
                updated_at: admin_cfg.updated_at,
            });
        }
    }

    Ok(default_agent_memory_job_config(user_id))
}

pub async fn upsert_agent_memory_job_config(
    db: &Db,
    req: UpsertAgentMemoryJobConfigRequest,
) -> Result<AgentMemoryJobConfig, String> {
    let mut current = fetch_agent_memory_job_config(db, req.user_id.as_str())
        .await?
        .unwrap_or_else(|| default_agent_memory_job_config(req.user_id.as_str()));

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
    if let Some(v) = req.keep_raw_level0_count {
        current.keep_raw_level0_count = v.max(0);
    }
    if let Some(v) = req.max_level {
        current.max_level = v.max(1);
    }
    if let Some(v) = req.max_agents_per_tick {
        current.max_agents_per_tick = v.max(1);
    }

    current.updated_at = now_rfc3339();

    agent_memory_job_collection(db)
        .replace_one(doc! {"user_id": &current.user_id}, current.clone())
        .upsert(true)
        .await
        .map_err(|e| e.to_string())?;

    Ok(current)
}

fn default_summary_rollup_job_config(user_id: &str) -> SummaryRollupJobConfig {
    SummaryRollupJobConfig {
        user_id: user_id.to_string(),
        enabled: 1,
        summary_model_config_id: None,
        token_limit: 6000,
        round_limit: 50,
        target_summary_tokens: 700,
        job_interval_seconds: 60,
        keep_raw_level0_count: 5,
        max_level: 4,
        max_sessions_per_tick: 50,
        updated_at: now_rfc3339(),
    }
}

pub async fn get_summary_rollup_job_config(
    db: &Db,
    user_id: &str,
) -> Result<Option<SummaryRollupJobConfig>, String> {
    fetch_summary_rollup_job_config(db, user_id).await
}

async fn fetch_summary_rollup_job_config(
    db: &Db,
    user_id: &str,
) -> Result<Option<SummaryRollupJobConfig>, String> {
    summary_rollup_collection(db)
        .find_one(doc! {"user_id": user_id})
        .await
        .map_err(|e| e.to_string())
}

pub async fn get_effective_summary_rollup_job_config(
    db: &Db,
    user_id: &str,
) -> Result<SummaryRollupJobConfig, String> {
    if let Some(cfg) = fetch_summary_rollup_job_config(db, user_id).await? {
        return Ok(cfg);
    }

    if user_id != ADMIN_USER_ID {
        if let Some(admin_cfg) = fetch_summary_rollup_job_config(db, ADMIN_USER_ID).await? {
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

    Ok(default_summary_rollup_job_config(user_id))
}

pub async fn upsert_summary_rollup_job_config(
    db: &Db,
    req: UpsertSummaryRollupJobConfigRequest,
) -> Result<SummaryRollupJobConfig, String> {
    let mut current = fetch_summary_rollup_job_config(db, req.user_id.as_str())
        .await?
        .unwrap_or_else(|| default_summary_rollup_job_config(req.user_id.as_str()));

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
        current.round_limit = v.max(3);
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

    summary_rollup_collection(db)
        .replace_one(doc! {"user_id": &current.user_id}, current.clone())
        .upsert(true)
        .await
        .map_err(|e| e.to_string())?;

    Ok(current)
}
