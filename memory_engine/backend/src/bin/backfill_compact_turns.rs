#![allow(dead_code, unused_imports)]
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[path = "../config.rs"]
mod config;
#[path = "../db/mod.rs"]
mod db;
#[path = "../models/mod.rs"]
mod models;
#[path = "../repositories/mod.rs"]
mod repositories;

use std::collections::HashSet;

use repositories::records::compact_turns;
use sqlx::types::Json;
use sqlx::{Postgres, QueryBuilder};

use crate::models::EngineRecord;

fn optional_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[tokio::main]
async fn main() -> Result<(), String> {
    chatos_service_runtime::load_service_dotenv(std::path::Path::new(env!("CARGO_MANIFEST_DIR")));
    let config = config::AppConfig::from_env()?;
    let pool = db::init_pool(&config).await?;
    db::init_schema(&pool).await?;

    let tenant_id = optional_env("MEMORY_ENGINE_BACKFILL_TENANT_ID");
    let source_id = optional_env("MEMORY_ENGINE_BACKFILL_SOURCE_ID");
    let thread_id = optional_env("MEMORY_ENGINE_BACKFILL_THREAD_ID");
    let record_type =
        optional_env("MEMORY_ENGINE_BACKFILL_RECORD_TYPE").unwrap_or_else(|| "message".to_string());

    let mut query = QueryBuilder::<Postgres>::new(
        "SELECT data FROM engine_records WHERE role='user' AND record_type=",
    );
    query
        .push_bind(&record_type)
        .push(" AND NULLIF(data #>> '{metadata,conversation_turn_id}','') IS NOT NULL");
    for (column, value) in [
        ("tenant_id", tenant_id.as_deref()),
        ("source_id", source_id.as_deref()),
        ("thread_id", thread_id.as_deref()),
    ] {
        if let Some(value) = value {
            query.push(" AND ").push(column).push("=").push_bind(value);
        }
    }
    query.push(" ORDER BY tenant_id,source_id,thread_id,created_at,id");
    let rows = query
        .build_query_scalar::<Json<serde_json::Value>>()
        .fetch_all(&pool)
        .await
        .map_err(|error| error.to_string())?;

    let mut seen = HashSet::new();
    let mut scanned_users = 0usize;
    let mut rebuilt_turns = 0usize;

    for row in rows {
        let record: EngineRecord = repositories::postgres::decode(row)?;
        scanned_users += 1;
        let Some(turn_id) = compact_turns::extract_turn_id(&record).map(ToOwned::to_owned) else {
            continue;
        };
        let key = format!(
            "{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
            record.tenant_id, record.source_id, record.thread_id, record.record_type, turn_id
        );
        if !seen.insert(key) {
            continue;
        }

        compact_turns::rebuild_compact_turn(
            &pool,
            record.thread_id.as_str(),
            record.tenant_id.as_str(),
            record.source_id.as_str(),
            record.record_type.as_str(),
            turn_id.as_str(),
        )
        .await?;
        rebuilt_turns += 1;
    }

    println!(
        "backfill_compact_turns complete scanned_users={} rebuilt_turns={} record_type={} tenant_id={} source_id={} thread_id={}",
        scanned_users,
        rebuilt_turns,
        record_type,
        tenant_id.as_deref().unwrap_or("*"),
        source_id.as_deref().unwrap_or("*"),
        thread_id.as_deref().unwrap_or("*"),
    );

    Ok(())
}
