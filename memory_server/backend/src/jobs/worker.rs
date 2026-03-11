use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use tokio::time::{self, MissedTickBehavior};
use tracing::{info, warn};

use crate::ai::AiClient;
use crate::repositories::{configs, sessions};
use crate::state::AppState;

use super::{rollup, summary};

#[derive(Default)]
struct WorkerState {
    last_run_ts: HashMap<String, i64>,
}

pub fn start(state: Arc<AppState>, ai: AiClient) {
    tokio::spawn(async move {
        let worker_state = Arc::new(Mutex::new(WorkerState::default()));
        let mut ticker = time::interval(Duration::from_secs(10));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

        info!("[MEMORY-WORKER] started, tick=10s");

        loop {
            ticker.tick().await;
            if let Err(err) = tick_once(state.clone(), ai.clone(), worker_state.clone()).await {
                warn!("[MEMORY-WORKER] tick failed: {}", err);
            }
        }
    });
}

async fn tick_once(
    state: Arc<AppState>,
    ai: AiClient,
    worker_state: Arc<Mutex<WorkerState>>,
) -> Result<(), String> {
    let user_ids = sessions::list_active_user_ids(&state.pool, 500).await?;
    if user_ids.is_empty() {
        return Ok(());
    }

    let now_ts = chrono::Utc::now().timestamp();

    for user_id in user_ids {
        let summary_cfg = configs::get_summary_job_config(&state.pool, user_id.as_str()).await?;
        if summary_cfg.enabled == 1 {
            let key = format!("summary:{}", user_id);
            if is_due(&worker_state, key.as_str(), now_ts, summary_cfg.job_interval_seconds) {
                let result = summary::run_once(&state.pool, &ai, user_id.as_str()).await;
                if let Err(err) = result {
                    warn!("[MEMORY-WORKER] summary run failed user_id={} error={}", user_id, err);
                }
                mark_run(&worker_state, key.as_str(), now_ts);
            }
        }

        let rollup_cfg =
            configs::get_summary_rollup_job_config(&state.pool, user_id.as_str()).await?;
        if rollup_cfg.enabled == 1 {
            let key = format!("rollup:{}", user_id);
            if is_due(
                &worker_state,
                key.as_str(),
                now_ts,
                rollup_cfg.job_interval_seconds,
            ) {
                let result = rollup::run_once(&state.pool, &ai, user_id.as_str()).await;
                if let Err(err) = result {
                    warn!("[MEMORY-WORKER] rollup run failed user_id={} error={}", user_id, err);
                }
                mark_run(&worker_state, key.as_str(), now_ts);
            }
        }
    }

    Ok(())
}

fn is_due(worker_state: &Arc<Mutex<WorkerState>>, key: &str, now_ts: i64, interval: i64) -> bool {
    let interval = interval.max(10);
    let guard = worker_state.lock();
    match guard.last_run_ts.get(key) {
        Some(last) => now_ts.saturating_sub(*last) >= interval,
        None => true,
    }
}

fn mark_run(worker_state: &Arc<Mutex<WorkerState>>, key: &str, now_ts: i64) {
    let mut guard = worker_state.lock();
    guard.last_run_ts.insert(key.to_string(), now_ts);
}
