use futures::TryStreamExt;
use mongodb::bson::{Bson, Document, doc};
use mongodb::options::{FindOneOptions, FindOptions, UpdateOptions};
use sqlx::{QueryBuilder, Sqlite};

use crate::core::values::optional_string_bson;
use crate::models::memory_mapping::{
    ChatosContact, ChatosContactRow, ChatosMemoryProject, ChatosMemoryProjectRow,
    ChatosProjectAgentLink, ChatosProjectAgentLinkRow,
};
use crate::repositories::db::with_db;

fn normalize_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

#[derive(Debug, Clone)]
pub struct UpdateContactTaskRunnerConfigInput {
    pub enabled: bool,
    pub base_url: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub clear_password: bool,
}

pub fn default_project_name(project_id: &str) -> String {
    if project_id.trim() == "0" {
        "未指定项目".to_string()
    } else {
        format!("项目 {}", project_id.trim())
    }
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
                let mut query = sqlx::query_as::<_, ChatosContactRow>(&sql).bind(&user_id);
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

pub async fn update_contact_agent(
    contact_id: &str,
    agent_id: &str,
    agent_name_snapshot: Option<String>,
) -> Result<Option<ChatosContact>, String> {
    let updated_at = crate::core::time::now_rfc3339();
    with_db(
        |db| {
            let contact_id = contact_id.to_string();
            let agent_id = agent_id.to_string();
            let agent_name_snapshot = agent_name_snapshot.clone();
            let updated_at = updated_at.clone();
            Box::pin(async move {
                let mut set_doc = doc! {
                    "agent_id": &agent_id,
                    "updated_at": &updated_at,
                };
                match agent_name_snapshot {
                    Some(value) => {
                        set_doc.insert("agent_name_snapshot", value);
                    }
                    None => {
                        set_doc.insert("agent_name_snapshot", Bson::Null);
                    }
                }
                let result = db
                    .collection::<Document>("chatos_contacts")
                    .update_one(
                        doc! { "id": &contact_id },
                        doc! { "$set": set_doc },
                        None,
                    )
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
            let agent_id = agent_id.to_string();
            let agent_name_snapshot = agent_name_snapshot.clone();
            let updated_at = updated_at.clone();
            Box::pin(async move {
                let result = sqlx::query(
                    "UPDATE chatos_contacts SET agent_id = ?, agent_name_snapshot = ?, updated_at = ? WHERE id = ?",
                )
                .bind(&agent_id)
                .bind(&agent_name_snapshot)
                .bind(&updated_at)
                .bind(&contact_id)
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;
                if result.rows_affected() == 0 {
                    return Ok(None);
                }
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

pub async fn update_contact_task_runner_config(
    contact_id: &str,
    input: UpdateContactTaskRunnerConfigInput,
) -> Result<Option<ChatosContact>, String> {
    let Some(existing) = get_contact_by_id(contact_id).await? else {
        return Ok(None);
    };
    let updated_at = crate::core::time::now_rfc3339();
    let base_url = normalize_optional_text(input.base_url.as_deref());
    let username = normalize_optional_text(input.username.as_deref());
    let password = match normalize_optional_text(input.password.as_deref()) {
        Some(value) => Some(value),
        None if input.clear_password => None,
        None => existing.task_runner_password.clone(),
    };
    let enabled = input.enabled && base_url.is_some() && username.is_some() && password.is_some();

    with_db(
        |db| {
            let contact_id = contact_id.to_string();
            let base_url = base_url.clone();
            let username = username.clone();
            let password = password.clone();
            let updated_at = updated_at.clone();
            Box::pin(async move {
                let mut set_doc = doc! {
                    "task_runner_enabled": enabled,
                    "updated_at": &updated_at,
                };
                set_doc.insert(
                    "task_runner_base_url",
                    optional_string_bson(base_url.clone()),
                );
                set_doc.insert(
                    "task_runner_username",
                    optional_string_bson(username.clone()),
                );
                set_doc.insert(
                    "task_runner_password",
                    optional_string_bson(password.clone()),
                );
                let result = db
                    .collection::<Document>("chatos_contacts")
                    .update_one(
                        doc! { "id": &contact_id },
                        doc! { "$set": set_doc },
                        None,
                    )
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
            let username = username.clone();
            let password = password.clone();
            let updated_at = updated_at.clone();
            Box::pin(async move {
                let result = sqlx::query(
                    "UPDATE chatos_contacts SET \
                    task_runner_enabled = ?, task_runner_base_url = ?, task_runner_username = ?, task_runner_password = ?, updated_at = ? \
                    WHERE id = ?",
                )
                .bind(if enabled { 1_i64 } else { 0_i64 })
                .bind(&base_url)
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

#[derive(Debug, Clone)]
pub struct UpsertMemoryProjectInput {
    pub user_id: String,
    pub project_id: String,
    pub name: String,
    pub root_path: Option<String>,
    pub description: Option<String>,
    pub status: Option<String>,
    pub is_virtual: Option<i64>,
}

pub async fn get_project_by_user_and_project_id(
    user_id: &str,
    project_id: &str,
) -> Result<Option<ChatosMemoryProject>, String> {
    with_db(
        |db| {
            let user_id = user_id.to_string();
            let project_id = project_id.to_string();
            Box::pin(async move {
                db.collection::<ChatosMemoryProject>("chatos_memory_projects")
                    .find_one(
                        doc! { "user_id": &user_id, "project_id": &project_id },
                        None,
                    )
                    .await
                    .map_err(|e| e.to_string())
            })
        },
        |pool| {
            let user_id = user_id.to_string();
            let project_id = project_id.to_string();
            Box::pin(async move {
                let row = sqlx::query_as::<_, ChatosMemoryProjectRow>(
                    "SELECT * FROM chatos_memory_projects WHERE user_id = ? AND project_id = ?",
                )
                .bind(&user_id)
                .bind(&project_id)
                .fetch_optional(pool)
                .await
                .map_err(|e| e.to_string())?;
                Ok(row.map(ChatosMemoryProjectRow::to_project))
            })
        },
    )
    .await
}

pub async fn upsert_memory_project(
    input: UpsertMemoryProjectInput,
) -> Result<Option<ChatosMemoryProject>, String> {
    let now = crate::core::time::now_rfc3339();
    let project_id =
        normalize_optional_text(Some(input.project_id.as_str())).unwrap_or_else(|| "0".to_string());
    let status =
        normalize_optional_text(input.status.as_deref()).unwrap_or_else(|| "active".to_string());
    let archived_at = if status == "archived" || status == "deleted" {
        Some(now.clone())
    } else {
        None
    };

    with_db(
        |db| {
            let input = input.clone();
            let now = now.clone();
            let project_id = project_id.clone();
            let status = status.clone();
            let archived_at = archived_at.clone();
            Box::pin(async move {
                let filter = doc! {
                    "user_id": &input.user_id,
                    "project_id": &project_id,
                };
                let mut set_doc = doc! {
                    "user_id": &input.user_id,
                    "project_id": &project_id,
                    "name": &input.name,
                    "status": &status,
                    "is_virtual": input.is_virtual.unwrap_or(0).max(0),
                    "updated_at": &now,
                };
                set_doc.insert(
                    "root_path",
                    crate::core::values::optional_string_bson(input.root_path.clone()),
                );
                set_doc.insert(
                    "description",
                    crate::core::values::optional_string_bson(input.description.clone()),
                );
                match archived_at.clone() {
                    Some(value) => set_doc.insert("archived_at", value),
                    None => set_doc.insert("archived_at", Bson::Null),
                };
                let update_options = UpdateOptions::builder().upsert(true).build();
                db.collection::<Document>("chatos_memory_projects")
                    .update_one(
                        filter.clone(),
                        doc! {
                            "$set": set_doc,
                            "$setOnInsert": {
                                "id": uuid::Uuid::new_v4().to_string(),
                                "created_at": &now,
                            }
                        },
                        update_options,
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                db.collection::<ChatosMemoryProject>("chatos_memory_projects")
                    .find_one(filter, None)
                    .await
                    .map_err(|e| e.to_string())
            })
        },
        |pool| {
            let input = input.clone();
            let now = now.clone();
            let project_id = project_id.clone();
            let status = status.clone();
            let archived_at = archived_at.clone();
            Box::pin(async move {
                let existing = sqlx::query_as::<_, ChatosMemoryProjectRow>(
                    "SELECT * FROM chatos_memory_projects WHERE user_id = ? AND project_id = ?",
                )
                .bind(&input.user_id)
                .bind(&project_id)
                .fetch_optional(pool)
                .await
                .map_err(|e| e.to_string())?;

                if let Some(row) = existing {
                    sqlx::query(
                        "UPDATE chatos_memory_projects SET \
                        name = ?, root_path = ?, description = ?, status = ?, is_virtual = ?, updated_at = ?, archived_at = ? \
                        WHERE id = ?",
                    )
                    .bind(&input.name)
                    .bind(&input.root_path)
                    .bind(&input.description)
                    .bind(&status)
                    .bind(input.is_virtual.unwrap_or(0).max(0))
                    .bind(&now)
                    .bind(&archived_at)
                    .bind(&row.id)
                    .execute(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                } else {
                    sqlx::query(
                        "INSERT INTO chatos_memory_projects \
                        (id, user_id, project_id, name, root_path, description, status, is_virtual, created_at, updated_at, archived_at) \
                        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    )
                    .bind(uuid::Uuid::new_v4().to_string())
                    .bind(&input.user_id)
                    .bind(&project_id)
                    .bind(&input.name)
                    .bind(&input.root_path)
                    .bind(&input.description)
                    .bind(&status)
                    .bind(input.is_virtual.unwrap_or(0).max(0))
                    .bind(&now)
                    .bind(&now)
                    .bind(&archived_at)
                    .execute(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                }

                let row = sqlx::query_as::<_, ChatosMemoryProjectRow>(
                    "SELECT * FROM chatos_memory_projects WHERE user_id = ? AND project_id = ?",
                )
                .bind(&input.user_id)
                .bind(&project_id)
                .fetch_optional(pool)
                .await
                .map_err(|e| e.to_string())?;
                Ok(row.map(ChatosMemoryProjectRow::to_project))
            })
        },
    )
    .await
}

pub async fn list_projects_by_ids(
    user_id: &str,
    project_ids: &[String],
) -> Result<Vec<ChatosMemoryProject>, String> {
    let ids = project_ids
        .iter()
        .filter_map(|value| normalize_optional_text(Some(value.as_str())))
        .collect::<Vec<_>>();
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    with_db(
        |db| {
            let user_id = user_id.to_string();
            let ids = ids.clone();
            Box::pin(async move {
                let cursor = db
                    .collection::<ChatosMemoryProject>("chatos_memory_projects")
                    .find(
                        doc! {
                            "user_id": &user_id,
                            "project_id": { "$in": ids },
                        },
                        None,
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                cursor
                    .try_collect::<Vec<ChatosMemoryProject>>()
                    .await
                    .map_err(|e| e.to_string())
            })
        },
        |pool| {
            let user_id = user_id.to_string();
            let ids = ids.clone();
            Box::pin(async move {
                let mut qb = QueryBuilder::<Sqlite>::new(
                    "SELECT * FROM chatos_memory_projects WHERE user_id = ",
                );
                qb.push_bind(&user_id);
                qb.push(" AND project_id IN (");
                {
                    let mut separated = qb.separated(", ");
                    for id in &ids {
                        separated.push_bind(id);
                    }
                }
                qb.push(")");
                let rows = qb
                    .build_query_as::<ChatosMemoryProjectRow>()
                    .fetch_all(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(rows
                    .into_iter()
                    .map(ChatosMemoryProjectRow::to_project)
                    .collect())
            })
        },
    )
    .await
}

pub async fn list_memory_projects(
    user_id: &str,
    status: Option<&str>,
    include_virtual: bool,
    limit: i64,
    offset: i64,
) -> Result<Vec<ChatosMemoryProject>, String> {
    with_db(
        |db| {
            let user_id = user_id.to_string();
            let status = normalize_optional_text(status);
            Box::pin(async move {
                let mut filter = doc! { "user_id": &user_id };
                if let Some(status) = status.as_deref() {
                    filter.insert("status", status);
                }
                if !include_virtual {
                    filter.insert("is_virtual", 0);
                }
                let options = FindOptions::builder()
                    .sort(doc! { "updated_at": -1, "created_at": -1 })
                    .limit(Some(limit.max(1).min(500)))
                    .skip(Some(offset.max(0) as u64))
                    .build();
                let cursor = db
                    .collection::<ChatosMemoryProject>("chatos_memory_projects")
                    .find(filter, options)
                    .await
                    .map_err(|e| e.to_string())?;
                cursor
                    .try_collect::<Vec<ChatosMemoryProject>>()
                    .await
                    .map_err(|e| e.to_string())
            })
        },
        |pool| {
            let user_id = user_id.to_string();
            let status = normalize_optional_text(status);
            Box::pin(async move {
                let mut sql = "SELECT * FROM chatos_memory_projects WHERE user_id = ?".to_string();
                if status.is_some() {
                    sql.push_str(" AND status = ?");
                }
                if !include_virtual {
                    sql.push_str(" AND is_virtual = 0");
                }
                sql.push_str(" ORDER BY updated_at DESC, created_at DESC LIMIT ? OFFSET ?");
                let mut query = sqlx::query_as::<_, ChatosMemoryProjectRow>(&sql).bind(&user_id);
                if let Some(status) = status.as_deref() {
                    query = query.bind(status);
                }
                query = query.bind(limit.max(1).min(500)).bind(offset.max(0));
                let rows = query.fetch_all(pool).await.map_err(|e| e.to_string())?;
                Ok(rows
                    .into_iter()
                    .map(ChatosMemoryProjectRow::to_project)
                    .collect())
            })
        },
    )
    .await
}

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
