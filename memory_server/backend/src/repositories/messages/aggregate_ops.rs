use futures_util::TryStreamExt;
use mongodb::bson::{doc, Document};

use crate::db::Db;
use crate::repositories::session_support::{
    build_contact_or_conditions, contact_or_agent_presence_match, project_scope_condition,
};

fn prefix_condition_keys(prefix: &str, row: &Document) -> Document {
    let mut out = Document::new();
    for (key, value) in row {
        out.insert(format!("{}.{}", prefix, key), value.clone());
    }
    out
}

pub async fn list_session_ids_with_pending_messages_by_user(
    db: &Db,
    user_id: &str,
    limit: i64,
) -> Result<Vec<String>, String> {
    let contact_match = contact_or_agent_presence_match("session");
    let pipeline = vec![
        doc! {"$match": {"summary_status": "pending"}},
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
        .collection::<mongodb::bson::Document>("messages")
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

pub async fn list_session_ids_with_pending_messages_by_scope(
    db: &Db,
    user_id: &str,
    project_id: &str,
    contact_id: Option<&str>,
    agent_id: Option<&str>,
    limit: i64,
) -> Result<Vec<String>, String> {
    let contact_conditions = build_contact_or_conditions(contact_id, agent_id)
        .into_iter()
        .map(|row| prefix_condition_keys("session", &row))
        .collect::<Vec<_>>();
    if contact_conditions.is_empty() {
        return Ok(Vec::new());
    }

    let project_scope = if project_id == "0" {
        prefix_condition_keys("session", &project_scope_condition(project_id))
    } else {
        doc! {"session.project_id": project_id}
    };

    let session_scope_match = doc! {
        "$and": [
            {"$or": contact_conditions},
            project_scope
        ]
    };

    let pipeline = vec![
        doc! {"$match": {"summary_status": "pending"}},
        doc! {"$lookup": {
            "from": "sessions",
            "localField": "session_id",
            "foreignField": "id",
            "as": "session"
        }},
        doc! {"$unwind": "$session"},
        doc! {"$match": {
            "session.user_id": user_id,
            "session.status": "active",
        }},
        doc! {"$match": session_scope_match},
        doc! {"$group": {"_id": "$session_id", "min_created_at": {"$min": "$created_at"}}},
        doc! {"$sort": {"min_created_at": 1}},
        doc! {"$limit": limit.max(1).min(5000)},
        doc! {"$project": {"_id": 0, "session_id": "$_id"}},
    ];

    let cursor = db
        .collection::<mongodb::bson::Document>("messages")
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

pub async fn count_pending_messages_by_scope(
    db: &Db,
    user_id: &str,
    project_id: &str,
    contact_id: Option<&str>,
    agent_id: Option<&str>,
) -> Result<i64, String> {
    let contact_conditions = build_contact_or_conditions(contact_id, agent_id)
        .into_iter()
        .map(|row| prefix_condition_keys("session", &row))
        .collect::<Vec<_>>();
    if contact_conditions.is_empty() {
        return Ok(0);
    }

    let project_scope = if project_id == "0" {
        prefix_condition_keys("session", &project_scope_condition(project_id))
    } else {
        doc! {"session.project_id": project_id}
    };

    let session_scope_match = doc! {
        "$and": [
            {"$or": contact_conditions},
            project_scope
        ]
    };

    let pipeline = vec![
        doc! {"$match": {"summary_status": "pending"}},
        doc! {"$lookup": {
            "from": "sessions",
            "localField": "session_id",
            "foreignField": "id",
            "as": "session"
        }},
        doc! {"$unwind": "$session"},
        doc! {"$match": {
            "session.user_id": user_id,
            "session.status": "active",
        }},
        doc! {"$match": session_scope_match},
        doc! {"$count": "pending_count"},
    ];

    let cursor = db
        .collection::<mongodb::bson::Document>("messages")
        .aggregate(pipeline)
        .await
        .map_err(|e| e.to_string())?;
    let docs: Vec<mongodb::bson::Document> =
        cursor.try_collect().await.map_err(|e| e.to_string())?;

    Ok(docs
        .first()
        .and_then(|doc| doc.get_i64("pending_count").ok())
        .unwrap_or(0))
}
