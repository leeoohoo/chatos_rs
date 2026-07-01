// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use futures_util::StreamExt;
use mongodb::bson::doc;

use crate::db::Db;
use crate::models::EngineSummary;

use super::super::common::{collect_summaries, summary_collection};

pub async fn find_summary_by_source_digest(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    level: i64,
    source_digest: &str,
) -> Result<Option<EngineSummary>, String> {
    let normalized = source_digest.trim();
    if normalized.is_empty() {
        return Ok(None);
    }

    summary_collection(db)
        .find_one(doc! {
            "tenant_id": tenant_id,
            "source_id": source_id,
            "thread_id": thread_id,
            "level": level,
            "source_digest": normalized,
        })
        .await
        .map_err(|err| err.to_string())
}

pub async fn list_pending_summaries_by_level(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    level: i64,
) -> Result<Vec<EngineSummary>, String> {
    let cursor = summary_collection(db)
        .find(doc! {
            "tenant_id": tenant_id,
            "source_id": source_id,
            "thread_id": thread_id,
            "summary_type": "thread_incremental",
            "level": level,
            "status": "done",
            "rollup_status": "pending",
        })
        .sort(doc! {"created_at": 1})
        .await
        .map_err(|err| err.to_string())?;

    collect_summaries(cursor).await
}

pub async fn list_threads_with_pending_rollups(
    db: &Db,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    max_level: i64,
    limit: i64,
) -> Result<Vec<(String, String, String)>, String> {
    let mut match_doc = doc! {
        "summary_type": "thread_incremental",
        "status": "done",
        "rollup_status": "pending",
        "level": {"$lte": max_level.max(0)}
    };
    if let Some(value) = tenant_id.map(str::trim).filter(|value| !value.is_empty()) {
        match_doc.insert("tenant_id", value);
    }
    if let Some(value) = source_id.map(str::trim).filter(|value| !value.is_empty()) {
        match_doc.insert("source_id", value);
    }

    let pipeline = vec![
        doc! {"$match": match_doc},
        doc! {"$group": {
            "_id": {
                "thread_id": "$thread_id",
                "tenant_id": "$tenant_id",
                "source_id": "$source_id",
            },
            "min_created_at": {"$min": "$created_at"}
        }},
        doc! {"$sort": {"min_created_at": 1}},
        doc! {"$limit": limit.max(1).min(500)},
    ];

    let mut rows = db
        .collection::<mongodb::bson::Document>("engine_summaries")
        .aggregate(pipeline)
        .await
        .map_err(|err| err.to_string())?;

    let mut out = Vec::new();
    while let Some(row) = rows.next().await {
        let row = row.map_err(|err| err.to_string())?;
        let Ok(id_doc) = row.get_document("_id") else {
            continue;
        };
        let Some(thread_id) = id_doc.get_str("thread_id").ok().map(ToOwned::to_owned) else {
            continue;
        };
        let Some(tenant_id) = id_doc.get_str("tenant_id").ok().map(ToOwned::to_owned) else {
            continue;
        };
        let Some(source_id) = id_doc.get_str("source_id").ok().map(ToOwned::to_owned) else {
            continue;
        };
        out.push((tenant_id, source_id, thread_id));
    }
    Ok(out)
}
