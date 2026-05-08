use std::sync::Arc;
use std::time::Duration;

use tokio::time::{self, MissedTickBehavior};
use tracing::{info, warn};

use crate::state::AppState;

use super::summary_jobs;

pub fn start(state: Arc<AppState>) {
    tokio::spawn(async move {
        let mut ticker = time::interval(Duration::from_secs(state.config.worker_interval_secs));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

        info!(
            "[MEMORY-ENGINE-WORKER] started tick={}s max_threads_per_tick={}",
            state.config.worker_interval_secs,
            state.config.worker_max_threads_per_tick
        );

        loop {
            ticker.tick().await;
            match summary_jobs::run_pending_thread_summaries(&state.config, &state.pool).await {
                Ok(result) if result.summarized_threads > 0 || result.processed_threads > 0 => {
                    info!(
                        "[MEMORY-ENGINE-WORKER] tick processed_threads={} summarized_threads={}",
                        result.processed_threads, result.summarized_threads
                    );
                }
                Ok(_) => {}
                Err(err) => warn!("[MEMORY-ENGINE-WORKER] tick failed: {}", err),
            }

            let rollup_settings = crate::services::summary::default_rollup_settings();
            match summary_jobs::run_pending_thread_rollups(
                &state.pool,
                &state.config,
                None,
                None,
                state.config.worker_max_threads_per_tick,
                &rollup_settings,
            )
            .await
            {
                Ok(result)
                    if result.generated_summaries > 0
                        || result.marked_summaries > 0
                        || result.processed_threads > 0 =>
                {
                    info!(
                        "[MEMORY-ENGINE-WORKER] rollup processed_threads={} rolled_up_threads={} generated_summaries={} marked_summaries={} failed_threads={}",
                        result.processed_threads,
                        result.rolled_up_threads,
                        result.generated_summaries,
                        result.marked_summaries,
                        result.failed_threads
                    );
                }
                Ok(_) => {}
                Err(err) => warn!("[MEMORY-ENGINE-WORKER] rollup tick failed: {}", err),
            }
        }
    });
}
