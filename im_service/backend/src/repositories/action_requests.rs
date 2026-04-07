use futures_util::TryStreamExt;
use mongodb::bson::{doc, Bson};
use mongodb::options::FindOptions;
use uuid::Uuid;

use crate::db::Db;
use crate::models::{
    ConversationActionRequest, CreateConversationActionRequest, UpdateConversationActionRequest,
};

use super::{normalize_optional_text, now_rfc3339};

fn collection(db: &Db) -> mongodb::Collection<ConversationActionRequest> {
    db.collection::<ConversationActionRequest>("conversation_action_requests")
}

pub async fn create_action_request(
    db: &Db,
    req: CreateConversationActionRequest,
) -> Result<ConversationActionRequest, String> {
    let now = now_rfc3339();
    let item = ConversationActionRequest {
        id: Uuid::new_v4().to_string(),
        conversation_id: req.conversation_id.trim().to_string(),
        trigger_message_id: req.trigger_message_id,
        run_id: req.run_id,
        action_type: req.action_type.trim().to_string(),
        status: req
            .status
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "pending".to_string()),
        payload: req.payload,
        submitted_payload: req.submitted_payload,
        created_at: now.clone(),
        updated_at: now,
    };

    collection(db)
        .insert_one(item.clone())
        .await
        .map_err(|e| e.to_string())?;

    Ok(item)
}

pub async fn list_action_requests_by_conversation(
    db: &Db,
    conversation_id: &str,
    limit: i64,
) -> Result<Vec<ConversationActionRequest>, String> {
    let options = FindOptions::builder()
        .sort(doc! {"created_at": -1})
        .limit(Some(limit.max(1)))
        .build();

    let cursor = collection(db)
        .find(doc! {"conversation_id": conversation_id.trim()})
        .with_options(options)
        .await
        .map_err(|e| e.to_string())?;

    cursor.try_collect().await.map_err(|e| e.to_string())
}

pub async fn get_action_request_by_id(
    db: &Db,
    action_request_id: &str,
) -> Result<Option<ConversationActionRequest>, String> {
    collection(db)
        .find_one(doc! {"id": action_request_id.trim()})
        .await
        .map_err(|e| e.to_string())
}

pub async fn update_action_request(
    db: &Db,
    action_request_id: &str,
    req: UpdateConversationActionRequest,
) -> Result<Option<ConversationActionRequest>, String> {
    let Some(existing) = get_action_request_by_id(db, action_request_id).await? else {
        return Ok(None);
    };

    let mut update_fields = doc! {
        "updated_at": now_rfc3339(),
    };

    if let Some(status) = normalize_optional_text(req.status.as_deref()) {
        update_fields.insert("status", status);
    }
    if let Some(submitted_payload) = req.submitted_payload {
        update_fields.insert("submitted_payload", bson_value(submitted_payload));
    }

    collection(db)
        .update_one(doc! {"id": &existing.id}, doc! {"$set": update_fields})
        .await
        .map_err(|e| e.to_string())?;

    get_action_request_by_id(db, action_request_id).await
}

fn bson_value(value: serde_json::Value) -> Bson {
    mongodb::bson::to_bson(&value).unwrap_or(Bson::Null)
}
