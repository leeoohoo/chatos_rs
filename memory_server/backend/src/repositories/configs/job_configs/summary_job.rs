use mongodb::bson::doc;

use crate::db::Db;
use crate::models::{SummaryJobConfig, UpsertSummaryJobConfigRequest};

use super::super::super::{auth::ADMIN_USER_ID, now_rfc3339};
use super::shared::summary_job_collection;

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

async fn fetch_summary_job_config(
    db: &Db,
    user_id: &str,
) -> Result<Option<SummaryJobConfig>, String> {
    summary_job_collection(db)
        .find_one(doc! {"user_id": user_id})
        .await
        .map_err(|e| e.to_string())
}

pub async fn get_summary_job_config(
    db: &Db,
    user_id: &str,
) -> Result<Option<SummaryJobConfig>, String> {
    fetch_summary_job_config(db, user_id).await
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
