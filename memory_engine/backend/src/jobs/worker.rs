// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;
use std::time::Duration;

use tokio::time::{self, MissedTickBehavior};
use tracing::{info, warn};

use crate::repositories::control_plane;
use crate::state::AppState;

pub fn start(state: Arc<AppState>) {
    tokio::spawn(async move {
        let mut ticker = time::interval(Duration::from_secs(state.config.worker_interval_secs));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

        info!(
            "[MEMORY-ENGINE-WORKER] started maintenance tick={}s",
            state.config.worker_interval_secs,
        );

        loop {
            ticker.tick().await;
            let tick_started_at = time::Instant::now();

            if let Err(err) = control_plane::fail_stale_running_job_runs(&state.pool, 300).await {
                warn!("[MEMORY-ENGINE-WORKER] stale job cleanup failed: {}", err);
            }

            state
                .runtime_stats
                .record_worker_tick(tick_started_at.elapsed());
        }
    });
}
