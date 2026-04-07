use futures_util::TryStreamExt;
use mongodb::bson::{doc, Bson};
use mongodb::options::FindOptions;
use uuid::Uuid;

use crate::db::Db;
use crate::models::{CreateConversationRequest, ImConversation, UpdateConversationRequest};

use super::{normalize_optional_text, now_rfc3339};

fn collection(db: &Db) -> mongodb::Collection<ImConversation> {
    db.collection::<ImConversation>("conversations")
}

pub async fn create_conversation(
    db: &Db,
    req: CreateConversationRequest,
) -> Result<ImConversation, String> {
    let now = now_rfc3339();
    let conversation = ImConversation {
        id: Uuid::new_v4().to_string(),
        owner_user_id: req.owner_user_id.trim().to_string(),
        contact_id: req.contact_id.trim().to_string(),
        project_id: normalize_optional_text(req.project_id.as_deref()),
        title: normalize_optional_text(req.title.as_deref()),
        status: "active".to_string(),
        last_message_at: None,
        last_message_preview: None,
        unread_count: 0,
        created_at: now.clone(),
        updated_at: now,
    };

    collection(db)
        .insert_one(conversation.clone())
        .await
        .map_err(|e| e.to_string())?;

    Ok(conversation)
}

pub async fn get_conversation_by_id(
    db: &Db,
    conversation_id: &str,
) -> Result<Option<ImConversation>, String> {
    collection(db)
        .find_one(doc! {"id": conversation_id.trim()})
        .await
        .map_err(|e| e.to_string())
}

pub async fn list_conversations_by_owner(
    db: &Db,
    owner_user_id: &str,
    limit: i64,
) -> Result<Vec<ImConversation>, String> {
    let options = FindOptions::builder()
        .sort(doc! {"updated_at": -1, "created_at": -1})
        .limit(Some(limit.max(1)))
        .build();

    let cursor = collection(db)
        .find(doc! {"owner_user_id": owner_user_id.trim()})
        .with_options(options)
        .await
        .map_err(|e| e.to_string())?;

    cursor.try_collect().await.map_err(|e| e.to_string())
}

pub async fn update_conversation(
    db: &Db,
    conversation_id: &str,
    req: UpdateConversationRequest,
) -> Result<Option<ImConversation>, String> {
    let Some(existing) = get_conversation_by_id(db, conversation_id).await? else {
        return Ok(None);
    };

    let mut update_fields = doc! {
        "updated_at": now_rfc3339(),
    };

    if let Some(title) = req.title {
        update_fields.insert(
            "title",
            normalize_optional_text(Some(title.as_str()))
                .map(Bson::String)
                .unwrap_or(Bson::Null),
        );
    }
    if let Some(status) = normalize_optional_text(req.status.as_deref()) {
        update_fields.insert("status", status);
    }
    if let Some(last_message_at) = normalize_optional_text(req.last_message_at.as_deref()) {
        update_fields.insert("last_message_at", last_message_at);
    }
    if let Some(last_message_preview) = req.last_message_preview {
        update_fields.insert(
            "last_message_preview",
            normalize_optional_text(Some(last_message_preview.as_str()))
                .map(Bson::String)
                .unwrap_or(Bson::Null),
        );
    }
    if let Some(unread_count) = req.unread_count {
        update_fields.insert("unread_count", unread_count.max(0));
    }

    collection(db)
        .update_one(doc! {"id": &existing.id}, doc! {"$set": update_fields})
        .await
        .map_err(|e| e.to_string())?;

    get_conversation_by_id(db, conversation_id).await
}

pub async fn touch_conversation_after_message(
    db: &Db,
    conversation_id: &str,
    preview: &str,
    message_created_at: &str,
    increment_unread: bool,
) -> Result<(), String> {
    let preview_text = preview.chars().take(120).collect::<String>();
    let update_doc = if increment_unread {
        doc! {
            "$set": {
                "last_message_at": message_created_at,
                "last_message_preview": preview_text,
                "updated_at": now_rfc3339(),
            },
            "$inc": {
                "unread_count": 1
            }
        }
    } else {
        doc! {
            "$set": {
                "last_message_at": message_created_at,
                "last_message_preview": preview_text,
                "updated_at": now_rfc3339(),
            }
        }
    };

    collection(db)
        .update_one(doc! {"id": conversation_id.trim()}, update_doc)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn mark_conversation_read(
    db: &Db,
    conversation_id: &str,
) -> Result<Option<ImConversation>, String> {
    let Some(existing) = get_conversation_by_id(db, conversation_id).await? else {
        return Ok(None);
    };

    collection(db)
        .update_one(
            doc! {"id": &existing.id},
            doc! {"$set": {"unread_count": 0i64}},
        )
        .await
        .map_err(|e| e.to_string())?;

    get_conversation_by_id(db, conversation_id).await
}
