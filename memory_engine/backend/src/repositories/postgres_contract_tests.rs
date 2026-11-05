// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_cloud_agent_runtime::{
    create_cloud_agent_run, CloudAgentRunStore, CloudAgentStateRepository, CloudAgentStateStore,
    NewCloudAgentRun,
};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use uuid::Uuid;

use crate::models::{
    EngineRecord, EngineSummary, UpsertSubjectMemoryRequest, UpsertThreadRequest,
    UpsertThreadSummaryRequest,
};
use crate::repositories::postgres::{json, timestamp};

async fn pool() -> Option<crate::db::Db> {
    let url = std::env::var("MEMORY_ENGINE_DATABASE_URL").ok()?;
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .connect(&url)
        .await
        .ok()
}

fn test_config(database_url: String) -> crate::config::AppConfig {
    crate::config::AppConfig {
        host: "127.0.0.1".to_string(),
        port: 0,
        database_url,
        ai_request_timeout_secs: 5,
        api_enabled: true,
        worker_enabled: false,
        worker_interval_secs: 30,
        worker_max_threads_per_tick: 1,
        worker_summary_concurrency: 1,
        worker_rollup_concurrency: 1,
        worker_subject_memory_concurrency: 1,
        worker_reconcile_concurrency: 1,
        cloud_agent_outbox_reconcile_interval: Duration::from_secs(1),
        cloud_agent_outbox_batch_size: 10,
        summary_retry_delay: Duration::from_millis(100),
        summary_outbox_reconcile_interval: Duration::from_secs(1),
        summary_outbox_batch_size: 10,
        rollup_retry_delay: Duration::from_millis(100),
        rollup_outbox_reconcile_interval: Duration::from_secs(1),
        rollup_outbox_batch_size: 10,
        subject_memory_retry_delay: Duration::from_millis(100),
        subject_memory_outbox_reconcile_interval: Duration::from_secs(1),
        subject_memory_outbox_batch_size: 10,
        subject_memory_lock_timeout_secs: 300,
        record_sync_lease_timeout_secs: 300,
        rollup_lock_timeout_secs: 300,
        internal_api_secrets: HashMap::new(),
        require_signed_internal_requests: true,
        user_service_base_url: "http://127.0.0.1:39190".to_string(),
        user_service_internal_base_url: "https://127.0.0.1:39192".to_string(),
        user_service_internal_http: reqwest::Client::new(),
        user_service_request_timeout_ms: 300,
    }
}

mod agent_directory_contracts;
mod context_scope_contracts;
mod cursor_contracts;
mod forced_summary_contracts;
mod memory_pipeline_contracts;
mod repository_round_trip;
mod summary_cursor_contracts;
mod tenant_key_contracts;
