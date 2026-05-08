use mongodb::bson::doc;
use uuid::Uuid;

use crate::db::Db;
use crate::models::{now_rfc3339, EngineSummary, UpsertThreadSummaryRequest};

pub async fn list_latest_thread_summaries(
    db: &Db,
    thread_id: &str,
    limit: i64,
) -> Result<Vec<EngineSummary>, String> {
    let mut rows = db
        .collection::<EngineSummary>("engine_summaries")
        .find(doc! {
            "thread_id": thread_id,
            "summary_type": "thread_incremental",
            "status": "done"
        })
        .sort(doc! {"level": -1, "created_at": -1})
        .limit(limit)
        .await
        .map_err(|err| err.to_string())?;

    let mut out = Vec::new();
    while rows.advance().await.map_err(|err| err.to_string())? {
        out.push(
            rows.deserialize_current()
                .map_err(|err| err.to_string())?,
        );
    }
    Ok(out)
}

pub async fn list_latest_thread_summaries_by_type(
    db: &Db,
    thread_id: &str,
    summary_type: &str,
    limit: i64,
) -> Result<Vec<EngineSummary>, String> {
    let mut rows = db
        .collection::<EngineSummary>("engine_summaries")
        .find(doc! {
            "thread_id": thread_id,
            "summary_type": summary_type,
            "status": "done"
        })
        .sort(doc! {"level": -1, "created_at": -1})
        .limit(limit)
        .await
        .map_err(|err| err.to_string())?;

    let mut out = Vec::new();
    while rows.advance().await.map_err(|err| err.to_string())? {
        out.push(
            rows.deserialize_current()
                .map_err(|err| err.to_string())?,
        );
    }
    Ok(out)
}

pub async fn list_thread_summaries(
    db: &Db,
    thread_id: &str,
    summary_type: Option<&str>,
    status: Option<&str>,
    level: Option<i64>,
    limit: i64,
    offset: i64,
) -> Result<Vec<EngineSummary>, String> {
    let mut filter = doc! {
        "thread_id": thread_id,
    };
    if let Some(value) = summary_type.map(str::trim).filter(|value| !value.is_empty()) {
        filter.insert("summary_type", value);
    }
    if let Some(value) = status.map(str::trim).filter(|value| !value.is_empty()) {
        filter.insert("status", value);
    }
    if let Some(value) = level {
        filter.insert("level", value);
    }

    let mut rows = db
        .collection::<EngineSummary>("engine_summaries")
        .find(filter)
        .sort(doc! {"level": -1, "created_at": 1})
        .skip(offset.max(0) as u64)
        .limit(limit.max(1).min(500))
        .await
        .map_err(|err| err.to_string())?;

    let mut out = Vec::new();
    while rows.advance().await.map_err(|err| err.to_string())? {
        out.push(
            rows.deserialize_current()
                .map_err(|err| err.to_string())?,
        );
    }
    Ok(out)
}

pub async fn list_summaries_by_thread_label(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_label: &str,
    summary_type: Option<&str>,
    status: Option<&str>,
    level: Option<i64>,
    subject_memory_summarized: Option<i64>,
    limit: i64,
    offset: i64,
) -> Result<Vec<EngineSummary>, String> {
    let normalized_label = thread_label.trim();
    if normalized_label.is_empty() {
        return Ok(Vec::new());
    }

    let mut thread_rows = db
        .collection::<crate::models::EngineThread>("engine_threads")
        .find(doc! {
            "tenant_id": tenant_id,
            "source_id": source_id,
            "labels": normalized_label,
        })
        .projection(doc! {"_id": 0, "id": 1})
        .sort(doc! {"updated_at": -1, "created_at": -1})
        .limit(5_000)
        .await
        .map_err(|err| err.to_string())?;

    let mut thread_ids = Vec::new();
    while thread_rows.advance().await.map_err(|err| err.to_string())? {
        let thread = thread_rows
            .deserialize_current()
            .map_err(|err| err.to_string())?;
        thread_ids.push(thread.id);
    }

    if thread_ids.is_empty() {
        return Ok(Vec::new());
    }

    let mut filter = doc! {
        "tenant_id": tenant_id,
        "source_id": source_id,
        "thread_id": {"$in": thread_ids},
    };
    if let Some(value) = summary_type.map(str::trim).filter(|value| !value.is_empty()) {
        filter.insert("summary_type", value);
    }
    if let Some(value) = status.map(str::trim).filter(|value| !value.is_empty()) {
        filter.insert("status", value);
    }
    if let Some(value) = level {
        filter.insert("level", value);
    }
    if let Some(value) = subject_memory_summarized {
        filter.insert("subject_memory_summarized", value.max(0));
    }

    let mut rows = db
        .collection::<EngineSummary>("engine_summaries")
        .find(filter)
        .sort(doc! {"created_at": 1, "level": -1})
        .skip(offset.max(0) as u64)
        .limit(limit.max(1).min(5_000))
        .await
        .map_err(|err| err.to_string())?;

    let mut out = Vec::new();
    while rows.advance().await.map_err(|err| err.to_string())? {
        out.push(
            rows.deserialize_current()
                .map_err(|err| err.to_string())?,
        );
    }
    Ok(out)
}

