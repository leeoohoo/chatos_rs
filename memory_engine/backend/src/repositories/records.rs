use mongodb::bson::{doc, Bson, Document};

use crate::db::Db;
use crate::models::{now_rfc3339, BatchSyncRecordsRequest, EngineRecord};

pub async fn batch_sync_records(
    db: &Db,
    thread_id: &str,
    req: &BatchSyncRecordsRequest,
) -> Result<usize, String> {
    let collection = db.collection::<EngineRecord>("engine_records");
    let mut upserted_count = 0usize;

    for record in &req.records {
        let summary_status = record
            .summary_status
            .clone()
            .unwrap_or_else(|| "pending".to_string());
        let result = collection
            .update_one(
                doc! {"thread_id": thread_id, "id": &record.id},
                doc! {
                    "$set": {
                        "thread_id": thread_id,
                        "tenant_id": &req.tenant_id,
                        "source_id": &req.source_id,
                        "external_record_id": mongodb::bson::to_bson(&record.external_record_id).unwrap_or(Bson::Null),
                        "role": &record.role,
                        "record_type": &record.record_type,
                        "content": &record.content,
                        "structured_payload": mongodb::bson::to_bson(&record.structured_payload).unwrap_or(Bson::Null),
                        "metadata": mongodb::bson::to_bson(&record.metadata).unwrap_or(Bson::Null),
                        "summary_status": &summary_status,
                        "summary_id": mongodb::bson::to_bson(&record.summary_id).unwrap_or(Bson::Null),
                        "summarized_at": mongodb::bson::to_bson(&record.summarized_at).unwrap_or(Bson::Null),
                        "created_at": &record.created_at,
                    }
                },
            )
            .upsert(true)
            .await
            .map_err(|err| err.to_string())?;

        if result.matched_count == 0 || result.upserted_id.is_some() {
            upserted_count += 1;
        }
    }

    let _ = now_rfc3339();
    Ok(upserted_count)
}

pub async fn list_recent_records(
    db: &Db,
    thread_id: &str,
    limit: i64,
) -> Result<Vec<EngineRecord>, String> {
    let mut rows = db
        .collection::<EngineRecord>("engine_records")
        .find(doc! {"thread_id": thread_id})
        .sort(doc! {"created_at": -1})
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
    out.reverse();
    Ok(out)
}

