// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_cloud_agent_runtime::{
    create_cloud_agent_run, CloudAgentRunStore, CloudAgentStateRepository, CloudAgentStateStore,
    NewCloudAgentRun,
};
use serde_json::json;
use std::collections::HashSet;
use uuid::Uuid;

use crate::config::test_app_config as test_config;
use crate::models::{EngineRecord, EngineSummary, UpsertSubjectMemoryRequest, UpsertThreadRequest};
use crate::repositories::postgres::{json, timestamp};

async fn pool() -> Option<crate::db::Db> {
    let url = std::env::var("MEMORY_ENGINE_DATABASE_URL").ok()?;
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .connect(&url)
        .await
        .ok()
}

mod agent_directory_contracts;
mod compact_turn_contracts;
mod context_scope_contracts;
mod cursor_contracts;
mod derived_event_contracts;
mod forced_summary_contracts;
mod generation_commit_contracts;
mod history_migration_contracts;
mod lifecycle_contracts;
mod memory_fact_contracts;
mod memory_pipeline_contracts;
mod operator_audit_contracts;
mod partition_claim_contracts;
mod partition_ingest_contracts;
mod reliability_audit_contracts;
mod repository_round_trip;
mod subject_memory_generation_contracts;
mod summary_cursor_contracts;
mod summary_generation_contracts;
mod tenant_key_contracts;
