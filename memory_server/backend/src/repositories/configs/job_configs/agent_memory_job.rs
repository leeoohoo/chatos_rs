use mongodb::bson::doc;

use crate::db::Db;
use crate::models::{AgentMemoryJobConfig, UpsertAgentMemoryJobConfigRequest};

use super::super::super::{auth::ADMIN_USER_ID, now_rfc3339};
use super::shared::agent_memory_job_collection;

fn default_agent_memory_job_config(user_id: &str) -> AgentMemoryJobConfig {
    AgentMemoryJobConfig {
        user_id: user_id.to_string(),
        enabled: 1,
        summary_model_config_id: None,
        token_limit: 6000,
        round_limit: 20,
        target_summary_tokens: 700,
        job_interval_seconds: 60,
        keep_raw_level0_count: 0,
        max_level: 4,
        max_agents_per_tick: 50,
        updated_at: now_rfc3339(),
    }
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

pub async fn get_agent_memory_job_config(
    db: &Db,
    user_id: &str,
) -> Result<Option<AgentMemoryJobConfig>, String> {
    fetch_agent_memory_job_config(db, user_id).await
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
