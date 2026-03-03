use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

use tokio::sync::Mutex;
use tokio::time::{self, MissedTickBehavior};
use tracing::{info, warn};

use crate::models::sub_agent_run_message::SubAgentRunMessageService;

use super::config;
use super::executor;
use super::types::SummaryJobDefaults;

#[derive(Default)]
struct WorkerState {
    last_checked_at_by_run: HashMap<String, i64>,
}

impl WorkerState {
    fn is_due(&self, run_id: &str, interval_seconds: i64, now_ts: i64) -> bool {
        let interval = interval_seconds.max(10);
        match self.last_checked_at_by_run.get(run_id) {
            Some(last) => now_ts.saturating_sub(*last) >= interval,
            None => true,
        }
    }

    fn mark_checked(&mut self, run_id: &str, now_ts: i64) {
        self.last_checked_at_by_run
            .insert(run_id.to_string(), now_ts);
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
    let defaults = SummaryJobDefaults::from_env();
    if !defaults.enabled {
        info!("[SUB-AGENT-SUMMARY-JOB] disabled by env");
        return;
    }

    let base_interval_seconds = defaults.job_interval_seconds.max(10) as u64;
    let poll_interval_seconds = 10u64;
    info!(
        "[SUB-AGENT-SUMMARY-JOB] starting background worker, poll_interval={}s, default_interval={}s",
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
                info!("[SUB-AGENT-SUMMARY-JOB] previous tick still running, skip this tick");
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
                    warn!("[SUB-AGENT-SUMMARY-JOB] tick failed: {}", err);
                }
            });
        }
    });
}

async fn run_once(
    defaults: &SummaryJobDefaults,
    state: Arc<Mutex<WorkerState>>,
) -> Result<(), String> {
    let run_ids =
        SubAgentRunMessageService::list_runs_with_pending_summary(Some(defaults.max_runs_per_tick))
            .await?;
    if run_ids.is_empty() {
        return Ok(());
    }

    let now_ts = chrono::Utc::now().timestamp();
    for run_id in run_ids {
        let effective = match config::resolve_effective_config(&run_id, defaults).await {
            Ok(Some(value)) => value,
            Ok(None) => continue,
            Err(err) => {
                warn!(
                    "[SUB-AGENT-SUMMARY-JOB] resolve config failed: run_id={} error={}",
                    run_id, err
                );
                continue;
            }
        };
        if !effective.enabled {
            continue;
        }

        let due = {
            let state_guard = state.lock().await;
            state_guard.is_due(&run_id, effective.job_interval_seconds, now_ts)
        };
        if !due {
            continue;
        }
        {
            let mut state_guard = state.lock().await;
            state_guard.mark_checked(&run_id, now_ts);
        }

        match executor::process_run(&run_id, &effective).await {
            Ok(outcome) => {
                info!(
                    "[SUB-AGENT-SUMMARY-JOB] processed run_id={} status={} trigger={} summary_id={} marked_messages={}",
                    run_id,
                    outcome.status,
                    outcome.trigger_type.unwrap_or_default(),
                    outcome.summary_id.unwrap_or_default(),
                    outcome.marked_messages,
                );
            }
            Err(err) => {
                warn!(
                    "[SUB-AGENT-SUMMARY-JOB] process run failed: run_id={} error={}",
                    run_id, err
                );
            }
        }
    }

    Ok(())
}
