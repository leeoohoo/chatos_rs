use futures_util::TryStreamExt;
use mongodb::bson::doc;
use mongodb::options::FindOptions;
use serde_json::Value;
use uuid::Uuid;

use crate::db::Db;
use crate::models::{ConversationMessage, CreateConversationMessageRequest};

use super::conversations;
use super::{normalize_optional_text, now_rfc3339};

fn collection(db: &Db) -> mongodb::Collection<ConversationMessage> {
    db.collection::<ConversationMessage>("conversation_messages")
}

fn metadata_attachment_items(metadata: &Option<Value>) -> Vec<&Value> {
    metadata
        .as_ref()
        .and_then(Value::as_object)
        .map(|object| {
            ["attachments", "attachments_payload"]
                .iter()
                .filter_map(|key| object.get(*key))
                .filter_map(Value::as_array)
                .flat_map(|items| items.iter())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn build_message_preview(content: &str, metadata: &Option<Value>) -> String {
    if !content.trim().is_empty() {
        return content.trim().to_string();
    }

    let attachments = metadata_attachment_items(metadata);
    if attachments.is_empty() {
        return String::new();
    }

    let names = attachments
        .iter()
        .filter_map(|item| item.get("name").and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .take(3)
        .collect::<Vec<_>>();

    if names.is_empty() {
        return format!("发送了 {} 个附件", attachments.len());
    }

    let suffix = if attachments.len() > names.len() { " 等" } else { "" };
    format!("发送了附件：{}{}", names.join("、"), suffix)
}

pub async fn create_message(
    db: &Db,
    conversation_id: &str,
    req: CreateConversationMessageRequest,
) -> Result<ConversationMessage, String> {
    let now = now_rfc3339();
    let content = req.content.trim().to_string();
    let has_attachments = !metadata_attachment_items(&req.metadata).is_empty();
    if content.is_empty() && !has_attachments {
        return Err("content is required".to_string());
    }

    let message = ConversationMessage {
        id: Uuid::new_v4().to_string(),
        conversation_id: conversation_id.trim().to_string(),
        sender_type: req.sender_type.trim().to_string(),
        sender_id: normalize_optional_text(req.sender_id.as_deref()),
        message_type: req
            .message_type
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "text".to_string()),
        content,
        delivery_status: req
            .delivery_status
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "sending".to_string()),
        client_message_id: normalize_optional_text(req.client_message_id.as_deref()),
        reply_to_message_id: normalize_optional_text(req.reply_to_message_id.as_deref()),
        metadata: req.metadata,
        created_at: now.clone(),
        updated_at: now.clone(),
    };

    collection(db)
        .insert_one(message.clone())
        .await
        .map_err(|e| e.to_string())?;

    let preview = build_message_preview(message.content.as_str(), &message.metadata);
    conversations::touch_conversation_after_message(
        db,
        conversation_id,
        preview.as_str(),
        message.created_at.as_str(),
        message.sender_type == "contact",
    )
    .await?;

    Ok(message)
}

pub async fn list_messages_by_conversation(
    db: &Db,
    conversation_id: &str,
    limit: i64,
    asc: bool,
) -> Result<Vec<ConversationMessage>, String> {
    let sort_order = if asc { 1 } else { -1 };
    let options = FindOptions::builder()
        .sort(doc! {"created_at": sort_order})
        .limit(Some(limit.max(1).min(2000)))
        .build();

    let cursor = collection(db)
        .find(doc! {"conversation_id": conversation_id.trim()})
        .with_options(options)
        .await
        .map_err(|e| e.to_string())?;

    cursor.try_collect().await.map_err(|e| e.to_string())
}
