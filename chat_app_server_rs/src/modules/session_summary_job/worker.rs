use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

use tokio::sync::Mutex;
use tokio::time::{self, MissedTickBehavior};
use tracing::{info, warn};

use crate::models::message::MessageService;
use crate::models::session::SessionService;

use super::config;
use super::executor;
use super::types::SummaryJobDefaults;

#[derive(Default)]
struct WorkerState {
    last_checked_at_by_session: HashMap<String, i64>,
}

impl WorkerState {
    fn is_due(&self, session_id: &str, interval_seconds: i64, now_ts: i64) -> bool {
        let interval = interval_seconds.max(10);
        match self.last_checked_at_by_session.get(session_id) {
            Some(last) => now_ts.saturating_sub(*last) >= interval,
            None => true,
        }
    }

    fn mark_checked(&mut self, session_id: &str, now_ts: i64) {
        self.last_checked_at_by_session
            .insert(session_id.to_string(), now_ts);
    }
}

fn should_delay_next_check(status: &str) -> bool {
    !matches!(status, "skipped:threshold_not_met" | "skipped:no_pending")
}

fn consumes_tick_quota(status: &str) -> bool {
    !matches!(status, "skipped:threshold_not_met" | "skipped:no_pending")
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
    let defaults = SummaryJobDefaults::from_env();
    if !defaults.enabled {
        info!("[SESSION-SUMMARY-JOB] disabled by env");
        return;
    }

    let base_interval_seconds = defaults.job_interval_seconds.max(10) as u64;
    let poll_interval_seconds = 10u64;
    info!(
        "[SESSION-SUMMARY-JOB] starting background worker, poll_interval={}s, default_interval={}s",
        poll_interval_seconds, base_interval_seconds
    );

    tokio::spawn(async move {
        let state = Arc::new(Mutex::new(WorkerState::default()));
        let running = Arc::new(AtomicBool::new(false));
        let mut ticker = time::interval(Duration::from_secs(poll_interval_seconds));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            ticker.tick().await;
            if running.swap(true, Ordering::AcqRel) {
                info!("[SESSION-SUMMARY-JOB] previous tick still running, skip this tick");
                continue;
            }

            let defaults_clone = defaults.clone();
            let state_clone = Arc::clone(&state);
            let running_clone = Arc::clone(&running);
            tokio::spawn(async move {
                let _guard = TickRunningGuard {
                    running: running_clone,
                };
                if let Err(err) = run_once(&defaults_clone, state_clone).await {
                    warn!("[SESSION-SUMMARY-JOB] tick failed: {}", err);
                }
            });
        }
    });
}

async fn run_once(
    defaults: &SummaryJobDefaults,
    state: Arc<Mutex<WorkerState>>,
) -> Result<(), String> {
    let session_ids = MessageService::list_sessions_with_pending_summary(None).await?;
    if session_ids.is_empty() {
        return Ok(());
    }

    let now_ts = chrono::Utc::now().timestamp();
    let max_sessions_per_tick = defaults.max_sessions_per_tick.max(1) as usize;
    let mut processed_sessions = 0usize;
    for session_id in session_ids {
        if processed_sessions >= max_sessions_per_tick {
            break;
        }

        let session = match SessionService::get_by_id(&session_id).await {
            Ok(Some(value)) => value,
            Ok(None) => continue,
            Err(err) => {
                warn!(
                    "[SESSION-SUMMARY-JOB] load session failed: session_id={} error={}",
                    session_id, err
                );
                continue;
            }
        };

        let effective = config::resolve_effective_config(&session, defaults).await;
        if !effective.enabled || effective.model_config_id.is_none() {
            continue;
        }

        let due = {
            let state_guard = state.lock().await;
            state_guard.is_due(&session_id, effective.job_interval_seconds, now_ts)
        };
        if !due {
            continue;
        }
        match executor::process_session(&session_id, defaults).await {
            Ok(outcome) => {
                if should_delay_next_check(outcome.status.as_str()) {
                    let mut state_guard = state.lock().await;
                    state_guard.mark_checked(&session_id, now_ts);
                }
                if consumes_tick_quota(outcome.status.as_str()) {
                    processed_sessions += 1;
                }
                info!(
                    "[SESSION-SUMMARY-JOB] processed session_id={} status={} trigger={} summary_id={} marked_messages={}",
                    session_id,
                    outcome.status,
                    outcome.trigger_type.unwrap_or_default(),
                    outcome.summary_id.unwrap_or_default(),
                    outcome.marked_messages,
                );
            }
            Err(err) => {
                {
                    let mut state_guard = state.lock().await;
                    state_guard.mark_checked(&session_id, now_ts);
                }
                processed_sessions += 1;
                warn!(
                    "[SESSION-SUMMARY-JOB] process session failed: session_id={} error={}",
                    session_id, err
                );
            }
        }
    }

    Ok(())
}
