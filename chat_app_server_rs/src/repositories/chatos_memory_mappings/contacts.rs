// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use futures::TryStreamExt;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use sqlx::{QueryBuilder, Sqlite};

use crate::core::values::optional_string_bson;
use crate::models::memory_mapping::{ChatosContact, ChatosContactRow};
use crate::repositories::db::with_db;

use super::support::normalize_optional_text;

#[derive(Debug, Clone)]
pub struct UpdateContactTaskRunnerConfigInput {
    pub enabled: bool,
    pub base_url: Option<String>,
    pub agent_account_id: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub clear_password: bool,
}

pub async fn list_contacts(
    user_id: &str,
    status: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<ChatosContact>, String> {
    with_db(
        |db| {
            let user_id = user_id.to_string();
            let status = normalize_optional_text(status);
            Box::pin(async move {
                let mut filter = doc! { "user_id": &user_id };
                if let Some(status) = status.as_deref() {
                    filter.insert("status", status);
                }
                let options = FindOptions::builder()
                    .sort(doc! { "updated_at": -1, "created_at": -1 })
                    .limit(Some(limit.max(1).min(500)))
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
        },
        |pool| {
            let user_id = user_id.to_string();
            let status = normalize_optional_text(status);
            Box::pin(async move {
                let mut sql = "SELECT * FROM chatos_contacts WHERE user_id = ?".to_string();
                if status.is_some() {
                    sql.push_str(" AND status = ?");
                }
                sql.push_str(" ORDER BY updated_at DESC, created_at DESC LIMIT ? OFFSET ?");
                let mut query =
                    sqlx::query_as::<_, ChatosContactRow>(sqlx::AssertSqlSafe(sql)).bind(&user_id);
                if let Some(status) = status.as_deref() {
                    query = query.bind(status);
                }
                query = query.bind(limit.max(1).min(500)).bind(offset.max(0));
                let rows = query.fetch_all(pool).await.map_err(|e| e.to_string())?;
                Ok(rows.into_iter().map(ChatosContactRow::to_contact).collect())
            })
        },
    )
    .await
}

pub async fn get_contact_by_id(contact_id: &str) -> Result<Option<ChatosContact>, String> {
    with_db(
        |db| {
            let contact_id = contact_id.to_string();
            Box::pin(async move {
                db.collection::<ChatosContact>("chatos_contacts")
                    .find_one(doc! { "id": &contact_id }, None)
                    .await
                    .map_err(|e| e.to_string())
            })
        },
        |pool| {
            let contact_id = contact_id.to_string();
            Box::pin(async move {
                let row = sqlx::query_as::<_, ChatosContactRow>(
                    "SELECT * FROM chatos_contacts WHERE id = ?",
                )
                .bind(&contact_id)
                .fetch_optional(pool)
                .await
                .map_err(|e| e.to_string())?;
                Ok(row.map(ChatosContactRow::to_contact))
            })
        },
    )
    .await
}

pub async fn get_contact_by_user_and_agent(
    user_id: &str,
    agent_id: &str,
) -> Result<Option<ChatosContact>, String> {
    with_db(
        |db| {
            let user_id = user_id.to_string();
            let agent_id = agent_id.to_string();
            Box::pin(async move {
                db.collection::<ChatosContact>("chatos_contacts")
                    .find_one(doc! { "user_id": &user_id, "agent_id": &agent_id }, None)
                    .await
                    .map_err(|e| e.to_string())
            })
        },
        |pool| {
            let user_id = user_id.to_string();
            let agent_id = agent_id.to_string();
            Box::pin(async move {
                let row = sqlx::query_as::<_, ChatosContactRow>(
                    "SELECT * FROM chatos_contacts WHERE user_id = ? AND agent_id = ?",
                )
                .bind(&user_id)
                .bind(&agent_id)
                .fetch_optional(pool)
                .await
                .map_err(|e| e.to_string())?;
                Ok(row.map(ChatosContactRow::to_contact))
            })
        },
    )
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

    with_db(
        |db| {
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
        },
        |pool| {
            let user_id = user_id.to_string();
            let status = normalize_optional_text(status);
            let ids = ids.clone();
            Box::pin(async move {
                let mut qb =
                    QueryBuilder::<Sqlite>::new("SELECT * FROM chatos_contacts WHERE user_id = ");
                qb.push_bind(&user_id);
                qb.push(" AND id IN (");
                {
                    let mut separated = qb.separated(", ");
                    for id in &ids {
                        separated.push_bind(id);
                    }
                }
                qb.push(")");
                if let Some(status) = status.as_deref() {
                    qb.push(" AND status = ");
                    qb.push_bind(status);
                }
                qb.push(" ORDER BY updated_at DESC, created_at DESC");
                let rows = qb
                    .build_query_as::<ChatosContactRow>()
                    .fetch_all(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(rows.into_iter().map(ChatosContactRow::to_contact).collect())
            })
        },
    )
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
    with_db(
        |db| {
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
        },
        |pool| {
            let contact = contact.clone();
            Box::pin(async move {
                match sqlx::query(
                    "INSERT INTO chatos_contacts \
                    (id, user_id, agent_id, agent_name_snapshot, status, created_at, updated_at) \
                    VALUES (?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(&contact.id)
                .bind(&contact.user_id)
                .bind(&contact.agent_id)
                .bind(&contact.agent_name_snapshot)
                .bind(&contact.status)
                .bind(&contact.created_at)
                .bind(&contact.updated_at)
                .execute(pool)
                .await
                {
                    Ok(_) => Ok((contact, true)),
                    Err(err) => {
                        if err.to_string().to_lowercase().contains("unique") {
                            if let Some(existing) = sqlx::query_as::<_, ChatosContactRow>(
                                "SELECT * FROM chatos_contacts WHERE user_id = ? AND agent_id = ?",
                            )
                            .bind(&contact.user_id)
                            .bind(&contact.agent_id)
                            .fetch_optional(pool)
                            .await
                            .map_err(|e| e.to_string())?
                            {
                                return Ok((existing.to_contact(), false));
                            }
                        }
                        Err(err.to_string())
                    }
                }
            })
        },
    )
    .await
}

pub async fn update_contact_task_runner_config(
    contact_id: &str,
    input: UpdateContactTaskRunnerConfigInput,
) -> Result<Option<ChatosContact>, String> {
    let Some(existing) = get_contact_by_id(contact_id).await? else {
        return Ok(None);
    };
    let updated_at = crate::core::time::now_rfc3339();
    let base_url = normalize_optional_text(input.base_url.as_deref());
    let agent_account_id = normalize_optional_text(input.agent_account_id.as_deref());
    let username = normalize_optional_text(input.username.as_deref());
    let password = match normalize_optional_text(input.password.as_deref()) {
        Some(value) => Some(value),
        None if input.clear_password => None,
        None => existing.task_runner_password.clone(),
    };
    let enabled = input.enabled
        && base_url.is_some()
        && (agent_account_id.is_some() || (username.is_some() && password.is_some()));

    with_db(
        |db| {
            let contact_id = contact_id.to_string();
            let base_url = base_url.clone();
            let agent_account_id = agent_account_id.clone();
            let username = username.clone();
            let password = password.clone();
            let updated_at = updated_at.clone();
            Box::pin(async move {
                let mut set_doc = doc! {
                    "task_runner_enabled": enabled,
                    "updated_at": &updated_at,
                };
                set_doc.insert("task_runner_base_url", optional_string_bson(base_url.clone()));
                set_doc.insert(
                    "task_runner_agent_account_id",
                    optional_string_bson(agent_account_id.clone()),
                );
                set_doc.insert("task_runner_username", optional_string_bson(username.clone()));
                set_doc.insert("task_runner_password", optional_string_bson(password.clone()));
                let result = db
                    .collection::<Document>("chatos_contacts")
                    .update_one(doc! { "id": &contact_id }, doc! { "$set": set_doc }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                if result.matched_count == 0 {
                    return Ok(None);
                }
                db.collection::<ChatosContact>("chatos_contacts")
                    .find_one(doc! { "id": &contact_id }, None)
                    .await
                    .map_err(|e| e.to_string())
            })
        },
        |pool| {
            let contact_id = contact_id.to_string();
            let base_url = base_url.clone();
            let agent_account_id = agent_account_id.clone();
            let username = username.clone();
            let password = password.clone();
            let updated_at = updated_at.clone();
            Box::pin(async move {
                let result = sqlx::query(
                    "UPDATE chatos_contacts SET \
                    task_runner_enabled = ?, task_runner_base_url = ?, task_runner_agent_account_id = ?, task_runner_username = ?, task_runner_password = ?, updated_at = ? \
                    WHERE id = ?",
                )
                .bind(if enabled { 1_i64 } else { 0_i64 })
                .bind(&base_url)
                .bind(&agent_account_id)
                .bind(&username)
                .bind(&password)
                .bind(&updated_at)
                .bind(&contact_id)
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;
                if result.rows_affected() == 0 {
                    return Ok(None);
                }
                let row =
                    sqlx::query_as::<_, ChatosContactRow>("SELECT * FROM chatos_contacts WHERE id = ?")
                        .bind(&contact_id)
                        .fetch_optional(pool)
                        .await
                        .map_err(|e| e.to_string())?;
                Ok(row.map(ChatosContactRow::to_contact))
            })
        },
    )
    .await
}

pub async fn delete_contact_by_id(contact_id: &str) -> Result<bool, String> {
    with_db(
        |db| {
            let contact_id = contact_id.to_string();
            Box::pin(async move {
                let result = db
                    .collection::<Document>("chatos_contacts")
                    .delete_one(doc! { "id": &contact_id }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(result.deleted_count > 0)
            })
        },
        |pool| {
            let contact_id = contact_id.to_string();
            Box::pin(async move {
                let result = sqlx::query("DELETE FROM chatos_contacts WHERE id = ?")
                    .bind(&contact_id)
                    .execute(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(result.rows_affected() > 0)
            })
        },
    )
    .await
}
