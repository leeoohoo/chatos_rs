use futures_util::TryStreamExt;
use mongodb::bson::doc;
use mongodb::options::FindOptions;
use uuid::Uuid;

use crate::db::Db;
use crate::models::{CreateSessionRequest, Session, UpdateSessionRequest};

use super::now_rfc3339;

fn collection(db: &Db) -> mongodb::Collection<Session> {
    db.collection::<Session>("sessions")
}

pub async fn create_session(db: &Db, req: CreateSessionRequest) -> Result<Session, String> {
    let now = now_rfc3339();
    let session = Session {
        id: Uuid::new_v4().to_string(),
        user_id: req.user_id,
        project_id: req.project_id,
        title: req.title,
        status: "active".to_string(),
        created_at: now.clone(),
        updated_at: now,
        archived_at: None,
    };

    collection(db)
        .insert_one(session.clone())
        .await
        .map_err(|e| e.to_string())?;

    Ok(session)
}

#[allow(clippy::too_many_arguments)]
pub async fn upsert_session_sync(
    db: &Db,
    session_id: &str,
    user_id: &str,
    project_id: Option<String>,
    title: Option<String>,
    status: Option<String>,
    created_at: Option<String>,
    updated_at: Option<String>,
) -> Result<Session, String> {
    let now = now_rfc3339();
    let created_at = created_at.unwrap_or_else(|| now.clone());
    let updated_at = updated_at.unwrap_or_else(|| now.clone());
    let title = title.unwrap_or_else(|| "Untitled".to_string());
    let status = status.unwrap_or_else(|| "active".to_string());
    let archived_at = if status == "archived" {
        Some(updated_at.clone())
    } else {
        None
    };

    collection(db)
        .update_one(
            doc! {"id": session_id},
            doc! {
                "$set": {
                    "user_id": user_id,
                    "project_id": project_id,
                    "title": title,
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

pub async fn list_sessions(
    db: &Db,
    user_id: Option<&str>,
    project_id: Option<&str>,
    status: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<Session>, String> {
    let limit = limit.max(1).min(200) as u64;
    let offset = offset.max(0) as u64;

    let mut filter = doc! {};
    if let Some(v) = user_id {
        filter.insert("user_id", v);
    }
    if let Some(v) = project_id {
        filter.insert("project_id", v);
    }
    if let Some(v) = status {
        filter.insert("status", v);
    }

    let options = FindOptions::builder()
        .sort(doc! {"created_at": -1})
        .limit(Some(limit as i64))
        .skip(Some(offset))
        .build();

    let cursor = collection(db)
        .find(filter)
        .with_options(options)
        .await
        .map_err(|e| e.to_string())?;
    cursor.try_collect().await.map_err(|e| e.to_string())
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

pub async fn get_session_by_id(db: &Db, session_id: &str) -> Result<Option<Session>, String> {
    collection(db)
        .find_one(doc! {"id": session_id})
        .await
        .map_err(|e| e.to_string())
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

pub async fn list_active_user_ids(db: &Db, limit: i64) -> Result<Vec<String>, String> {
    let pipeline = vec![
        doc! {"$match": {"status": "active"}},
        doc! {"$group": {"_id": "$user_id", "max_updated_at": {"$max": "$updated_at"}}},
        doc! {"$sort": {"max_updated_at": -1}},
        doc! {"$limit": limit.max(1).min(2000)},
        doc! {"$project": {"_id": 0, "user_id": "$_id"}},
    ];

    let cursor = db
        .collection::<mongodb::bson::Document>("sessions")
        .aggregate(pipeline)
        .await
        .map_err(|e| e.to_string())?;
    let docs: Vec<mongodb::bson::Document> = cursor.try_collect().await.map_err(|e| e.to_string())?;
    Ok(docs
        .into_iter()
        .filter_map(|doc| doc.get_str("user_id").ok().map(|v| v.to_string()))
        .collect())
}
