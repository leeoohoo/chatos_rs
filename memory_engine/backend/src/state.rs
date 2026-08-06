// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::config::AppConfig;
use crate::db::Db;
use crate::models::MemoryEngineWorkerRuntimeStats;
use crate::pressure::MemoryEnginePressureState;
use chatos_queue_observability::RabbitMqQueueInspector;

#[derive(Default)]
pub struct MemoryEngineRuntimeStats {
    completed_ticks_total: AtomicU64,
    last_tick_duration_ms: AtomicU64,
    max_tick_duration_ms: AtomicU64,
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
}

#[derive(Clone)]
pub struct AppState {
    pub pool: Db,
    pub config: AppConfig,
    pub user_service_http: reqwest::Client,
    pub runtime_stats: Arc<MemoryEngineRuntimeStats>,
    pub rabbitmq_queue_inspector: RabbitMqQueueInspector,
    pub pressure: MemoryEnginePressureState,
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
}
