use futures_util::TryStreamExt;
use mongodb::bson::{doc, Bson, Document};
use mongodb::options::FindOptions;
use uuid::Uuid;

use crate::db::Db;
use crate::models::{CreateSummaryInput, SessionSummary};

use super::now_rfc3339;

fn collection(db: &Db) -> mongodb::Collection<SessionSummary> {
    db.collection::<SessionSummary>("session_summaries_v2")
}

fn doc_i64(doc: &Document, key: &str) -> i64 {
    match doc.get(key) {
        Some(Bson::Int32(v)) => *v as i64,
        Some(Bson::Int64(v)) => *v,
        Some(Bson::Double(v)) => *v as i64,
        _ => 0,
    }
}

pub async fn create_summary(db: &Db, input: CreateSummaryInput) -> Result<SessionSummary, String> {
    let now = now_rfc3339();
    let summary = SessionSummary {
        id: Uuid::new_v4().to_string(),
        session_id: input.session_id,
        summary_text: input.summary_text,
        summary_model: input.summary_model,
        trigger_type: input.trigger_type,
        source_start_message_id: input.source_start_message_id,
        source_end_message_id: input.source_end_message_id,
        source_message_count: input.source_message_count,
        source_estimated_tokens: input.source_estimated_tokens,
        status: input.status,
        error_message: input.error_message,
        level: input.level,
        rollup_summary_id: None,
        rolled_up_at: None,
        created_at: now.clone(),
        updated_at: now,
    };

    collection(db)
        .insert_one(summary.clone())
        .await
        .map_err(|e| e.to_string())?;

    Ok(summary)
}

pub async fn list_summaries(
    db: &Db,
    session_id: &str,
    level: Option<i64>,
    status: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<SessionSummary>, String> {
    let mut filter = doc! {"session_id": session_id};
    if let Some(v) = level {
        filter.insert("level", v);
    }
    if let Some(v) = status {
        filter.insert("status", v);
    }

    let options = FindOptions::builder()
        .sort(doc! {"level": -1, "created_at": 1})
        .limit(Some(limit.max(1).min(500)))
        .skip(Some(offset.max(0) as u64))
        .build();

    let cursor = collection(db)
        .find(filter)
        .with_options(options)
        .await
        .map_err(|e| e.to_string())?;

    cursor.try_collect().await.map_err(|e| e.to_string())
}

pub async fn list_all_summaries_by_session(
    db: &Db,
    session_id: &str,
) -> Result<Vec<SessionSummary>, String> {
    let options = FindOptions::builder().sort(doc! {"created_at": 1}).build();
    let cursor = collection(db)
        .find(doc! {"session_id": session_id})
        .with_options(options)
        .await
        .map_err(|e| e.to_string())?;

    cursor.try_collect().await.map_err(|e| e.to_string())
}

pub async fn list_summary_level_stats(
    db: &Db,
    session_id: &str,
) -> Result<Vec<(i64, i64, i64)>, String> {
    let pipeline = vec![
        doc! {"$match": {"session_id": session_id}},
        doc! {"$group": {
            "_id": "$level",
            "total": {"$sum": 1},
            "pending_count": {
                "$sum": {
                    "$cond": [
                        {"$eq": ["$status", "pending"]},
                        1,
                        0,
                    ]
                }
            }
        }},
        doc! {"$sort": {"_id": 1}},
    ];

    let cursor = db
        .collection::<mongodb::bson::Document>("session_summaries_v2")
        .aggregate(pipeline)
        .await
        .map_err(|e| e.to_string())?;
    let docs: Vec<mongodb::bson::Document> =
        cursor.try_collect().await.map_err(|e| e.to_string())?;

    let mut out = Vec::with_capacity(docs.len());
    for doc in docs {
        let level = doc_i64(&doc, "_id");
        let total = doc_i64(&doc, "total");
        let pending = doc_i64(&doc, "pending_count");
        out.push((level, total, pending));
    }
    Ok(out)
}

pub async fn list_pending_summaries_by_level_no_limit(
    db: &Db,
    session_id: &str,
    level: i64,
) -> Result<Vec<SessionSummary>, String> {
    let options = FindOptions::builder().sort(doc! {"created_at": 1}).build();
    let cursor = collection(db)
        .find(doc! {
            "session_id": session_id,
            "level": level,
            "status": "pending",
        })
        .with_options(options)
        .await
        .map_err(|e| e.to_string())?;

    cursor.try_collect().await.map_err(|e| e.to_string())
}

pub async fn list_session_ids_with_pending_rollup_by_user(
    db: &Db,
    user_id: &str,
    max_level: i64,
    limit: i64,
) -> Result<Vec<String>, String> {
    let pipeline = vec![
        doc! {"$match": {
            "status": "pending",
            "level": {"$lte": max_level.max(0)}
        }},
        doc! {"$lookup": {
            "from": "sessions",
            "localField": "session_id",
            "foreignField": "id",
            "as": "session"
        }},
        doc! {"$unwind": "$session"},
        doc! {"$match": {"session.user_id": user_id, "session.status": "active"}},
        doc! {"$group": {"_id": "$session_id", "min_created_at": {"$min": "$created_at"}}},
        doc! {"$sort": {"min_created_at": 1}},
        doc! {"$limit": limit.max(1).min(5000)},
        doc! {"$project": {"_id": 0, "session_id": "$_id"}},
    ];

    let cursor = db
        .collection::<mongodb::bson::Document>("session_summaries_v2")
        .aggregate(pipeline)
        .await
        .map_err(|e| e.to_string())?;
    let docs: Vec<mongodb::bson::Document> =
        cursor.try_collect().await.map_err(|e| e.to_string())?;

    Ok(docs
        .into_iter()
        .filter_map(|doc| doc.get_str("session_id").ok().map(|v| v.to_string()))
        .collect())
}

pub async fn mark_summaries_rolled_up(
    db: &Db,
    summary_ids: &[String],
    rollup_summary_id: &str,
) -> Result<usize, String> {
    if summary_ids.is_empty() {
        return Ok(0);
    }

    let now = now_rfc3339();
    let result = collection(db)
        .update_many(
            doc! {
                "id": {"$in": summary_ids.to_vec()},
                "status": "pending",
            },
            doc! {
                "$set": {
                    "status": "summarized",
                    "rollup_summary_id": rollup_summary_id,
                    "rolled_up_at": &now,
                    "updated_at": &now,
                }
            },
        )
        .await
        .map_err(|e| e.to_string())?;

    Ok(result.modified_count as usize)
}

pub async fn delete_summary(db: &Db, session_id: &str, summary_id: &str) -> Result<bool, String> {
    let result = collection(db)
        .delete_one(doc! {"session_id": session_id, "id": summary_id})
        .await
        .map_err(|e| e.to_string())?;
    Ok(result.deleted_count > 0)
}
