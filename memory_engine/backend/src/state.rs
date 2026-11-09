// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::config::AppConfig;
use crate::db::Db;
use crate::models::{
    ComposeContextResponse, MemoryEngineComposeBaselineStats, MemoryEngineWorkerRuntimeStats,
};
use crate::pressure::MemoryEnginePressureState;
use chatos_cloud_agent_runtime::CloudAgentStateStore;

#[derive(Default)]
pub struct MemoryEngineRuntimeStats {
    completed_ticks_total: AtomicU64,
    last_tick_duration_ms: AtomicU64,
    max_tick_duration_ms: AtomicU64,
    compose_requests_total: AtomicU64,
    compose_estimated_tokens_total: AtomicU64,
    compose_estimated_tokens_max: AtomicU64,
}

impl MemoryEngineRuntimeStats {
    pub fn record_worker_tick(&self, duration: Duration) {
        let duration_ms = duration.as_millis().min(u64::MAX as u128) as u64;
        self.completed_ticks_total.fetch_add(1, Ordering::Relaxed);
        self.last_tick_duration_ms
            .store(duration_ms, Ordering::Relaxed);
        self.max_tick_duration_ms
            .fetch_max(duration_ms, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> MemoryEngineWorkerRuntimeStats {
        MemoryEngineWorkerRuntimeStats {
            completed_ticks_total: self.completed_ticks_total.load(Ordering::Relaxed),
            last_tick_duration_ms: self.last_tick_duration_ms.load(Ordering::Relaxed),
            max_tick_duration_ms: self.max_tick_duration_ms.load(Ordering::Relaxed),
        }
    }

    pub fn record_compose_context(&self, response: &ComposeContextResponse) {
        let block_tokens = response
            .blocks
            .iter()
            .map(|block| crate::services::ai_pipeline::estimate_tokens_text(block.text.as_str()))
            .sum::<i64>();
        let record_tokens = response
            .recent_records
            .iter()
            .map(crate::repositories::records::estimate_pending_record_tokens)
            .sum::<i64>();
        let estimated_tokens = block_tokens.saturating_add(record_tokens).max(0) as u64;
        self.compose_requests_total.fetch_add(1, Ordering::Relaxed);
        self.compose_estimated_tokens_total
            .fetch_add(estimated_tokens, Ordering::Relaxed);
        self.compose_estimated_tokens_max
            .fetch_max(estimated_tokens, Ordering::Relaxed);
    }

    pub fn compose_baseline(&self) -> MemoryEngineComposeBaselineStats {
        MemoryEngineComposeBaselineStats {
            requests_total: self.compose_requests_total.load(Ordering::Relaxed),
            estimated_tokens_total: self.compose_estimated_tokens_total.load(Ordering::Relaxed),
            estimated_tokens_max: self.compose_estimated_tokens_max.load(Ordering::Relaxed),
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub pool: Db,
    pub config: AppConfig,
    pub user_service_http: reqwest::Client,
    pub runtime_stats: Arc<MemoryEngineRuntimeStats>,
    pub pressure: MemoryEnginePressureState,
    pub cloud_agent_store: CloudAgentStateStore,
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::MemoryEngineRuntimeStats;

    #[test]
    fn worker_runtime_stats_track_last_and_max_tick_duration() {
        let stats = MemoryEngineRuntimeStats::default();
        stats.record_worker_tick(Duration::from_millis(12));
        stats.record_worker_tick(Duration::from_millis(7));

        let snapshot = stats.snapshot();
        assert_eq!(snapshot.completed_ticks_total, 2);
        assert_eq!(snapshot.last_tick_duration_ms, 7);
        assert_eq!(snapshot.max_tick_duration_ms, 12);
    }

    #[test]
    fn compose_baseline_tracks_request_volume_and_token_estimates() {
        let stats = MemoryEngineRuntimeStats::default();
        stats.record_compose_context(&crate::models::ComposeContextResponse {
            thread_id: "thread-1".to_string(),
            blocks: vec![crate::models::ComposeContextBlock {
                block_type: "thread_summary".to_string(),
                text: "x".repeat(400),
                ..Default::default()
            }],
            recent_records: Vec::new(),
            meta: crate::models::ComposeContextMeta {
                summary_count: 1,
                recent_record_count: 0,
                ..Default::default()
            },
        });
        let snapshot = stats.compose_baseline();
        assert_eq!(snapshot.requests_total, 1);
        assert!(snapshot.estimated_tokens_total >= 100);
        assert_eq!(
            snapshot.estimated_tokens_total,
            snapshot.estimated_tokens_max
        );
    }
}
