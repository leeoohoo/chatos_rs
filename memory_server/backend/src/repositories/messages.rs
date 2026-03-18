use futures_util::TryStreamExt;
use mongodb::bson::{doc, Bson};
use mongodb::options::FindOptions;
use uuid::Uuid;

use crate::db::Db;
use crate::models::{CreateMessageRequest, Message};

use super::now_rfc3339;

fn collection(db: &Db) -> mongodb::Collection<Message> {
    db.collection::<Message>("messages")
}

pub async fn create_message(
    db: &Db,
    session_id: &str,
    req: CreateMessageRequest,
) -> Result<Message, String> {
    let message = Message {
        id: Uuid::new_v4().to_string(),
        session_id: session_id.to_string(),
        role: req.role,
        content: req.content,
        message_mode: req.message_mode,
        message_source: req.message_source,
        tool_calls: req.tool_calls,
        tool_call_id: req.tool_call_id,
        reasoning: req.reasoning,
        metadata: req.metadata,
        summary_status: "pending".to_string(),
        summary_id: None,
        summarized_at: None,
        created_at: now_rfc3339(),
    };

    collection(db)
        .insert_one(message.clone())
        .await
        .map_err(|e| e.to_string())?;

    Ok(message)
}

#[derive(Debug, Clone)]
pub struct SyncMessageInput {
    pub message_id: String,
    pub role: String,
    pub content: String,
    pub message_mode: Option<String>,
    pub message_source: Option<String>,
    pub tool_calls_json: Option<String>,
    pub tool_call_id: Option<String>,
    pub reasoning: Option<String>,
    pub metadata_json: Option<String>,
    pub created_at: String,
}

pub async fn upsert_message_sync(
    db: &Db,
    session_id: &str,
    input: SyncMessageInput,
) -> Result<Message, String> {
    let tool_calls = input
        .tool_calls_json
        .and_then(|v| serde_json::from_str::<serde_json::Value>(v.as_str()).ok());
    let metadata = input
        .metadata_json
        .and_then(|v| serde_json::from_str::<serde_json::Value>(v.as_str()).ok());
    let tool_calls_bson = tool_calls
        .as_ref()
        .and_then(|v| mongodb::bson::to_bson(v).ok())
        .unwrap_or(Bson::Null);
    let metadata_bson = metadata
        .as_ref()
        .and_then(|v| mongodb::bson::to_bson(v).ok())
        .unwrap_or(Bson::Null);

    collection(db)
        .update_one(
            doc! {"id": &input.message_id},
            doc! {
                "$set": {
                    "session_id": session_id,
                    "role": &input.role,
                    "content": &input.content,
                    "message_mode": input.message_mode,
                    "message_source": input.message_source,
                    "tool_calls": tool_calls_bson,
                    "tool_call_id": input.tool_call_id,
                    "reasoning": input.reasoning,
                    "metadata": metadata_bson,
                    "created_at": input.created_at,
                },
                "$setOnInsert": {
                    "id": &input.message_id,
                    "summary_status": "pending",
                    "summary_id": Bson::Null,
                    "summarized_at": Bson::Null,
                }
            },
        )
        .upsert(true)
        .await
        .map_err(|e| e.to_string())?;

    get_message_by_id(db, input.message_id.as_str())
        .await?
        .ok_or_else(|| "upserted message not found".to_string())
}

pub async fn batch_create_messages(
    db: &Db,
    session_id: &str,
    requests: Vec<CreateMessageRequest>,
) -> Result<Vec<Message>, String> {
    let mut out = Vec::with_capacity(requests.len());
    for req in requests {
        out.push(create_message(db, session_id, req).await?);
    }
    Ok(out)
}

pub async fn get_message_by_id(db: &Db, message_id: &str) -> Result<Option<Message>, String> {
    collection(db)
        .find_one(doc! {"id": message_id})
        .await
        .map_err(|e| e.to_string())
}

pub async fn delete_message_by_id(db: &Db, message_id: &str) -> Result<bool, String> {
    let result = collection(db)
        .delete_one(doc! {"id": message_id})
        .await
        .map_err(|e| e.to_string())?;
    Ok(result.deleted_count > 0)
}

pub async fn delete_messages_by_session(db: &Db, session_id: &str) -> Result<i64, String> {
    let result = collection(db)
        .delete_many(doc! {"session_id": session_id})
        .await
        .map_err(|e| e.to_string())?;
    Ok(result.deleted_count as i64)
}

pub async fn list_messages_by_session(
    db: &Db,
    session_id: &str,
    limit: i64,
    offset: i64,
    asc: bool,
) -> Result<Vec<Message>, String> {
    let sort_order = if asc { 1 } else { -1 };
    let options = FindOptions::builder()
        .sort(doc! {"created_at": sort_order})
        .limit(Some(limit.max(1).min(2000)))
        .skip(Some(offset.max(0) as u64))
        .build();

    let cursor = collection(db)
        .find(doc! {"session_id": session_id})
        .with_options(options)
        .await
        .map_err(|e| e.to_string())?;

    cursor.try_collect().await.map_err(|e| e.to_string())
}

pub async fn list_pending_messages(
    db: &Db,
    session_id: &str,
    limit: Option<i64>,
) -> Result<Vec<Message>, String> {
    let options = if let Some(v) = limit {
        FindOptions::builder()
            .sort(doc! {"created_at": 1})
            .limit(Some(v.max(1)))
            .build()
    } else {
        FindOptions::builder().sort(doc! {"created_at": 1}).build()
    };

    let cursor = collection(db)
        .find(doc! {"session_id": session_id, "summary_status": "pending"})
        .with_options(options)
        .await
        .map_err(|e| e.to_string())?;

    cursor.try_collect().await.map_err(|e| e.to_string())
}

pub async fn mark_messages_summarized(
    db: &Db,
    session_id: &str,
    message_ids: &[String],
    summary_id: &str,
) -> Result<usize, String> {
    if message_ids.is_empty() {
        return Ok(0);
    }

    let now = now_rfc3339();
    let result = collection(db)
        .update_many(
            doc! {
                "session_id": session_id,
                "id": {"$in": message_ids.to_vec()},
                "summary_status": "pending",
            },
            doc! {
                "$set": {
                    "summary_status": "summarized",
                    "summary_id": summary_id,
                    "summarized_at": &now,
                }
            },
        )
        .await
        .map_err(|e| e.to_string())?;

    Ok(result.modified_count as usize)
}

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
