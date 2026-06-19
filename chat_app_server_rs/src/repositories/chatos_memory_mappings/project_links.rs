use futures::TryStreamExt;
use mongodb::bson::{Document, doc};
use mongodb::options::{FindOneOptions, FindOptions, UpdateOptions};

use crate::core::values::optional_string_bson;
use crate::models::memory_mapping::{ChatosProjectAgentLink, ChatosProjectAgentLinkRow};
use crate::repositories::db::with_db;

use super::support::normalize_optional_text;

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
    let project_id =
        normalize_optional_text(Some(input.project_id.as_str())).unwrap_or_else(|| "0".to_string());
    let status =
        normalize_optional_text(input.status.as_deref()).unwrap_or_else(|| "active".to_string());

    with_db(
        |db| {
            let input = input.clone();
            let now = now.clone();
            let project_id = project_id.clone();
            let status = status.clone();
            Box::pin(async move {
                let filter = doc! {
                    "user_id": &input.user_id,
                    "project_id": &project_id,
                };
                let collection =
                    db.collection::<ChatosProjectAgentLink>("chatos_project_agent_links");
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
        },
        |pool| {
            let input = input.clone();
            let now = now.clone();
            let project_id = project_id.clone();
            let status = status.clone();
            Box::pin(async move {
                let existing = sqlx::query_as::<_, ChatosProjectAgentLinkRow>(
                    "SELECT * FROM chatos_project_agent_links \
                    WHERE user_id = ? AND project_id = ? \
                    ORDER BY last_bound_at DESC, updated_at DESC, created_at DESC LIMIT 1",
                )
                .bind(&input.user_id)
                .bind(&project_id)
                .fetch_optional(pool)
                .await
                .map_err(|e| e.to_string())?;

                if let Some(row) = existing {
                    sqlx::query(
                        "DELETE FROM chatos_project_agent_links \
                        WHERE user_id = ? AND project_id = ? AND id != ?",
                    )
                    .bind(&input.user_id)
                    .bind(&project_id)
                    .bind(&row.id)
                    .execute(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                    let replaces_contact =
                        row.agent_id != input.agent_id || row.contact_id != input.contact_id;
                    let latest_session_id = if input.latest_session_id.is_some() || replaces_contact
                    {
                        input.latest_session_id.clone()
                    } else {
                        row.latest_session_id.clone()
                    };
                    let last_message_at = if input.last_message_at.is_some() || replaces_contact {
                        input.last_message_at.clone()
                    } else {
                        row.last_message_at.clone()
                    };
                    sqlx::query(
                        "UPDATE chatos_project_agent_links SET \
                        agent_id = ?, contact_id = ?, latest_session_id = ?, last_bound_at = ?, last_message_at = ?, status = ?, updated_at = ? \
                        WHERE id = ?",
                    )
                    .bind(&input.agent_id)
                    .bind(&input.contact_id)
                    .bind(&latest_session_id)
                    .bind(&now)
                    .bind(&last_message_at)
                    .bind(&status)
                    .bind(&now)
                    .bind(&row.id)
                    .execute(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                } else {
                    sqlx::query(
                        "INSERT INTO chatos_project_agent_links \
                        (id, user_id, project_id, agent_id, contact_id, latest_session_id, first_bound_at, last_bound_at, last_message_at, status, created_at, updated_at) \
                        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    )
                    .bind(uuid::Uuid::new_v4().to_string())
                    .bind(&input.user_id)
                    .bind(&project_id)
                    .bind(&input.agent_id)
                    .bind(&input.contact_id)
                    .bind(&input.latest_session_id)
                    .bind(&now)
                    .bind(&now)
                    .bind(&input.last_message_at)
                    .bind(&status)
                    .bind(&now)
                    .bind(&now)
                    .execute(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                }

                let row = sqlx::query_as::<_, ChatosProjectAgentLinkRow>(
                    "SELECT * FROM chatos_project_agent_links \
                    WHERE user_id = ? AND project_id = ? \
                    ORDER BY last_bound_at DESC, updated_at DESC, created_at DESC LIMIT 1",
                )
                .bind(&input.user_id)
                .bind(&project_id)
                .fetch_optional(pool)
                .await
                .map_err(|e| e.to_string())?;
                Ok(row.map(ChatosProjectAgentLinkRow::to_link))
            })
        },
    )
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
    let project_id =
        normalize_optional_text(Some(input.project_id.as_str())).unwrap_or_else(|| "0".to_string());
    with_db(
        |db| {
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
        },
        |pool| {
            let input = input.clone();
            let now = now.clone();
            let project_id = project_id.clone();
            Box::pin(async move {
                let result = sqlx::query(
                    "UPDATE chatos_project_agent_links SET \
                    agent_id = ?, latest_session_id = ?, last_message_at = ?, updated_at = ? \
                    WHERE user_id = ? AND project_id = ? AND contact_id = ? AND status = 'active'",
                )
                .bind(&input.agent_id)
                .bind(&input.latest_session_id)
                .bind(&input.last_message_at)
                .bind(&now)
                .bind(&input.user_id)
                .bind(&project_id)
                .bind(&input.contact_id)
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;
                if result.rows_affected() == 0 {
                    return Ok(None);
                }
                let row = sqlx::query_as::<_, ChatosProjectAgentLinkRow>(
                    "SELECT * FROM chatos_project_agent_links \
                    WHERE user_id = ? AND project_id = ? AND contact_id = ? AND status = 'active'",
                )
                .bind(&input.user_id)
                .bind(&project_id)
                .bind(&input.contact_id)
                .fetch_optional(pool)
                .await
                .map_err(|e| e.to_string())?;
                Ok(row.map(ChatosProjectAgentLinkRow::to_link))
            })
        },
    )
    .await
}

pub async fn delete_project_agent_link(
    user_id: &str,
    project_id: &str,
    contact_id: Option<&str>,
) -> Result<bool, String> {
    let user_id = user_id.to_string();
    let project_id = normalize_optional_text(Some(project_id)).unwrap_or_else(|| "0".to_string());
    let contact_id = contact_id.and_then(|value| normalize_optional_text(Some(value)));
    with_db(
        |db| {
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
        },
        |pool| {
            let user_id = user_id.clone();
            let project_id = project_id.clone();
            let contact_id = contact_id.clone();
            Box::pin(async move {
                let result = if let Some(contact_id) = contact_id.as_deref() {
                    sqlx::query(
                        "DELETE FROM chatos_project_agent_links \
                        WHERE user_id = ? AND project_id = ? AND contact_id = ?",
                    )
                    .bind(&user_id)
                    .bind(&project_id)
                    .bind(contact_id)
                    .execute(pool)
                    .await
                    .map_err(|e| e.to_string())?
                } else {
                    sqlx::query(
                        "DELETE FROM chatos_project_agent_links WHERE user_id = ? AND project_id = ?",
                    )
                    .bind(&user_id)
                    .bind(&project_id)
                    .execute(pool)
                    .await
                    .map_err(|e| e.to_string())?
                };
                Ok(result.rows_affected() > 0)
            })
        },
    )
    .await
}

pub async fn list_project_agent_links_by_contact(
    user_id: &str,
    contact_id: &str,
    status: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<ChatosProjectAgentLink>, String> {
    with_db(
        |db| {
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
                    .limit(Some(limit.max(1).min(500)))
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
        },
        |pool| {
            let user_id = user_id.to_string();
            let contact_id = contact_id.to_string();
            let status = normalize_optional_text(status);
            Box::pin(async move {
                let mut sql =
                    "SELECT * FROM chatos_project_agent_links WHERE user_id = ? AND contact_id = ?"
                        .to_string();
                if status.is_some() {
                    sql.push_str(" AND status = ?");
                }
                sql.push_str(" ORDER BY last_bound_at DESC, updated_at DESC LIMIT ? OFFSET ?");
                let mut query = sqlx::query_as::<_, ChatosProjectAgentLinkRow>(&sql)
                    .bind(&user_id)
                    .bind(&contact_id);
                if let Some(status) = status.as_deref() {
                    query = query.bind(status);
                }
                query = query.bind(limit.max(1).min(500)).bind(offset.max(0));
                let rows = query.fetch_all(pool).await.map_err(|e| e.to_string())?;
                Ok(rows
                    .into_iter()
                    .map(ChatosProjectAgentLinkRow::to_link)
                    .collect())
            })
        },
    )
    .await
}

pub async fn list_project_agent_links_by_project(
    user_id: &str,
    project_id: &str,
    status: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<ChatosProjectAgentLink>, String> {
    with_db(
        |db| {
            let user_id = user_id.to_string();
            let project_id = project_id.to_string();
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
                    .limit(Some(limit.max(1).min(500)))
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
        },
        |pool| {
            let user_id = user_id.to_string();
            let project_id = project_id.to_string();
            let status = normalize_optional_text(status);
            Box::pin(async move {
                let mut sql =
                    "SELECT * FROM chatos_project_agent_links WHERE user_id = ? AND project_id = ?"
                        .to_string();
                if status.is_some() {
                    sql.push_str(" AND status = ?");
                }
                sql.push_str(" ORDER BY last_bound_at DESC, updated_at DESC LIMIT ? OFFSET ?");
                let mut query = sqlx::query_as::<_, ChatosProjectAgentLinkRow>(&sql)
                    .bind(&user_id)
                    .bind(&project_id);
                if let Some(status) = status.as_deref() {
                    query = query.bind(status);
                }
                query = query.bind(limit.max(1).min(500)).bind(offset.max(0));
                let rows = query.fetch_all(pool).await.map_err(|e| e.to_string())?;
                Ok(rows
                    .into_iter()
                    .map(ChatosProjectAgentLinkRow::to_link)
                    .collect())
            })
        },
    )
    .await
}
