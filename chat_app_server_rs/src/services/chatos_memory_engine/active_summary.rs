use std::time::Duration;

use memory_engine_sdk::RunThreadActiveSummaryResponse;
use tokio::time::{sleep, Instant};
use tracing::{info, warn};

use crate::config::Config;
use crate::models::session::Session;

use super::client::{build_active_summary_trigger_client, build_client};
use super::mapping::build_thread_mapping;

pub async fn run_chatos_active_summary(
    session: &Session,
    trigger_reason: &str,
) -> Result<RunThreadActiveSummaryResponse, String> {
    let mapping = build_thread_mapping(session)?;
    let client = build_active_summary_trigger_client()?;
    let status = client
        .run_thread_active_summary(
            mapping.thread_id.as_str(),
            mapping.tenant_id.as_str(),
            Some(trigger_reason),
        )
        .await?;
    info!(
        "[CHATOS-ACTIVE-SUMMARY] trigger session_id={} running={} completed={} failed={} generated={} compacted={} job_run_id={}",
        session.id,
        status.running,
        status.completed,
        status.failed,
        status.generated,
        status.compacted,
        status.job_run_id.as_deref().unwrap_or("-")
    );
    Ok(status)
}

pub async fn get_chatos_active_summary_status(
    session: &Session,
    job_run_id: Option<&str>,
) -> Result<RunThreadActiveSummaryResponse, String> {
    let mapping = build_thread_mapping(session)?;
    let client = build_client()?;
    let status = client
        .get_thread_active_summary_status(
            mapping.thread_id.as_str(),
            mapping.tenant_id.as_str(),
            job_run_id,
        )
        .await?;
    info!(
        "[CHATOS-ACTIVE-SUMMARY] status session_id={} running={} completed={} failed={} generated={} compacted={} job_run_id={}",
        session.id,
        status.running,
        status.completed,
        status.failed,
        status.generated,
        status.compacted,
        status.job_run_id.as_deref().unwrap_or("-")
    );
    Ok(status)
}

pub async fn try_start_chatos_active_summary(
    session: &Session,
    trigger_reason: &str,
) -> Option<RunThreadActiveSummaryResponse> {
    match run_chatos_active_summary(session, trigger_reason).await {
        Ok(status) => Some(status),
        Err(err) => {
            warn!(
                "[CHATOS-ACTIVE-SUMMARY] trigger failed session_id={} error={}",
                session.id, err
            );
            None
        }
    }
}

pub async fn wait_for_existing_chatos_active_summary_completion(
    session: &Session,
    initial: RunThreadActiveSummaryResponse,
) -> Result<RunThreadActiveSummaryResponse, String> {
    let cfg = Config::try_get()?;
    let poll_interval =
        Duration::from_millis(cfg.memory_engine_active_summary_poll_interval_ms.max(1_000) as u64);
    let poll_timeout =
        Duration::from_millis(cfg.memory_engine_active_summary_poll_timeout_ms.max(10_000) as u64);
    if initial.completed || initial.failed || !initial.running {
        return Ok(initial);
    }

    let deadline = Instant::now() + poll_timeout;
    let job_run_id = initial.job_run_id.clone();
    loop {
        if Instant::now() >= deadline {
            return Err(format!(
                "active summary poll timed out after {} ms",
                poll_timeout.as_millis()
            ));
        }

        sleep(poll_interval).await;

        let status = get_chatos_active_summary_status(session, job_run_id.as_deref()).await?;
        if status.completed || status.failed || !status.running {
            return Ok(status);
        }
    }
}
