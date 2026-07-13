// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use futures::TryStreamExt;
use mongodb::bson::{doc, Document};
use mongodb::options::{FindOneOptions, FindOptions, UpdateOptions};

use crate::core::values::optional_string_bson;
use crate::models::memory_mapping::ChatosProjectAgentLink;
use crate::repositories::db::with_db;

use super::support::{normalize_optional_text, normalize_project_id};

#[derive(Debug, Clone)]
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
    input: UpsertProjectAgentLinkInput,
) -> Result<Option<ChatosProjectAgentLink>, String> {
    let now = crate::core::time::now_rfc3339();
    let project_id = normalize_project_id(input.project_id.as_str());
    let status =
        normalize_optional_text(input.status.as_deref()).unwrap_or_else(|| "active".to_string());

    with_db(|db| {
        let input = input.clone();
        let now = now.clone();
        let project_id = project_id.clone();
        let status = status.clone();
        Box::pin(async move {
            let filter = doc! {
                "user_id": &input.user_id,
                "project_id": &project_id,
            };
            let collection = db.collection::<ChatosProjectAgentLink>("chatos_project_agent_links");
            let existing = collection
                .find_one(
                    filter.clone(),
                    FindOneOptions::builder()
                        .sort(doc! { "last_bound_at": -1, "updated_at": -1, "created_at": -1 })
                        .build(),
                )
                .await
                .map_err(|e| e.to_string())?;
            if let Some(existing) = existing.as_ref() {
                db.collection::<Document>("chatos_project_agent_links")
                    .delete_many(
                        doc! {
                            "user_id": &input.user_id,
                            "project_id": &project_id,
                            "id": { "$ne": &existing.id },
                        },
                        None,
                    )
                    .await
                    .map_err(|e| e.to_string())?;
            }
            let replaces_contact = existing.as_ref().is_some_and(|item| {
                item.agent_id != input.agent_id || item.contact_id != input.contact_id
            });
            let mut set_doc = doc! {
                "user_id": &input.user_id,
                "project_id": &project_id,
                "agent_id": &input.agent_id,
                "contact_id": optional_string_bson(input.contact_id.clone()),
                "status": &status,
                "last_bound_at": &now,
                "updated_at": &now,
            };
            if input.latest_session_id.is_some() || replaces_contact || existing.is_none() {
                set_doc.insert(
                    "latest_session_id",
                    optional_string_bson(input.latest_session_id.clone()),
                );
            }
            if input.last_message_at.is_some() || replaces_contact || existing.is_none() {
                set_doc.insert(
                    "last_message_at",
                    optional_string_bson(input.last_message_at.clone()),
                );
            }
            let update_options = UpdateOptions::builder().upsert(true).build();
            db.collection::<Document>("chatos_project_agent_links")
                .update_one(
                    filter.clone(),
                    doc! {
                        "$set": set_doc,
                        "$setOnInsert": {
                            "id": uuid::Uuid::new_v4().to_string(),
                            "first_bound_at": &now,
                            "created_at": &now,
                        }
                    },
                    update_options,
                )
                .await
                .map_err(|e| e.to_string())?;
            collection
                .find_one(
                    filter,
                    FindOneOptions::builder()
                        .sort(doc! { "last_bound_at": -1, "updated_at": -1, "created_at": -1 })
                        .build(),
                )
                .await
                .map_err(|e| e.to_string())
        })
    })
    .await
}

#[derive(Debug, Clone)]
pub struct TouchProjectAgentLinkSessionInput {
    pub user_id: String,
    pub project_id: String,
    pub agent_id: String,
    pub contact_id: String,
    pub latest_session_id: String,
    pub last_message_at: String,
}

