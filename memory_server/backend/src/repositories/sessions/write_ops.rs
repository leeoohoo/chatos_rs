use mongodb::bson::{doc, Bson};
use uuid::Uuid;

use crate::db::Db;
use crate::models::{CreateSessionRequest, Session, UpdateSessionRequest};

use super::super::session_support::{
    agent_id_from_metadata, contact_id_from_metadata, is_duplicate_key_error,
    normalize_project_scope, normalize_session_metadata,
};
use super::read_ops::{find_active_session_by_contact_project, get_session_by_id};
use super::{collection, now_rfc3339};

pub async fn create_session(db: &Db, req: CreateSessionRequest) -> Result<Session, String> {
    let normalized_project_id = normalize_project_scope(req.project_id.clone());
    let user_id = req.user_id;
    let title = req.title;
    let metadata = normalize_session_metadata(req.metadata.clone());
    let contact_id = contact_id_from_metadata(metadata.as_ref());
    let agent_id = agent_id_from_metadata(metadata.as_ref());
    if let Some(existing) = find_active_session_by_contact_project(
        db,
        user_id.as_str(),
        normalized_project_id.as_str(),
        contact_id.as_deref(),
        agent_id.as_deref(),
    )
    .await?
    {
        return Ok(existing);
    }

    let now = now_rfc3339();
    let session = Session {
        id: Uuid::new_v4().to_string(),
        user_id: user_id.clone(),
        project_id: Some(normalized_project_id.clone()),
        title,
        metadata,
        status: "active".to_string(),
        created_at: now.clone(),
        updated_at: now,
        archived_at: None,
    };

    if let Err(err) = collection(db).insert_one(session.clone()).await {
        if is_duplicate_key_error(&err) {
            if let Some(existing) = find_active_session_by_contact_project(
                db,
                user_id.as_str(),
                normalized_project_id.as_str(),
                contact_id.as_deref(),
                agent_id.as_deref(),
            )
            .await?
            {
                return Ok(existing);
            }
        }
        return Err(err.to_string());
    }

    Ok(session)
}

#[allow(clippy::too_many_arguments)]
pub async fn upsert_session_sync(
    db: &Db,
    session_id: &str,
    user_id: &str,
    project_id: Option<String>,
    title: Option<String>,
    metadata: Option<serde_json::Value>,
    status: Option<String>,
    created_at: Option<String>,
    updated_at: Option<String>,
) -> Result<Session, String> {
    let now = now_rfc3339();
    let normalized_project_id = normalize_project_scope(project_id.clone());
    let metadata = normalize_session_metadata(metadata);
    let contact_id = contact_id_from_metadata(metadata.as_ref());
    let agent_id = agent_id_from_metadata(metadata.as_ref());
    let created_at = created_at.unwrap_or_else(|| now.clone());
    let updated_at = updated_at.unwrap_or_else(|| now.clone());
    let title = title.unwrap_or_else(|| "Untitled".to_string());
    let status = status.unwrap_or_else(|| "active".to_string());
    let metadata_bson = metadata
        .as_ref()
        .and_then(|value| mongodb::bson::to_bson(value).ok())
        .unwrap_or(Bson::Null);
    let archived_at = if status == "archived" {
        Some(updated_at.clone())
    } else {
        None
    };

    if status == "active" {
        if let Some(existing) = find_active_session_by_contact_project(
            db,
            user_id,
            normalized_project_id.as_str(),
            contact_id.as_deref(),
            agent_id.as_deref(),
        )
        .await?
        {
            collection(db)
                .update_one(
                    doc! {"id": existing.id.as_str()},
                    doc! {
                        "$set": {
                            "project_id": normalized_project_id.clone(),
                            "title": title.clone(),
                            "metadata": metadata_bson.clone(),
                            "status": &status,
                            "updated_at": &updated_at,
                            "archived_at": archived_at.clone(),
                        },
                        "$setOnInsert": {
                            "created_at": created_at.clone(),
                        }
                    },
                )
                .await
                .map_err(|e| e.to_string())?;
            return get_session_by_id(db, existing.id.as_str())
                .await?
                .ok_or_else(|| "upserted session not found".to_string());
        }
    }

    collection(db)
        .update_one(
            doc! {"id": session_id},
            doc! {
                "$set": {
                    "user_id": user_id,
                    "project_id": normalized_project_id,
                    "title": title,
                    "metadata": metadata_bson,
                    "status": &status,
                    "updated_at": &updated_at,
                    "archived_at": archived_at,
                },
                "$setOnInsert": {
                    "id": session_id,
                    "created_at": created_at,
                }
            },
        )
        .upsert(true)
        .await
        .map_err(|e| e.to_string())?;

    get_session_by_id(db, session_id)
        .await?
        .ok_or_else(|| "upserted session not found".to_string())
}

pub async fn delete_session(db: &Db, session_id: &str) -> Result<bool, String> {
    let now = now_rfc3339();
    let result = collection(db)
        .update_one(
            doc! {"id": session_id, "status": {"$ne": "archived"}},
            doc! {
                "$set": {
                    "status": "archived",
                    "archived_at": &now,
                    "updated_at": &now,
                }
            },
        )
        .await
        .map_err(|e| e.to_string())?;

    Ok(result.matched_count > 0)
}

pub async fn update_session(
    db: &Db,
    session_id: &str,
    req: UpdateSessionRequest,
) -> Result<Option<Session>, String> {
    let current = get_session_by_id(db, session_id).await?;
    let Some(current) = current else {
        return Ok(None);
    };

    let now = now_rfc3339();
    let title = req.title.or(current.title);
    let metadata = normalize_session_metadata(req.metadata.or(current.metadata));
    let metadata_bson = metadata
        .as_ref()
        .and_then(|value| mongodb::bson::to_bson(value).ok())
        .unwrap_or(Bson::Null);
    let status = req.status.unwrap_or(current.status);
    let archived_at = if status == "archived" {
        Some(now.clone())
    } else {
        current.archived_at
    };

    collection(db)
        .update_one(
            doc! {"id": session_id},
            doc! {
                "$set": {
                    "title": title,
                    "metadata": metadata_bson,
                    "status": &status,
                    "archived_at": archived_at,
                    "updated_at": &now,
                }
            },
        )
        .await
        .map_err(|e| e.to_string())?;

    get_session_by_id(db, session_id).await
}

pub async fn archive_sessions_by_contact(
    db: &Db,
    user_id: &str,
    contact_id: &str,
    agent_id: &str,
) -> Result<usize, String> {
    let now = now_rfc3339();
    let mut or_conditions = vec![];
    if !contact_id.trim().is_empty() {
        or_conditions.push(doc! {"metadata.contact.contact_id": contact_id});
        or_conditions.push(doc! {"metadata.ui_contact.contact_id": contact_id});
    }
    if !agent_id.trim().is_empty() {
        or_conditions.push(doc! {"metadata.contact.agent_id": agent_id});
        or_conditions.push(doc! {"metadata.ui_contact.agent_id": agent_id});
    }
    if or_conditions.is_empty() {
        return Ok(0);
    }

    let result = collection(db)
        .update_many(
            doc! {
                "user_id": user_id,
                "status": {"$ne": "archived"},
                "$or": or_conditions,
            },
            doc! {
                "$set": {
                    "status": "archived",
                    "archived_at": &now,
                    "updated_at": &now,
                }
            },
        )
        .await
        .map_err(|e| e.to_string())?;

    Ok(result.modified_count as usize)
}
