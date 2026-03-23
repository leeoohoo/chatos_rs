use futures_util::TryStreamExt;
use mongodb::bson::{doc, Bson};
use mongodb::options::FindOptions;
use uuid::Uuid;

use crate::db::Db;
use crate::models::MemoryProjectAgentLink;

use super::{default_active_status, normalize_optional_text, now_rfc3339};

fn collection(db: &Db) -> mongodb::Collection<MemoryProjectAgentLink> {
    db.collection::<MemoryProjectAgentLink>("memory_project_agent_links")
}

pub struct UpsertProjectAgentLinkInput {
    pub user_id: String,
    pub project_id: String,
    pub agent_id: String,
    pub contact_id: Option<String>,
    pub latest_session_id: Option<String>,
    pub last_message_at: Option<String>,
    pub status: Option<String>,
}

pub async fn upsert_project_agent_link(
    db: &Db,
    input: UpsertProjectAgentLinkInput,
) -> Result<Option<MemoryProjectAgentLink>, String> {
    let now = now_rfc3339();
    let project_id = normalize_optional_text(Some(input.project_id.as_str()))
        .unwrap_or_else(|| "0".to_string());
    let status =
        normalize_optional_text(input.status.as_deref()).unwrap_or_else(default_active_status);
    let filter = doc! {
        "user_id": input.user_id.as_str(),
        "project_id": project_id.as_str(),
        "agent_id": input.agent_id.as_str(),
    };

    let mut set_doc = doc! {
        "user_id": input.user_id.as_str(),
        "project_id": project_id.as_str(),
        "agent_id": input.agent_id.as_str(),
        "status": status.as_str(),
        "last_bound_at": now.as_str(),
        "updated_at": now.as_str(),
    };
    if let Some(contact_id) = normalize_optional_text(input.contact_id.as_deref()) {
        set_doc.insert("contact_id", contact_id);
    } else {
        set_doc.insert("contact_id", Bson::Null);
    }
    if let Some(session_id) = normalize_optional_text(input.latest_session_id.as_deref()) {
        set_doc.insert("latest_session_id", session_id);
    } else {
        set_doc.insert("latest_session_id", Bson::Null);
    }
    if let Some(last_message_at) = normalize_optional_text(input.last_message_at.as_deref()) {
        set_doc.insert("last_message_at", last_message_at);
    }

    collection(db)
        .update_one(
            filter.clone(),
            doc! {
                "$set": set_doc,
                "$setOnInsert": {
                    "id": Uuid::new_v4().to_string(),
                    "first_bound_at": now.as_str(),
                    "created_at": now.as_str(),
                }
            },
        )
        .upsert(true)
        .await
        .map_err(|e| e.to_string())?;

    collection(db)
        .find_one(filter)
        .await
        .map_err(|e| e.to_string())
}

pub async fn list_project_agent_links_by_contact(
    db: &Db,
    user_id: &str,
    contact_id: &str,
    status: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<MemoryProjectAgentLink>, String> {
    let mut filter = doc! {
        "user_id": user_id,
        "contact_id": contact_id,
    };
    if let Some(status) = normalize_optional_text(status) {
        filter.insert("status", status);
    }

    let options = FindOptions::builder()
        .sort(doc! {"last_bound_at": -1, "updated_at": -1})
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

pub async fn list_project_agent_links_by_project(
    db: &Db,
    user_id: &str,
    project_id: &str,
    status: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<MemoryProjectAgentLink>, String> {
    let mut filter = doc! {
        "user_id": user_id,
        "project_id": project_id,
    };
    if let Some(status) = normalize_optional_text(status) {
        filter.insert("status", status);
    }

    let options = FindOptions::builder()
        .sort(doc! {"last_bound_at": -1, "updated_at": -1})
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
