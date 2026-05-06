use tracing::warn;

use crate::db::Db;
use crate::models::AiModelConfig;
use crate::repositories::{auth::ADMIN_USER_ID, configs, jobs};

pub(crate) async fn finish_failed_job_run(
    pool: &Db,
    job_run_id: &str,
    error_message: &str,
    log_prefix: &str,
) {
    if let Err(err) = jobs::finish_job_run(pool, job_run_id, "failed", 0, Some(error_message)).await
    {
        warn!(
            "{} mark job failed status failed: job_run_id={} error={}",
            log_prefix, job_run_id, err
        );
    }
}

pub(crate) async fn update_failed_job_run_diagnostics(
    pool: &Db,
    job_run_id: &str,
    pending_before_count: Option<i64>,
    selected_count: Option<i64>,
    marked_count: Option<i64>,
    pending_after_count: Option<i64>,
    log_prefix: &str,
) {
    if let Err(err) = jobs::update_job_run_diagnostics(
        pool,
        job_run_id,
        pending_before_count,
        selected_count,
        marked_count,
        pending_after_count,
    )
    .await
    {
        warn!(
            "{} update failed job diagnostics failed: job_run_id={} error={}",
            log_prefix, job_run_id, err
        );
    }
}

pub(crate) async fn resolve_model_config(
    pool: &Db,
    user_id: &str,
    model_config_id: Option<&str>,
) -> Result<Option<AiModelConfig>, String> {
    if let Some(id) = model_config_id {
        if let Some(cfg) = configs::get_model_config_by_id(pool, id).await? {
            if (cfg.user_id == user_id || cfg.user_id == ADMIN_USER_ID) && cfg.enabled == 1 {
                return Ok(Some(cfg));
            }
        }
    }

    let all = configs::list_model_configs(pool, user_id).await?;
    if let Some(cfg) = all.into_iter().find(|c| c.enabled == 1) {
        return Ok(Some(cfg));
    }

    if user_id != ADMIN_USER_ID {
        let admin_all = configs::list_model_configs(pool, ADMIN_USER_ID).await?;
        return Ok(admin_all.into_iter().find(|c| c.enabled == 1));
    }

    Ok(None)
}

pub(crate) fn resolve_tick_concurrency(
    max_items_per_tick: i64,
    env_key: &str,
    default_limit: usize,
) -> usize {
    let configured = std::env::var(env_key)
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(default_limit.max(1));
    let cap = max_items_per_tick.max(1) as usize;
    configured.min(cap).max(1)
}

pub(crate) fn resolve_lock_lease_seconds() -> i64 {
    std::env::var("MEMORY_SERVER_JOB_LOCK_LEASE_SECONDS")
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(1800)
        .max(60)
}

pub(crate) fn resolve_job_timeout_seconds() -> u64 {
    std::env::var("MEMORY_SERVER_JOB_TIMEOUT_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(180)
        .max(30)
}