pub async fn list_records(
    db: &Db,
    thread_id: &str,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    role: Option<&str>,
    record_type: Option<&str>,
    summary_status: Option<&str>,
    limit: i64,
    offset: i64,
    asc: bool,
) -> Result<Vec<EngineRecord>, String> {
    let filter = build_record_filter(
        thread_id,
        tenant_id,
        source_id,
        role,
        record_type,
        summary_status,
    );

    let sort_order = if asc { 1 } else { -1 };
    let mut rows = db
        .collection::<EngineRecord>("engine_records")
        .find(filter)
        .sort(doc! {"created_at": sort_order})
        .skip(offset.max(0) as u64)
        .limit(limit.max(1).min(2000))
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

pub async fn count_records(
    db: &Db,
    thread_id: &str,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    role: Option<&str>,
    record_type: Option<&str>,
    summary_status: Option<&str>,
) -> Result<i64, String> {
    let filter = build_record_filter(
        thread_id,
        tenant_id,
        source_id,
        role,
        record_type,
        summary_status,
    );

    db.collection::<EngineRecord>("engine_records")
        .count_documents(filter)
        .await
        .map(|count| count as i64)
        .map_err(|err| err.to_string())
}

pub async fn get_record_by_id(
    db: &Db,
    record_id: &str,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
) -> Result<Option<EngineRecord>, String> {
    let mut filter = doc! {
        "id": record_id,
    };
    if let Some(value) = tenant_id.map(str::trim).filter(|value| !value.is_empty()) {
        filter.insert("tenant_id", value);
    }
    if let Some(value) = source_id.map(str::trim).filter(|value| !value.is_empty()) {
        filter.insert("source_id", value);
    }

    db.collection::<EngineRecord>("engine_records")
        .find_one(filter)
        .await
        .map_err(|err| err.to_string())
}

fn build_record_filter(
    thread_id: &str,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    role: Option<&str>,
    record_type: Option<&str>,
    summary_status: Option<&str>,
) -> Document {
    let mut filter = doc! {
        "thread_id": thread_id,
    };
    if let Some(value) = tenant_id.map(str::trim).filter(|value| !value.is_empty()) {
        filter.insert("tenant_id", value);
    }
    if let Some(value) = source_id.map(str::trim).filter(|value| !value.is_empty()) {
        filter.insert("source_id", value);
    }
    if let Some(value) = role.map(str::trim).filter(|value| !value.is_empty()) {
        filter.insert("role", value);
    }
    if let Some(value) = record_type.map(str::trim).filter(|value| !value.is_empty()) {
        filter.insert("record_type", value);
    }
    if let Some(value) = summary_status.map(str::trim).filter(|value| !value.is_empty()) {
        if value == "pending" {
            filter.insert(
                "$or",
                vec![
                    doc! {"summary_status": "pending"},
                    doc! {"summary_status": {"$exists": false}},
                    doc! {"summary_status": Bson::Null},
                    doc! {"summary_status": ""},
                ],
            );
        } else {
            filter.insert("summary_status", value);
        }
    }
    filter
}

pub async fn list_pending_records(
    db: &Db,
    thread_id: &str,
    limit: i64,
) -> Result<Vec<EngineRecord>, String> {
    let mut rows = db
        .collection::<EngineRecord>("engine_records")
        .find(doc! {
            "thread_id": thread_id,
            "$or": [
                {"summary_status": "pending"},
                {"summary_status": {"$exists": false}},
                {"summary_status": Bson::Null},
                {"summary_status": ""}
            ]
        })
        .sort(doc! {"created_at": 1})
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

pub async fn delete_records_by_thread(
    db: &Db,
    thread_id: &str,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    record_type: Option<&str>,
) -> Result<i64, String> {
    let mut filter = doc! {
        "thread_id": thread_id,
    };
    if let Some(value) = tenant_id.map(str::trim).filter(|value| !value.is_empty()) {
        filter.insert("tenant_id", value);
    }
    if let Some(value) = source_id.map(str::trim).filter(|value| !value.is_empty()) {
        filter.insert("source_id", value);
    }
    if let Some(value) = record_type.map(str::trim).filter(|value| !value.is_empty()) {
        filter.insert("record_type", value);
    }

    let result = db
        .collection::<EngineRecord>("engine_records")
        .delete_many(filter)
        .await
        .map_err(|err| err.to_string())?;
    Ok(result.deleted_count as i64)
}

pub async fn delete_record_by_id(
    db: &Db,
    record_id: &str,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
) -> Result<bool, String> {
    let mut filter = doc! {
        "id": record_id,
    };
    if let Some(value) = tenant_id.map(str::trim).filter(|value| !value.is_empty()) {
        filter.insert("tenant_id", value);
    }
    if let Some(value) = source_id.map(str::trim).filter(|value| !value.is_empty()) {
        filter.insert("source_id", value);
    }

    let result = db
        .collection::<EngineRecord>("engine_records")
        .delete_one(filter)
        .await
        .map_err(|err| err.to_string())?;
    Ok(result.deleted_count > 0)
}

pub async fn mark_records_summarized(
    db: &Db,
    thread_id: &str,
    record_ids: &[String],
    summary_id: &str,
) -> Result<usize, String> {
    if record_ids.is_empty() {
        return Ok(0);
    }

    let result = db
        .collection::<EngineRecord>("engine_records")
        .update_many(
            doc! {
                "thread_id": thread_id,
                "id": {"$in": record_ids.to_vec()},
            },
            doc! {
                "$set": {
                    "summary_status": "summarized",
                    "summary_id": summary_id,
                    "summarized_at": now_rfc3339(),
                }
            },
        )
        .await
        .map_err(|err| err.to_string())?;

    Ok(result.modified_count as usize)
}

pub async fn reset_records_summary_by_summary_id(
    db: &Db,
    thread_id: &str,
    summary_id: &str,
) -> Result<usize, String> {
    let normalized_summary_id = summary_id.trim();
    if normalized_summary_id.is_empty() {
        return Ok(0);
    }

    let result = db
        .collection::<EngineRecord>("engine_records")
        .update_many(
            doc! {
                "thread_id": thread_id,
                "summary_id": normalized_summary_id,
            },
            doc! {
                "$set": {
                    "summary_status": "pending",
                    "summarized_at": Bson::Null,
                },
                "$unset": {
                    "summary_id": "",
                }
            },
        )
        .await
        .map_err(|err| err.to_string())?;

    Ok(result.modified_count as usize)
}
