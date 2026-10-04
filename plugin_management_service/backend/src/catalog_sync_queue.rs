// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use tracing::{info, warn};

use crate::api::{is_syncable_network_marketplace, run_queued_plugin_catalog_sync};
use crate::models::PluginCatalogSyncOutboxEvent;
use crate::pressure::PlatformPressureLevel;
use crate::state::AppState;

const CATALOG_CLAIM_TTL: std::time::Duration = std::time::Duration::from_secs(5 * 60);

pub fn start(state: AppState) {
    if !state.config.plugin_catalog_sync_enabled {
        return;
    }
    for worker_index in 0..state.config.plugin_catalog_consumer_concurrency.max(1) {
        tokio::spawn(run_worker(state.clone(), worker_index));
    }
    tokio::spawn(run_reconciler(state));
}

pub async fn publish_pending_marketplace(
    state: &AppState,
    marketplace_id: &str,
) -> Result<bool, String> {
    if !state.config.plugin_catalog_sync_enabled {
        return Ok(false);
    }
    Ok(state
        .store
        .pending_plugin_catalog_sync_event(marketplace_id)
        .await?
        .is_some())
}

pub async fn replay_dead_lettered_marketplace(
    state: &AppState,
    marketplace_id: &str,
    dead_letter_version: i64,
) -> Result<Option<bool>, String> {
    Ok(state
        .store
        .replay_dead_lettered_plugin_catalog_sync(marketplace_id, dead_letter_version)
        .await?
        .map(|_| true))
}

async fn run_worker(state: AppState, worker_index: usize) {
    let mut interval = tokio::time::interval(state.config.plugin_catalog_outbox_reconcile_interval);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        match process_claimed_batch(&state).await {
            Ok(count) if count > 0 => info!(
                worker_index,
                processed_count = count,
                "Plugin Catalog database worker processed sync events"
            ),
            Ok(_) => {}
            Err(error) => warn!(worker_index, error, "Plugin Catalog database worker failed"),
        }
    }
}

async fn process_claimed_batch(state: &AppState) -> Result<usize, String> {
    let now = chrono::Utc::now();
    let claim_until = now
        + chrono::Duration::from_std(CATALOG_CLAIM_TTL)
            .map_err(|error| format!("invalid Plugin Catalog claim TTL: {error}"))?;
    let claim_token = uuid::Uuid::new_v4().to_string();
    let events = state
        .store
        .claim_pending_plugin_catalog_sync_events(
            state.config.plugin_catalog_outbox_batch_size,
            now,
            claim_token.as_str(),
            claim_until,
        )
        .await?;
    let mut processed = 0usize;
    for event in events {
        match process_event(state, &event).await {
            Ok(()) => processed = processed.saturating_add(1),
            Err(error) => {
                let retry_at = chrono::Utc::now()
                    + chrono::Duration::from_std(state.config.plugin_catalog_retry_delay)
                        .unwrap_or_else(|_| chrono::Duration::minutes(5));
                let dead_lettered = state
                    .store
                    .fail_plugin_catalog_sync_claim(
                        &event,
                        error.as_str(),
                        retry_at,
                        state.config.plugin_catalog_max_delivery_attempts,
                    )
                    .await?;
                warn!(
                    marketplace_id = event.marketplace_id,
                    event_version = event.event_version,
                    dead_lettered,
                    retry_delay_ms = state.config.plugin_catalog_retry_delay.as_millis(),
                    error,
                    "Plugin Catalog database sync failed"
                );
            }
        }
    }
    Ok(processed)
}

