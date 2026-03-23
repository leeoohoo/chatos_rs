use futures_util::TryStreamExt;
use mongodb::bson::{doc, Bson};
use mongodb::options::FindOptions;
use uuid::Uuid;

use crate::db::Db;
use crate::models::MemoryProject;

use super::{default_active_status, normalize_optional_text, now_rfc3339};

fn collection(db: &Db) -> mongodb::Collection<MemoryProject> {
    db.collection::<MemoryProject>("memory_projects")
}

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
    db: &Db,
    user_id: &str,
    project_id: &str,
) -> Result<Option<MemoryProject>, String> {
    collection(db)
        .find_one(doc! {
            "user_id": user_id,
            "project_id": project_id,
        })
        .await
        .map_err(|e| e.to_string())
}

pub async fn upsert_project(
    db: &Db,
    input: UpsertMemoryProjectInput,
) -> Result<Option<MemoryProject>, String> {
    let now = now_rfc3339();
    let project_id =
        normalize_optional_text(Some(input.project_id.as_str())).unwrap_or_else(|| "0".to_string());
    let status =
        normalize_optional_text(input.status.as_deref()).unwrap_or_else(default_active_status);
    let archived_at = if status == "archived" || status == "deleted" {
        Some(now.clone())
    } else {
        None
    };

    let filter = doc! {
        "user_id": input.user_id.as_str(),
        "project_id": project_id.as_str(),
    };

    let mut set_doc = doc! {
        "user_id": input.user_id.as_str(),
        "project_id": project_id.as_str(),
        "name": input.name.as_str(),
        "status": status.as_str(),
        "is_virtual": input.is_virtual.unwrap_or(0).max(0),
        "updated_at": now.as_str(),
    };
    if let Some(root_path) = normalize_optional_text(input.root_path.as_deref()) {
        set_doc.insert("root_path", root_path);
    } else {
        set_doc.insert("root_path", Bson::Null);
    }
    if let Some(description) = normalize_optional_text(input.description.as_deref()) {
        set_doc.insert("description", description);
    } else {
        set_doc.insert("description", Bson::Null);
    }
    match archived_at {
        Some(value) => set_doc.insert("archived_at", value),
        None => set_doc.insert("archived_at", Bson::Null),
    };

    collection(db)
        .update_one(
            filter.clone(),
            doc! {
                "$set": set_doc,
                "$setOnInsert": {
                    "id": Uuid::new_v4().to_string(),
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

pub async fn list_projects(
    db: &Db,
    user_id: &str,
    status: Option<&str>,
    include_virtual: bool,
    limit: i64,
    offset: i64,
) -> Result<Vec<MemoryProject>, String> {
    let mut filter = doc! {
        "user_id": user_id,
    };
    if let Some(status) = normalize_optional_text(status) {
        filter.insert("status", status);
    }
    if !include_virtual {
        filter.insert("is_virtual", doc! {"$ne": 1});
    }

    let options = FindOptions::builder()
        .sort(doc! {"updated_at": -1})
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

pub async fn list_projects_by_ids(
    db: &Db,
    user_id: &str,
    project_ids: &[String],
) -> Result<Vec<MemoryProject>, String> {
    if project_ids.is_empty() {
        return Ok(Vec::new());
    }
    let sanitized: Vec<String> = project_ids
        .iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect();
    if sanitized.is_empty() {
        return Ok(Vec::new());
    }
    let cursor = collection(db)
        .find(doc! {
            "user_id": user_id,
            "project_id": {"$in": sanitized},
        })
        .await
        .map_err(|e| e.to_string())?;
    cursor.try_collect().await.map_err(|e| e.to_string())
}
