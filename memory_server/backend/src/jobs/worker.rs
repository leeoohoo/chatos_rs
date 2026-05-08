use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use tokio::time::{self, MissedTickBehavior};
use tracing::{info, warn};

use crate::repositories::sessions;
use crate::state::AppState;

#[derive(Default)]
struct WorkerState {
    last_run_ts: HashMap<String, i64>,
}

pub fn start(state: Arc<AppState>) {
    tokio::spawn(async move {
        let worker_state = Arc::new(Mutex::new(WorkerState::default()));
        let mut ticker = time::interval(Duration::from_secs(10));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

        info!("[MEMORY-WORKER] started, tick=10s");

        loop {
            ticker.tick().await;
            if let Err(err) = tick_once(state.clone(), worker_state.clone()).await {
                warn!("[MEMORY-WORKER] tick failed: {}", err);
            }
        }
    });
}

async fn tick_once(
    state: Arc<AppState>,
    worker_state: Arc<Mutex<WorkerState>>,
) -> Result<(), String> {
    let user_ids = sessions::list_active_user_ids(&state.pool, 500).await?;
    if user_ids.is_empty() {
        return Ok(());
    }

    let now_ts = chrono::Utc::now().timestamp();

    for user_id in user_ids {
        let summary_key = format!("summary:{}", user_id);
        if is_due(&worker_state, summary_key.as_str(), now_ts, 60) {
            info!(
                "[MEMORY-WORKER] skip summary dispatch for user_id={} because memory_engine now owns summary scheduling",
                user_id
            );
            mark_run(&worker_state, summary_key.as_str(), now_ts);
        }

        let rollup_key = format!("rollup:{}", user_id);
        if is_due(&worker_state, rollup_key.as_str(), now_ts, 60) {
            info!(
                "[MEMORY-WORKER] skip rollup dispatch for user_id={} because memory_engine now owns rollup scheduling",
                user_id
            );
            mark_run(&worker_state, rollup_key.as_str(), now_ts);
        }

        let key = format!("agent_memory:{}", user_id);
        if is_due(&worker_state, key.as_str(), now_ts, 60) {
            info!(
                "[MEMORY-WORKER] skip agent memory dispatch for user_id={} because memory_engine now owns subject_memory scheduling",
                user_id
            );
            mark_run(&worker_state, key.as_str(), now_ts);
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
