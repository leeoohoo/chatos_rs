use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

use tokio::time::{self, MissedTickBehavior};
use tracing::{info, warn};

use crate::models::session::SessionService;

#[derive(Debug, Clone)]
struct ArchiveJobConfig {
    enabled: bool,
    poll_interval_seconds: u64,
    max_sessions_per_tick: i64,
}

impl ArchiveJobConfig {
    fn from_env() -> Self {
        let enabled = std::env::var("SESSION_ARCHIVE_JOB_ENABLED")
            .unwrap_or_else(|_| "true".to_string())
            .to_lowercase()
            != "false";
        let poll_interval_seconds = std::env::var("SESSION_ARCHIVE_JOB_INTERVAL_SECONDS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(10)
            .max(5);
        let max_sessions_per_tick = std::env::var("SESSION_ARCHIVE_JOB_MAX_SESSIONS_PER_TICK")
            .ok()
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(20)
            .max(1);

        Self {
            enabled,
            poll_interval_seconds,
            max_sessions_per_tick,
        }
    }
}

struct TickRunningGuard {
    running: Arc<AtomicBool>,
}

impl Drop for TickRunningGuard {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Release);
    }
}

pub fn start_worker() {
    let config = ArchiveJobConfig::from_env();
    if !config.enabled {
        info!("[SESSION-ARCHIVE-JOB] disabled by env");
        return;
    }

    info!(
        "[SESSION-ARCHIVE-JOB] starting background worker, poll_interval={}s, max_sessions_per_tick={}",
        config.poll_interval_seconds, config.max_sessions_per_tick
    );

    tokio::spawn(async move {
        let running = Arc::new(AtomicBool::new(false));
        let mut ticker = time::interval(Duration::from_secs(config.poll_interval_seconds));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            ticker.tick().await;
            if running.swap(true, Ordering::AcqRel) {
                info!("[SESSION-ARCHIVE-JOB] previous tick still running, skip this tick");
                continue;
            }

            let config_clone = config.clone();
            let running_clone = Arc::clone(&running);
            tokio::spawn(async move {
                let _guard = TickRunningGuard {
                    running: running_clone,
                };
                if let Err(err) = run_once(&config_clone).await {
                    warn!("[SESSION-ARCHIVE-JOB] tick failed: {}", err);
                }
            });
        }
    });
}

async fn run_once(config: &ArchiveJobConfig) -> Result<(), String> {
    let session_ids = SessionService::list_archiving(Some(config.max_sessions_per_tick)).await?;
    if session_ids.is_empty() {
        return Ok(());
    }

    for session_id in session_ids {
        match SessionService::process_archive(&session_id).await {
            Ok(_) => {
                info!(
                    "[SESSION-ARCHIVE-JOB] archived session completed: session_id={}",
                    session_id
                );
            }
            Err(err) => {
                warn!(
                    "[SESSION-ARCHIVE-JOB] archive session failed: session_id={} error={}",
                    session_id, err
                );
            }
        }
    }

    Ok(())
}
