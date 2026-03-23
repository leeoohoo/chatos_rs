use futures_util::TryStreamExt;
use mongodb::bson::doc;

use crate::db::Db;

pub async fn list_session_ids_with_pending_messages_by_user(
    db: &Db,
    user_id: &str,
    limit: i64,
) -> Result<Vec<String>, String> {
    let contact_match = doc! {
        "$or": [
            {"session.metadata.contact.contact_id": {"$exists": true, "$type": "string"}},
            {"session.metadata.ui_contact.contact_id": {"$exists": true, "$type": "string"}},
            {"session.metadata.contact.agent_id": {"$exists": true, "$type": "string"}},
            {"session.metadata.ui_contact.agent_id": {"$exists": true, "$type": "string"}},
            {"session.metadata.ui_chat_selection.selected_agent_id": {"$exists": true, "$type": "string"}}
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
