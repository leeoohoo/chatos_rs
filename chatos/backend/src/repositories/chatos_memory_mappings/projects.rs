// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use futures::TryStreamExt;
use mongodb::bson::{doc, Bson, Document};
use mongodb::options::{FindOptions, UpdateOptions};

use crate::models::memory_mapping::ChatosMemoryProject;
use crate::repositories::db::with_db;

use super::support::{normalize_optional_text, normalize_project_id};

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
    let project_id = normalize_project_id(project_id);
    with_db(|db| {
        let user_id = user_id.to_string();
        let project_id = project_id.clone();
        Box::pin(async move {
            db.collection::<ChatosMemoryProject>("chatos_memory_projects")
                .find_one(
                    doc! { "user_id": &user_id, "project_id": &project_id },
                    None,
                )
                .await
                .map_err(|e| e.to_string())
        })
    })
    .await
}

pub async fn upsert_memory_project(
    input: UpsertMemoryProjectInput,
) -> Result<Option<ChatosMemoryProject>, String> {
    let now = crate::core::time::now_rfc3339();
    let project_id = normalize_project_id(input.project_id.as_str());
    let status =
        normalize_optional_text(input.status.as_deref()).unwrap_or_else(|| "active".to_string());
    let archived_at = if status == "archived" || status == "deleted" {
        Some(now.clone())
    } else {
        None
    };

    with_db(|db| {
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
    })
    .await
}

pub async fn list_projects_by_ids(
    user_id: &str,
    project_ids: &[String],
) -> Result<Vec<ChatosMemoryProject>, String> {
    let ids = project_ids
        .iter()
        .filter_map(|value| normalize_optional_text(Some(value.as_str())))
        .map(|value| normalize_project_id(value.as_str()))
        .collect::<Vec<_>>();
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    with_db(|db| {
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
    })
    .await
}

pub async fn list_memory_projects(
    user_id: &str,
    status: Option<&str>,
    include_virtual: bool,
    limit: i64,
    offset: i64,
) -> Result<Vec<ChatosMemoryProject>, String> {
    with_db(|db| {
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
                .limit(Some(limit.clamp(1, 500)))
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
    })
    .await
}
