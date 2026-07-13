// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use mongodb::bson::{doc, Bson, Document};

use crate::core::mongo_cursor::collect_and_map;
use crate::models::terminal_log::TerminalLog;
use crate::repositories::db::{
    doc_from_pairs, mongo_delete_many_doc, mongo_insert_doc, to_doc, with_db,
};

fn normalize_doc(doc: &Document) -> Option<TerminalLog> {
    Some(TerminalLog {
        id: doc.get_str("id").ok()?.to_string(),
        terminal_id: doc.get_str("terminal_id").ok()?.to_string(),
        log_type: doc.get_str("type").ok()?.to_string(),
        content: doc.get_str("content").ok()?.to_string(),
        created_at: doc.get_str("created_at").unwrap_or("").to_string(),
    })
}

pub async fn create_terminal_log(log: &TerminalLog) -> Result<String, String> {
    let log_mongo = log.clone();
    with_db(|db| {
        let doc = to_doc(doc_from_pairs(vec![
            ("id", Bson::String(log_mongo.id.clone())),
            ("terminal_id", Bson::String(log_mongo.terminal_id.clone())),
            ("type", Bson::String(log_mongo.log_type.clone())),
            ("content", Bson::String(log_mongo.content.clone())),
            ("created_at", Bson::String(log_mongo.created_at.clone())),
        ]));
        Box::pin(async move {
            mongo_insert_doc(db, "terminal_logs", doc).await?;
            Ok(log_mongo.id.clone())
        })
    })
    .await
}

pub async fn list_terminal_logs(
    terminal_id: &str,
    limit: Option<i64>,
    offset: i64,
) -> Result<Vec<TerminalLog>, String> {
    with_db(|db| {
        let terminal_id = terminal_id.to_string();
        Box::pin(async move {
            let mut options = mongodb::options::FindOptions::builder()
                .sort(doc! { "created_at": 1 })
                .build();
            if let Some(l) = limit {
                options.limit = Some(l);
            }
            if offset > 0 {
                options.skip = Some(offset as u64);
            }
            let cursor = db
                .collection::<Document>("terminal_logs")
                .find(doc! { "terminal_id": terminal_id }, options)
                .await
                .map_err(|e| e.to_string())?;
            collect_and_map(cursor, normalize_doc).await
        })
    })
    .await
}

pub async fn list_terminal_logs_recent(
    terminal_id: &str,
    limit: i64,
) -> Result<Vec<TerminalLog>, String> {
    let capped_limit = limit.max(1);
    with_db(|db| {
        let terminal_id = terminal_id.to_string();
        Box::pin(async move {
            let options = mongodb::options::FindOptions::builder()
                .sort(doc! { "created_at": -1 })
                .limit(Some(capped_limit))
                .build();
            let cursor = db
                .collection::<Document>("terminal_logs")
                .find(doc! { "terminal_id": terminal_id }, options)
                .await
                .map_err(|e| e.to_string())?;
            let mut out: Vec<TerminalLog> = collect_and_map(cursor, normalize_doc).await?;
            out.reverse();
            Ok(out)
        })
    })
    .await
}

pub async fn list_terminal_logs_before(
    terminal_id: &str,
    before_created_at: &str,
    limit: i64,
) -> Result<Vec<TerminalLog>, String> {
    let capped_limit = limit.max(1);
    with_db(|db| {
        let terminal_id = terminal_id.to_string();
        let before_created_at = before_created_at.to_string();
        Box::pin(async move {
            let options = mongodb::options::FindOptions::builder()
                .sort(doc! { "created_at": -1 })
                .limit(Some(capped_limit))
                .build();
            let cursor = db
                .collection::<Document>("terminal_logs")
                .find(
                    doc! {
                        "terminal_id": terminal_id,
                        "created_at": { "$lt": before_created_at },
                    },
                    options,
                )
                .await
                .map_err(|e| e.to_string())?;
            let mut out: Vec<TerminalLog> = collect_and_map(cursor, normalize_doc).await?;
            out.reverse();
            Ok(out)
        })
    })
    .await
}

pub async fn delete_terminal_logs(terminal_id: &str) -> Result<(), String> {
    with_db(|db| {
        let terminal_id = terminal_id.to_string();
        Box::pin(async move {
            mongo_delete_many_doc(db, "terminal_logs", doc! { "terminal_id": &terminal_id })
                .await?;
            Ok(())
        })
    })
    .await
}
