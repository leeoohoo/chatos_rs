// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_cloud_agent_runtime::{
    create_cloud_agent_run, CloudAgentRunStore, CloudAgentStateRepository, CloudAgentStateStore,
    NewCloudAgentRun,
};
use serde_json::json;
use std::collections::HashSet;
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

mod cursor_contracts;
mod repository_round_trip;
mod summary_cursor_contracts;
mod tenant_key_contracts;