async fn process_event(
    state: &AppState,
    event: &PluginCatalogSyncOutboxEvent,
) -> Result<(), String> {
    let Some(consumed) = state
        .store
        .plugin_catalog_sync_event_consumed(event.marketplace_id.as_str(), event.event_version)
        .await?
    else {
        return Ok(());
    };
    if consumed {
        return Ok(());
    }

    let Some(marketplace) = state
        .store
        .get_plugin_marketplace(event.marketplace_id.as_str())
        .await?
    else {
        return Ok(());
    };
    if !is_syncable_network_marketplace(&marketplace) {
        state
            .store
            .complete_plugin_catalog_sync_event(event, None)
            .await?;
        return Ok(());
    }

    if should_defer_scheduled_sync(event, state.pressure.snapshot().level) {
        let available_at = chrono::Utc::now()
            + chrono::Duration::from_std(state.config.plugin_catalog_outbox_reconcile_interval)
                .unwrap_or_else(|_| chrono::Duration::seconds(1));
        state
            .store
            .defer_plugin_catalog_sync_claim(event, available_at, None)
            .await?;
        info!(
            marketplace_id = event.marketplace_id,
            event_version = event.event_version,
            "Plugin Catalog scheduled sync deferred by critical platform pressure"
        );
        return Ok(());
    }

    run_queued_plugin_catalog_sync(state, event.marketplace_id.as_str()).await?;
    let schedule_next = state
        .store
        .get_plugin_marketplace(event.marketplace_id.as_str())
        .await?
        .is_some_and(|marketplace| is_syncable_network_marketplace(&marketplace));
    let schedule_next_at = if schedule_next {
        Some(
            chrono::Utc::now()
                + chrono::Duration::from_std(state.config.plugin_catalog_sync_interval)
                    .map_err(|error| format!("invalid Plugin Catalog sync interval: {error}"))?,
        )
    } else {
        None
    };
    state
        .store
        .complete_plugin_catalog_sync_event(event, schedule_next_at)
        .await?;
    Ok(())
}

fn should_defer_scheduled_sync(
    event: &PluginCatalogSyncOutboxEvent,
    pressure_level: PlatformPressureLevel,
) -> bool {
    event.scheduled && pressure_level == PlatformPressureLevel::Critical
}

async fn run_reconciler(state: AppState) {
    let mut interval = tokio::time::interval(state.config.plugin_catalog_outbox_reconcile_interval);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        let recovered_claims = state
            .store
            .recover_stale_plugin_catalog_sync_claims(state.config.plugin_catalog_outbox_batch_size)
            .await;
        let recovered_schedules = state
            .store
            .recover_plugin_catalog_sync_events(state.config.plugin_catalog_outbox_batch_size)
            .await;
        match (recovered_claims, recovered_schedules) {
            (Ok(claims), Ok(schedules)) if claims + schedules > 0 => info!(
                recovered_claims = claims,
                armed_schedules = schedules,
                "Plugin Management reconciled Catalog database workers"
            ),
            (Ok(_), Ok(_)) => {}
            (claims, schedules) => warn!(
                claim_error = claims.err().as_deref().unwrap_or(""),
                schedule_error = schedules.err().as_deref().unwrap_or(""),
                "Plugin Management failed to reconcile Catalog database workers"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::should_defer_scheduled_sync;
    use crate::models::PluginCatalogSyncOutboxEvent;
    use crate::pressure::PlatformPressureLevel;

    fn event(scheduled: bool) -> PluginCatalogSyncOutboxEvent {
        PluginCatalogSyncOutboxEvent {
            marketplace_id: "marketplace-1".to_string(),
            event_version: 4,
            requested_at: "2026-08-05T00:00:00Z".to_string(),
            scheduled,
        }
    }

    #[test]
    fn scheduled_sync_is_deferred_only_for_critical_pressure() {
        assert!(should_defer_scheduled_sync(
            &event(true),
            PlatformPressureLevel::Critical
        ));
        assert!(!should_defer_scheduled_sync(
            &event(true),
            PlatformPressureLevel::Elevated
        ));
        assert!(!should_defer_scheduled_sync(
            &event(false),
            PlatformPressureLevel::Critical
        ));
    }
}
