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

fn env_flag(name: &str) -> bool {
    matches!(
        optional_env(name)
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn build_filter(thread_id: Option<&str>) -> Document {
    let mut filter = doc! {
        "role": "user",
        "record_type": "message",
        "$and": [
            {
                "$or": [
                    { "metadata.message_mode": "project_requirement_execution" },
                    { "metadata.project_requirement_execution": { "$exists": true } },
                ],
            },
            {
                "$or": [
                    { "metadata.conversation_turn_id": { "$exists": false } },
                    { "metadata.conversation_turn_id": "" },
                    { "metadata.task_runner_async.source_turn_id": { "$exists": false } },
                    { "metadata.task_runner_async.source_turn_id": "" },
                ],
            },
        ],
    };
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

    let apply = env_flag("MEMORY_ENGINE_REPAIR_APPLY");
    let thread_id = optional_env("MEMORY_ENGINE_REPAIR_THREAD_ID");
    let filter = build_filter(thread_id.as_deref());
    let collection = pool.collection::<EngineRecord>("engine_records");
    let mut cursor = collection
        .find(filter)
        .sort(doc! {
            "created_at": 1,
            "id": 1,
        })
        .await
        .map_err(|err| err.to_string())?;

    let mut matched = 0usize;
    let mut updated = 0usize;
    let mut rebuilt_turns = 0usize;

    while let Some(record) = cursor.try_next().await.map_err(|err| err.to_string())? {
        matched += 1;
        let turn_id = record.id.clone();
        println!(
            "{} record_id={} thread_id={} tenant_id={} source_id={} created_at={} content={}",
            if apply { "repair" } else { "dry-run" },
            record.id,
            record.thread_id,
            record.tenant_id,
            record.source_id,
            record.created_at,
            record.content.replace('\n', "\\n")
        );

        if !apply {
            continue;
        }

        collection
            .update_one(
                doc! {
                    "id": &record.id,
                    "tenant_id": &record.tenant_id,
                    "source_id": &record.source_id,
                    "thread_id": &record.thread_id,
                    "record_type": &record.record_type,
                },
                doc! {
                    "$set": {
                        "metadata.conversation_turn_id": &turn_id,
                        "metadata.task_runner_async.source_turn_id": &turn_id,
                    }
                },
            )
            .await
            .map_err(|err| err.to_string())?;
        updated += 1;

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
        "repair_project_requirement_execution_turns complete apply={} matched={} updated={} rebuilt_turns={} thread_id={}",
        apply,
        matched,
        updated,
        rebuilt_turns,
        thread_id.as_deref().unwrap_or("*")
    );

    Ok(())
}
