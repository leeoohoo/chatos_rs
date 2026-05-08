use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use tokio::time::{self, MissedTickBehavior};
use tracing::{info, warn};

use crate::repositories::{configs, sessions};
use crate::state::AppState;

use super::agent_memory;

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
        let summary_cfg =
            configs::get_effective_summary_job_config(&state.pool, user_id.as_str()).await?;
        if summary_cfg.enabled == 1 {
            let key = format!("summary:{}", user_id);
            if is_due(
                &worker_state,
                key.as_str(),
                now_ts,
                summary_cfg.job_interval_seconds,
            ) {
                match crate::services::memory_engine_client::run_pending_summaries_once(
                    &state.config,
                    Some(user_id.as_str()),
                    Some(summary_cfg.max_sessions_per_tick.max(1)),
                )
                .await
                {
                    Ok(result) => {
                        info!(
                            "[MEMORY-WORKER] engine summary run user_id={} processed_threads={} summarized_threads={} max_sessions_per_tick={}",
                            user_id,
                            result.processed_threads,
                            result.summarized_threads,
                            summary_cfg.max_sessions_per_tick
                        );
                    }
                    Err(err) => {
                        warn!(
                            "[MEMORY-WORKER] engine summary run failed user_id={} error={}",
                            user_id, err
                        );
                    }
                }
                mark_run(&worker_state, key.as_str(), now_ts);
            }
        }

        let rollup_cfg =
            configs::get_effective_summary_rollup_job_config(&state.pool, user_id.as_str()).await?;
        if rollup_cfg.enabled == 1 {
            let key = format!("rollup:{}", user_id);
            if is_due(
                &worker_state,
                key.as_str(),
                now_ts,
                rollup_cfg.job_interval_seconds,
            ) {
                let result = crate::services::memory_engine_client::run_pending_rollups_once(
                    &state.config,
                    user_id.as_str(),
                    &rollup_cfg,
                )
                .await;
                match result {
                    Ok(result) => {
                        info!(
                            "[MEMORY-WORKER] engine rollup run user_id={} processed={} rolled_up={} generated={} marked={} failed={}",
                            user_id,
                            result.processed_threads,
                            result.rolled_up_threads,
                            result.generated_summaries,
                            result.marked_summaries,
                            result.failed_threads
                        );
                    }
                    Err(err) => {
                        warn!(
                            "[MEMORY-WORKER] engine rollup run failed user_id={} error={}",
                            user_id, err
                        );
                    }
                }
                mark_run(&worker_state, key.as_str(), now_ts);
            }
        }

        let agent_memory_cfg =
            configs::get_effective_agent_memory_job_config(&state.pool, user_id.as_str()).await?;
        if agent_memory_cfg.enabled == 1 {
            let key = format!("agent_memory:{}", user_id);
            if is_due(
                &worker_state,
                key.as_str(),
                now_ts,
                agent_memory_cfg.job_interval_seconds,
            ) {
                let result =
                    agent_memory::run_once(&state.pool, &state.config, user_id.as_str())
                        .await;
                match result {
                    Ok(result) => {
                        info!(
                            "[MEMORY-WORKER] agent memory run user_id={} processed={} summarized={} generated={} marked_summaries={} marked_recalls={} failed={}",
                            user_id,
                            result.processed_agents,
                            result.summarized_agents,
                            result.generated_recalls,
                            result.marked_source_summaries,
                            result.marked_source_recalls,
                            result.failed_agents
                        );
                    }
                    Err(err) => {
                        warn!(
                            "[MEMORY-WORKER] agent memory run failed user_id={} error={}",
                            user_id, err
                        );
                    }
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
