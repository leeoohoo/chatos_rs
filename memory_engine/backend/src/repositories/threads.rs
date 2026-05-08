use mongodb::bson::{doc, Bson};
use futures_util::TryStreamExt;

use crate::db::Db;
use crate::models::{now_rfc3339, EngineThread, UpsertThreadRequest};

pub async fn upsert_thread(
    db: &Db,
    thread_id: &str,
    req: UpsertThreadRequest,
) -> Result<EngineThread, String> {
    let now = now_rfc3339();
    let created_at = req.created_at.clone().unwrap_or_else(|| now.clone());
    let updated_at = req.updated_at.clone().unwrap_or_else(|| now.clone());
    let id = thread_id.to_string();
    let status = req.status.unwrap_or_else(|| "active".to_string());
    let archived_at = if let Some(value) = req.archived_at.clone() {
        Some(value)
    } else if status == "archived" {
        Some(updated_at.clone())
    } else {
        None
    };

    db.collection::<EngineThread>("engine_threads")
        .update_one(
            doc! {
                "tenant_id": &req.tenant_id,
                "source_id": &req.source_id,
                "id": thread_id
            },
            doc! {
                "$set": {
                    "tenant_id": &req.tenant_id,
                    "source_id": &req.source_id,
                    "subject_id": &req.subject_id,
                    "thread_type": &req.thread_type,
                    "external_thread_id": mongodb::bson::to_bson(&req.external_thread_id).unwrap_or(Bson::Null),
                    "title": mongodb::bson::to_bson(&req.title).unwrap_or(Bson::Null),
                    "labels": mongodb::bson::to_bson(&req.labels).unwrap_or(Bson::Null),
                    "metadata": mongodb::bson::to_bson(&req.metadata).unwrap_or(Bson::Null),
                    "status": &status,
                    "archived_at": mongodb::bson::to_bson(&archived_at).unwrap_or(Bson::Null),
                    "updated_at": &updated_at,
                },
                "$setOnInsert": {
                    "id": id,
                    "created_at": &created_at,
                }
            },
        )
        .upsert(true)
        .await
        .map_err(|err| err.to_string())?;

    get_thread_by_id(db, req.tenant_id.as_str(), req.source_id.as_str(), thread_id)
        .await?
        .ok_or_else(|| "upserted thread not found".to_string())
}

pub async fn get_thread_by_id(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
) -> Result<Option<EngineThread>, String> {
    db.collection::<EngineThread>("engine_threads")
        .find_one(doc! {
            "tenant_id": tenant_id,
            "source_id": source_id,
            "id": thread_id
        })
        .await
        .map_err(|err| err.to_string())
}

pub async fn list_threads_with_pending_records(
    db: &Db,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    limit: i64,
) -> Result<Vec<EngineThread>, String> {
    let mut match_doc = doc! {
        "$or": [
            {"summary_status": "pending"},
            {"summary_status": {"$exists": false}},
            {"summary_status": Bson::Null},
            {"summary_status": ""}
        ]
    };
    if let Some(value) = tenant_id.map(str::trim).filter(|value| !value.is_empty()) {
        match_doc.insert("tenant_id", value);
    }
    if let Some(value) = source_id.map(str::trim).filter(|value| !value.is_empty()) {
        match_doc.insert("source_id", value);
    }

    let pipeline = vec![
        doc! {
            "$match": match_doc
        },
        doc! {
            "$group": {
                "_id": "$thread_id",
                "tenant_id": {"$first": "$tenant_id"},
                "source_id": {"$first": "$source_id"},
                "latest_created_at": {"$max": "$created_at"}
            }
        },
        doc! {"$sort": {"latest_created_at": 1}},
        doc! {"$limit": limit.max(1).min(500)},
    ];

    let mut rows = db
        .collection::<mongodb::bson::Document>("engine_records")
        .aggregate(pipeline)
        .await
        .map_err(|err| err.to_string())?;

    let mut out = Vec::new();
    while let Some(row) = rows.try_next().await.map_err(|err| err.to_string())? {
        let Some(thread_id) = row.get_str("_id").ok() else {
            continue;
        };
        let Some(tenant_id) = row.get_str("tenant_id").ok() else {
            continue;
        };
        let Some(source_id) = row.get_str("source_id").ok() else {
            continue;
        };
        if let Some(thread) = get_thread_by_id(db, tenant_id, source_id, thread_id).await? {
            out.push(thread);
        }
    }
    Ok(out)
}

pub async fn list_threads_by_label(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_label: &str,
    status: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<EngineThread>, String> {
    let normalized_label = thread_label.trim();
    if normalized_label.is_empty() {
        return Ok(Vec::new());
    }

    let mut filter = doc! {
        "tenant_id": tenant_id,
        "source_id": source_id,
        "labels": normalized_label,
    };
    if let Some(value) = status.map(str::trim).filter(|value| !value.is_empty()) {
        filter.insert("status", value);
    }

    let mut rows = db
        .collection::<EngineThread>("engine_threads")
        .find(filter)
        .sort(doc! {"updated_at": -1, "created_at": -1})
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
