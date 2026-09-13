// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use futures::TryStreamExt;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;

use crate::models::memory_mapping::ChatosContact;
use crate::repositories::db::with_db;

use super::support::normalize_optional_text;

pub async fn list_contacts(
    user_id: &str,
    status: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<ChatosContact>, String> {
    with_db(|db| {
        let user_id = user_id.to_string();
        let status = normalize_optional_text(status);
        Box::pin(async move {
            let mut filter = doc! { "user_id": &user_id };
            if let Some(status) = status.as_deref() {
                filter.insert("status", status);
            }
            let options = FindOptions::builder()
                .sort(doc! { "updated_at": -1, "created_at": -1 })
                .limit(Some(limit.clamp(1, 500)))
                .skip(Some(offset.max(0) as u64))
                .build();
            let cursor = db
                .collection::<ChatosContact>("chatos_contacts")
                .find(filter, options)
                .await
                .map_err(|e| e.to_string())?;
            cursor
                .try_collect::<Vec<ChatosContact>>()
                .await
                .map_err(|e| e.to_string())
        })
    })
    .await
}

pub async fn get_contact_by_id(contact_id: &str) -> Result<Option<ChatosContact>, String> {
    with_db(|db| {
        let contact_id = contact_id.to_string();
        Box::pin(async move {
            db.collection::<ChatosContact>("chatos_contacts")
                .find_one(doc! { "id": &contact_id }, None)
                .await
                .map_err(|e| e.to_string())
        })
    })
    .await
}

pub async fn get_contact_by_user_and_agent(
    user_id: &str,
    agent_id: &str,
) -> Result<Option<ChatosContact>, String> {
    with_db(|db| {
        let user_id = user_id.to_string();
        let agent_id = agent_id.to_string();
        Box::pin(async move {
            db.collection::<ChatosContact>("chatos_contacts")
                .find_one(doc! { "user_id": &user_id, "agent_id": &agent_id }, None)
                .await
                .map_err(|e| e.to_string())
        })
    })
    .await
}

pub async fn list_contacts_by_ids(
    user_id: &str,
    contact_ids: &[String],
    status: Option<&str>,
) -> Result<Vec<ChatosContact>, String> {
    let ids = contact_ids
        .iter()
        .filter_map(|value| normalize_optional_text(Some(value.as_str())))
        .collect::<Vec<_>>();
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    with_db(|db| {
        let user_id = user_id.to_string();
        let status = normalize_optional_text(status);
        let ids = ids.clone();
        Box::pin(async move {
            let mut filter = doc! {
                "user_id": &user_id,
                "id": { "$in": ids },
            };
            if let Some(status) = status.as_deref() {
                filter.insert("status", status);
            }
            let options = FindOptions::builder()
                .sort(doc! { "updated_at": -1, "created_at": -1 })
                .build();
            let cursor = db
                .collection::<ChatosContact>("chatos_contacts")
                .find(filter, options)
                .await
                .map_err(|e| e.to_string())?;
            cursor
                .try_collect::<Vec<ChatosContact>>()
                .await
                .map_err(|e| e.to_string())
        })
    })
    .await
}

pub async fn create_contact_idempotent(
    user_id: &str,
    agent_id: &str,
    agent_name_snapshot: Option<String>,
) -> Result<(ChatosContact, bool), String> {
    if let Some(existing) = get_contact_by_user_and_agent(user_id, agent_id).await? {
        return Ok((existing, false));
    }
    let contact = ChatosContact::new(
        user_id.to_string(),
        agent_id.to_string(),
        agent_name_snapshot,
        "active".to_string(),
    );
    with_db(|db| {
        let contact = contact.clone();
        Box::pin(async move {
            match db
                .collection::<ChatosContact>("chatos_contacts")
                .insert_one(contact.clone(), None)
                .await
            {
                Ok(_) => Ok((contact, true)),
                Err(err) => {
                    if err.to_string().contains("E11000") {
                        let existing = db
                            .collection::<ChatosContact>("chatos_contacts")
                            .find_one(
                                doc! {
                                    "user_id": &contact.user_id,
                                    "agent_id": &contact.agent_id,
                                },
                                None,
                            )
                            .await
                            .map_err(|e| e.to_string())?;
                        if let Some(existing) = existing {
                            return Ok((existing, false));
                        }
                    }
                    Err(err.to_string())
                }
            }
        })
    })
    .await
}

pub async fn delete_contact_by_id(contact_id: &str) -> Result<bool, String> {
    with_db(|db| {
        let contact_id = contact_id.to_string();
        Box::pin(async move {
            let result = db
                .collection::<Document>("chatos_contacts")
                .delete_one(doc! { "id": &contact_id }, None)
                .await
                .map_err(|e| e.to_string())?;
            Ok(result.deleted_count > 0)
        })
    })
    .await
}