pub async fn touch_project_agent_link_session(
    input: TouchProjectAgentLinkSessionInput,
) -> Result<Option<ChatosProjectAgentLink>, String> {
    let now = crate::core::time::now_rfc3339();
    let project_id = normalize_project_id(input.project_id.as_str());
    with_db(|db| {
        let input = input.clone();
        let now = now.clone();
        let project_id = project_id.clone();
        Box::pin(async move {
            let filter = doc! {
                "user_id": &input.user_id,
                "project_id": &project_id,
                "contact_id": &input.contact_id,
                "status": "active",
            };
            db.collection::<Document>("chatos_project_agent_links")
                .update_one(
                    filter.clone(),
                    doc! {
                        "$set": {
                            "agent_id": &input.agent_id,
                            "latest_session_id": &input.latest_session_id,
                            "last_message_at": &input.last_message_at,
                            "updated_at": &now,
                        }
                    },
                    None,
                )
                .await
                .map_err(|e| e.to_string())?;
            db.collection::<ChatosProjectAgentLink>("chatos_project_agent_links")
                .find_one(filter, None)
                .await
                .map_err(|e| e.to_string())
        })
    })
    .await
}

pub async fn delete_project_agent_link(
    user_id: &str,
    project_id: &str,
    contact_id: Option<&str>,
) -> Result<bool, String> {
    let user_id = user_id.to_string();
    let project_id = normalize_project_id(project_id);
    let contact_id = contact_id.and_then(|value| normalize_optional_text(Some(value)));
    with_db(|db| {
        let user_id = user_id.clone();
        let project_id = project_id.clone();
        let contact_id = contact_id.clone();
        Box::pin(async move {
            let mut filter = doc! {
                "user_id": &user_id,
                "project_id": &project_id,
            };
            if let Some(contact_id) = contact_id.as_deref() {
                filter.insert("contact_id", contact_id);
            }
            let result = db
                .collection::<Document>("chatos_project_agent_links")
                .delete_one(filter, None)
                .await
                .map_err(|e| e.to_string())?;
            Ok(result.deleted_count > 0)
        })
    })
    .await
}

pub async fn list_project_agent_links_by_contact(
    user_id: &str,
    contact_id: &str,
    status: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<ChatosProjectAgentLink>, String> {
    with_db(|db| {
        let user_id = user_id.to_string();
        let contact_id = contact_id.to_string();
        let status = normalize_optional_text(status);
        Box::pin(async move {
            let mut filter = doc! {
                "user_id": &user_id,
                "contact_id": &contact_id,
            };
            if let Some(status) = status.as_deref() {
                filter.insert("status", status);
            }
            let options = FindOptions::builder()
                .sort(doc! { "last_bound_at": -1, "updated_at": -1 })
                .limit(Some(limit.clamp(1, 500)))
                .skip(Some(offset.max(0) as u64))
                .build();
            let cursor = db
                .collection::<ChatosProjectAgentLink>("chatos_project_agent_links")
                .find(filter, options)
                .await
                .map_err(|e| e.to_string())?;
            cursor
                .try_collect::<Vec<ChatosProjectAgentLink>>()
                .await
                .map_err(|e| e.to_string())
        })
    })
    .await
}

pub async fn list_project_agent_links_by_project(
    user_id: &str,
    project_id: &str,
    status: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<ChatosProjectAgentLink>, String> {
    let project_id = normalize_project_id(project_id);
    with_db(|db| {
        let user_id = user_id.to_string();
        let project_id = project_id.clone();
        let status = normalize_optional_text(status);
        Box::pin(async move {
            let mut filter = doc! {
                "user_id": &user_id,
                "project_id": &project_id,
            };
            if let Some(status) = status.as_deref() {
                filter.insert("status", status);
            }
            let options = FindOptions::builder()
                .sort(doc! { "last_bound_at": -1, "updated_at": -1 })
                .limit(Some(limit.clamp(1, 500)))
                .skip(Some(offset.max(0) as u64))
                .build();
            let cursor = db
                .collection::<ChatosProjectAgentLink>("chatos_project_agent_links")
                .find(filter, options)
                .await
                .map_err(|e| e.to_string())?;
            cursor
                .try_collect::<Vec<ChatosProjectAgentLink>>()
                .await
                .map_err(|e| e.to_string())
        })
    })
    .await
}