pub async fn find_summary_by_source_digest(
    db: &Db,
    thread_id: &str,
    level: i64,
    source_digest: &str,
) -> Result<Option<EngineSummary>, String> {
    let normalized = source_digest.trim();
    if normalized.is_empty() {
        return Ok(None);
    }

    db.collection::<EngineSummary>("engine_summaries")
        .find_one(doc! {
            "thread_id": thread_id,
            "level": level,
            "source_digest": normalized,
        })
        .await
        .map_err(|err| err.to_string())
}

pub async fn list_pending_summaries_by_level(
    db: &Db,
    thread_id: &str,
    level: i64,
) -> Result<Vec<EngineSummary>, String> {
    let mut rows = db
        .collection::<EngineSummary>("engine_summaries")
        .find(doc! {
            "thread_id": thread_id,
            "summary_type": "thread_incremental",
            "level": level,
            "status": "done",
            "rollup_status": "pending",
        })
        .sort(doc! {"created_at": 1})
        .await
        .map_err(|err| err.to_string())?;

    let mut out = Vec::new();
    while rows.advance().await.map_err(|err| err.to_string())? {
        out.push(
            rows.deserialize_current()
                .map_err(|err| err.to_string())?,
        );
    }
    Ok(out)
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
    while rows.advance().await.map_err(|err| err.to_string())? {
        let row = rows.deserialize_current().map_err(|err| err.to_string())?;
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

pub async fn delete_thread_summary(
    db: &Db,
    thread_id: &str,
    summary_id: &str,
) -> Result<usize, String> {
    let reset_count =
        crate::repositories::records::reset_records_summary_by_summary_id(db, thread_id, summary_id)
            .await?;
    let result = db
        .collection::<EngineSummary>("engine_summaries")
        .delete_one(doc! {"thread_id": thread_id, "id": summary_id})
        .await
        .map_err(|err| err.to_string())?;

    if result.deleted_count > 0 || reset_count > 0 {
        Ok(reset_count)
    } else {
        Ok(0)
    }
}

pub async fn create_thread_summary(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    subject_id: &str,
    summary_text: &str,
    source_record_start_id: Option<String>,
    source_record_end_id: Option<String>,
    source_record_count: usize,
) -> Result<EngineSummary, String> {
    create_thread_summary_with_type(
        db,
        tenant_id,
        source_id,
        thread_id,
        subject_id,
        "thread_incremental",
        None,
        summary_text,
        source_record_start_id,
        source_record_end_id,
        source_record_count,
        Some(serde_json::json!({
            "generator": "memory_engine_rule_based_v1"
        })),
    )
    .await
}

pub async fn upsert_thread_summary(
    db: &Db,
    thread_id: &str,
    summary_id: &str,
    req: UpsertThreadSummaryRequest,
) -> Result<EngineSummary, String> {
    let now = now_rfc3339();
    let created_at = req.created_at.clone().unwrap_or_else(|| now.clone());
    let updated_at = req.updated_at.clone().unwrap_or_else(|| now.clone());
    let status = req.status.clone().unwrap_or_else(|| "done".to_string());
    let rollup_status = req
        .rollup_status
        .clone()
        .unwrap_or_else(|| "pending".to_string());

    let filter = doc! {
        "thread_id": thread_id,
        "id": summary_id,
    };

    db.collection::<EngineSummary>("engine_summaries")
        .update_one(
            filter.clone(),
            doc! {
                "$set": {
                    "tenant_id": &req.tenant_id,
                    "source_id": &req.source_id,
                    "thread_id": thread_id,
                    "subject_id": &req.subject_id,
                    "summary_type": &req.summary_type,
                    "level": req.level.unwrap_or(0).max(0),
                    "source_digest": mongodb::bson::to_bson(&req.source_digest).unwrap_or(mongodb::bson::Bson::Null),
                    "summary_text": &req.summary_text,
                    "source_record_start_id": mongodb::bson::to_bson(&req.source_record_start_id).unwrap_or(mongodb::bson::Bson::Null),
                    "source_record_end_id": mongodb::bson::to_bson(&req.source_record_end_id).unwrap_or(mongodb::bson::Bson::Null),
                    "source_record_count": req.source_record_count.unwrap_or(0).max(0),
                    "status": &status,
                    "rollup_status": &rollup_status,
                    "rollup_summary_id": mongodb::bson::to_bson(&req.rollup_summary_id).unwrap_or(mongodb::bson::Bson::Null),
                    "rolled_up_at": mongodb::bson::to_bson(&req.rolled_up_at).unwrap_or(mongodb::bson::Bson::Null),
                    "subject_memory_summarized": req.subject_memory_summarized.unwrap_or(0).max(0),
                    "subject_memory_summarized_at": mongodb::bson::to_bson(&req.subject_memory_summarized_at).unwrap_or(mongodb::bson::Bson::Null),
                    "metadata": mongodb::bson::to_bson(&req.metadata).unwrap_or(mongodb::bson::Bson::Null),
                    "updated_at": &updated_at,
                },
                "$setOnInsert": {
                    "id": summary_id,
                    "created_at": &created_at,
                }
            },
        )
        .upsert(true)
        .await
        .map_err(|err| err.to_string())?;

    db.collection::<EngineSummary>("engine_summaries")
        .find_one(filter)
        .await
        .map_err(|err| err.to_string())?
        .ok_or_else(|| "upserted summary not found".to_string())
}

pub async fn create_thread_summary_with_type(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    subject_id: &str,
    summary_type: &str,
    source_digest: Option<String>,
    summary_text: &str,
    source_record_start_id: Option<String>,
    source_record_end_id: Option<String>,
    source_record_count: usize,
    metadata: Option<serde_json::Value>,
) -> Result<EngineSummary, String> {
    let now = now_rfc3339();
    let summary = EngineSummary {
        id: format!("sum_{}", Uuid::new_v4()),
        tenant_id: tenant_id.to_string(),
        source_id: source_id.to_string(),
        thread_id: thread_id.to_string(),
        subject_id: subject_id.to_string(),
        summary_type: summary_type.to_string(),
        level: 0,
        source_digest,
        summary_text: summary_text.to_string(),
        source_record_start_id,
        source_record_end_id,
        source_record_count: source_record_count as i64,
        status: "done".to_string(),
        rollup_status: "pending".to_string(),
        rollup_summary_id: None,
        rolled_up_at: None,
        subject_memory_summarized: 0,
        subject_memory_summarized_at: None,
        metadata,
        created_at: now.clone(),
        updated_at: now,
    };

    db.collection::<EngineSummary>("engine_summaries")
        .insert_one(summary.clone())
        .await
        .map_err(|err| err.to_string())?;

    Ok(summary)
}

pub async fn create_rollup_summary(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    subject_id: &str,
    level: i64,
    source_digest: Option<String>,
    summary_text: &str,
    source_record_start_id: Option<String>,
    source_record_end_id: Option<String>,
    source_record_count: usize,
    metadata: Option<serde_json::Value>,
) -> Result<EngineSummary, String> {
    let now = now_rfc3339();
    let summary = EngineSummary {
        id: format!("sum_{}", Uuid::new_v4()),
        tenant_id: tenant_id.to_string(),
        source_id: source_id.to_string(),
        thread_id: thread_id.to_string(),
        subject_id: subject_id.to_string(),
        summary_type: "thread_incremental".to_string(),
        level: level.max(0),
        source_digest,
        summary_text: summary_text.to_string(),
        source_record_start_id,
        source_record_end_id,
        source_record_count: source_record_count as i64,
        status: "done".to_string(),
        rollup_status: "pending".to_string(),
        rollup_summary_id: None,
        rolled_up_at: None,
        subject_memory_summarized: 0,
        subject_memory_summarized_at: None,
        metadata,
        created_at: now.clone(),
        updated_at: now,
    };

    db.collection::<EngineSummary>("engine_summaries")
        .insert_one(summary.clone())
        .await
        .map_err(|err| err.to_string())?;

    Ok(summary)
}

pub async fn mark_summaries_rolled_up(
    db: &Db,
    thread_id: &str,
    summary_ids: &[String],
    rollup_summary_id: &str,
) -> Result<usize, String> {
    if summary_ids.is_empty() {
        return Ok(0);
    }

    let now = now_rfc3339();
    let result = db
        .collection::<EngineSummary>("engine_summaries")
        .update_many(
            doc! {
                "thread_id": thread_id,
                "id": {"$in": summary_ids.to_vec()},
                "rollup_status": "pending",
            },
            doc! {
                "$set": {
                    "rollup_status": "done",
                    "rollup_summary_id": rollup_summary_id,
                    "rolled_up_at": &now,
                    "updated_at": &now,
                }
            },
        )
        .await
        .map_err(|err| err.to_string())?;

    Ok(result.modified_count as usize)
}

pub async fn mark_summaries_subject_memory_summarized(
    db: &Db,
    thread_id: &str,
    summary_ids: &[String],
) -> Result<usize, String> {
    if summary_ids.is_empty() {
        return Ok(0);
    }

    let now = now_rfc3339();
    let result = db
        .collection::<EngineSummary>("engine_summaries")
        .update_many(
            doc! {
                "thread_id": thread_id,
                "id": {"$in": summary_ids.to_vec()},
                "subject_memory_summarized": {"$ne": 1},
            },
            doc! {
                "$set": {
                    "subject_memory_summarized": 1,
                    "subject_memory_summarized_at": &now,
                    "updated_at": &now,
                }
            },
        )
        .await
        .map_err(|err| err.to_string())?;

    Ok(result.modified_count as usize)
}
