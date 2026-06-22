#![allow(dead_code, unused_imports)]

#[path = "../config.rs"]
mod config;
#[path = "../db/mod.rs"]
mod db;
#[path = "../models/mod.rs"]
mod models;
#[path = "../repositories/mod.rs"]
mod repositories;

use std::collections::HashSet;

use futures_util::TryStreamExt;
use mongodb::bson::{doc, Document};
use repositories::records::compact_turns;

use crate::models::EngineRecord;

fn optional_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn build_filter(
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    thread_id: Option<&str>,
    record_type: &str,
) -> Document {
    let mut filter = doc! {
        "role": "user",
        "record_type": record_type,
        "metadata.conversation_turn_id": {
            "$exists": true,
            "$type": "string",
        },
    };
    if let Some(value) = tenant_id {
        filter.insert("tenant_id", value);
    }
    if let Some(value) = source_id {
        filter.insert("source_id", value);
    }
    if let Some(value) = thread_id {
        filter.insert("thread_id", value);
    }
    filter
}

#[tokio::main]
async fn main() -> Result<(), String> {
    dotenvy::dotenv().ok();
    let config = config::AppConfig::from_env();
    let pool = db::init_pool(&config).await?;
    db::init_schema(&pool).await?;

    let tenant_id = optional_env("MEMORY_ENGINE_BACKFILL_TENANT_ID");
    let source_id = optional_env("MEMORY_ENGINE_BACKFILL_SOURCE_ID");
    let thread_id = optional_env("MEMORY_ENGINE_BACKFILL_THREAD_ID");
    let record_type =
        optional_env("MEMORY_ENGINE_BACKFILL_RECORD_TYPE").unwrap_or_else(|| "message".to_string());

    let filter = build_filter(
        tenant_id.as_deref(),
        source_id.as_deref(),
        thread_id.as_deref(),
        record_type.as_str(),
    );
    let mut cursor = pool
        .collection::<EngineRecord>("engine_records")
        .find(filter)
        .sort(doc! {
            "tenant_id": 1,
            "source_id": 1,
            "thread_id": 1,
            "created_at": 1,
            "id": 1,
        })
        .await
        .map_err(|err| err.to_string())?;

    let mut seen = HashSet::new();
    let mut scanned_users = 0usize;
    let mut rebuilt_turns = 0usize;

    while let Some(record) = cursor.try_next().await.map_err(|err| err.to_string())? {
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
