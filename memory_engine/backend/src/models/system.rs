// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::Serialize;

#[derive(Debug, Clone, Default, Serialize)]
pub struct MemoryEngineSummaryBacklogStats {
    pub pending_threads: i64,
    pub pending_records: i64,
    pub pending_tokens: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct MemoryEngineRollupBacklogStats {
    pub pending_summaries: i64,
    pub pending_threads: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct MemoryEngineReconcileBacklogStats {
    pub candidate_threads: i64,
    pub running_jobs: i64,
    pub stale_running_jobs: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct MemoryEngineBacklogStats {
    pub summary: MemoryEngineSummaryBacklogStats,
    pub rollup: MemoryEngineRollupBacklogStats,
    pub reconcile: MemoryEngineReconcileBacklogStats,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryEngineRoleStats {
    pub api_enabled: bool,
    pub worker_enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryEngineWorkerConfigStats {
    pub interval_secs: u64,
    pub max_threads_per_tick: i64,
    pub summary_concurrency: usize,
    pub rollup_concurrency: usize,
    pub subject_memory_concurrency: usize,
    pub reconcile_concurrency: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct MemoryEngineWorkerRuntimeStats {
    pub completed_ticks_total: u64,
    pub last_tick_duration_ms: u64,
    pub max_tick_duration_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryEngineSystemStatsResponse {
    pub ok: bool,
    pub service: &'static str,
    pub now: String,
    pub roles: MemoryEngineRoleStats,
    pub worker_config: MemoryEngineWorkerConfigStats,
    pub worker_runtime: MemoryEngineWorkerRuntimeStats,
    pub backlog: MemoryEngineBacklogStats,
    pub job_runs_last_24h: serde_json::Value,
}
