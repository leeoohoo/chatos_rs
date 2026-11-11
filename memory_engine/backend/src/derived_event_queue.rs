// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use tracing::{info, warn};

use crate::repositories::derived_events;
use crate::state::AppState;

const PUBLISH_LEASE_SECONDS: i64 = 300;
const PUBLISH_MAX_ATTEMPTS: i64 = 8;

pub fn start(state: Arc<AppState>, service_id: String) {
    tokio::spawn(run(state, service_id));
}

async fn run(state: Arc<AppState>, service_id: String) {
    let mut interval = tokio::time::interval(state.config.summary_outbox_reconcile_interval);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        match publish_batch(&state, service_id.as_str()).await {
            Ok(count) if count > 0 => info!(
                published_count = count,
                "Memory Engine published derived database events"
            ),
            Ok(_) => {}
            Err(error) => warn!(error, "Memory Engine derived event publisher failed"),
        }
    }
}

async fn publish_batch(state: &AppState, service_id: &str) -> Result<usize, String> {
    let events = derived_events::claim_publish_batch(
        &state.pool,
        service_id,
        PUBLISH_LEASE_SECONDS,
        state.config.summary_outbox_batch_size,
    )
    .await?;
    let mut published = 0usize;
    for event in events {
        match derived_events::mark_published(&state.pool, &event).await {
            Ok(true) => published = published.saturating_add(1),
            Ok(false) => {}
            Err(error) => {
                let error_code = classify_publish_error(error.as_str());
                let status = derived_events::mark_publish_failed(
                    &state.pool,
                    &event,
                    error_code,
                    PUBLISH_MAX_ATTEMPTS,
                    state.config.summary_retry_delay.as_secs() as i64,
                )
                .await?;
                warn!(
                    event_id = event.event_id,
                    dispatch_version = event.dispatch_version,
                    status = status.as_deref().unwrap_or("lease_lost"),
                    error,
                    "Memory Engine could not publish derived database event"
                );
            }
        }
    }
    Ok(published)
}

fn classify_publish_error(error: &str) -> &'static str {
    if error.contains("missing") || error.contains("behind") {
        "invalid_durable_state"
    } else {
        "publisher_error"
    }
}

#[cfg(test)]
mod tests {
    use super::classify_publish_error;

    #[test]
    fn publisher_error_codes_do_not_persist_raw_details() {
        assert_eq!(
            classify_publish_error("thread partition is missing"),
            "invalid_durable_state"
        );
        assert_eq!(
            classify_publish_error("credential=secret"),
            "publisher_error"
        );
    }
}
