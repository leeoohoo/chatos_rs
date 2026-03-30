use futures_util::TryStreamExt;
use mongodb::bson::doc;

use crate::db::Db;
use crate::repositories::session_support::contact_or_agent_presence_match;

use super::{doc_i64, summary_agent_id_expr, summary_project_id_expr, AgentMemorySummarySource};

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

pub async fn list_session_ids_with_pending_rollup_by_user(
    db: &Db,
    user_id: &str,
    max_level: i64,
    limit: i64,
) -> Result<Vec<String>, String> {
    let contact_match = contact_or_agent_presence_match("session");
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
        doc! {"$match": contact_match},
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

pub async fn list_agent_ids_with_pending_agent_memory_by_user(
    db: &Db,
    user_id: &str,
    limit: i64,
) -> Result<Vec<String>, String> {
    let pipeline = vec![
        doc! {"$match": {
            "agent_memory_summarized": {"$ne": 1},
        }},
        doc! {"$lookup": {
            "from": "sessions",
            "localField": "session_id",
            "foreignField": "id",
            "as": "session"
        }},
        doc! {"$unwind": "$session"},
        doc! {"$match": {"session.user_id": user_id, "session.status": "active"}},
        doc! {"$addFields": {"agent_id": summary_agent_id_expr()}},
        doc! {"$match": {"agent_id": {"$exists": true, "$type": "string", "$ne": ""}}},
        doc! {"$group": {"_id": "$agent_id", "min_created_at": {"$min": "$created_at"}}},
        doc! {"$sort": {"min_created_at": 1}},
        doc! {"$limit": limit.max(1).min(5000)},
        doc! {"$project": {"_id": 0, "agent_id": "$_id"}},
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
        .filter_map(|doc| doc.get_str("agent_id").ok().map(|value| value.to_string()))
        .collect())
}

pub async fn list_pending_agent_memory_summaries_by_agent(
    db: &Db,
    user_id: &str,
    agent_id: &str,
) -> Result<Vec<AgentMemorySummarySource>, String> {
    let pipeline = vec![
        doc! {"$match": {
            "agent_memory_summarized": {"$ne": 1},
        }},
        doc! {"$lookup": {
            "from": "sessions",
            "localField": "session_id",
            "foreignField": "id",
            "as": "session"
        }},
        doc! {"$unwind": "$session"},
        doc! {"$match": {"session.user_id": user_id, "session.status": "active"}},
        doc! {"$addFields": {
            "agent_id": summary_agent_id_expr(),
            "project_id": summary_project_id_expr(),
        }},
        doc! {"$match": {"agent_id": agent_id}},
        doc! {"$sort": {"created_at": 1}},
        doc! {"$project": {
            "_id": 0,
            "id": 1,
            "session_id": 1,
            "summary_text": 1,
            "summary_model": 1,
            "trigger_type": 1,
            "source_start_message_id": 1,
            "source_end_message_id": 1,
            "source_message_count": 1,
            "source_estimated_tokens": 1,
            "status": 1,
            "level": 1,
            "project_id": 1,
            "created_at": 1,
            "updated_at": 1,
        }},
    ];

    let cursor = db
        .collection::<mongodb::bson::Document>("session_summaries_v2")
        .aggregate(pipeline)
        .await
        .map_err(|e| e.to_string())?;
    let docs: Vec<mongodb::bson::Document> =
        cursor.try_collect().await.map_err(|e| e.to_string())?;
    docs.into_iter()
        .map(|doc| {
            mongodb::bson::from_document::<AgentMemorySummarySource>(doc).map_err(|e| e.to_string())
        })
        .collect()
}
