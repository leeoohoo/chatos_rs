use futures_util::TryStreamExt;
use mongodb::bson::{doc, Bson};
use mongodb::options::{FindOneOptions, FindOptions};
use std::collections::HashSet;
use uuid::Uuid;

use crate::db::Db;
use crate::models::{CreateSessionRequest, Session, UpdateSessionRequest};

use super::now_rfc3339;

fn collection(db: &Db) -> mongodb::Collection<Session> {
    db.collection::<Session>("sessions")
}

fn normalize_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|v| v.to_string())
}

fn normalize_project_scope(project_id: Option<String>) -> String {
    normalize_optional_text(project_id.as_deref())
        .unwrap_or_else(|| "0".to_string())
}

fn metadata_text(metadata: Option<&serde_json::Value>, path: &[&str]) -> Option<String> {
    let mut cursor = metadata?;
    for key in path {
        cursor = cursor.get(*key)?;
    }
    normalize_optional_text(cursor.as_str())
}

fn contact_id_from_metadata(metadata: Option<&serde_json::Value>) -> Option<String> {
    metadata_text(metadata, &["contact", "contact_id"])
        .or_else(|| metadata_text(metadata, &["ui_contact", "contact_id"]))
}

fn agent_id_from_metadata(metadata: Option<&serde_json::Value>) -> Option<String> {
    metadata_text(metadata, &["contact", "agent_id"])
        .or_else(|| metadata_text(metadata, &["ui_contact", "agent_id"]))
        .or_else(|| metadata_text(metadata, &["ui_chat_selection", "selected_agent_id"]))
}

fn set_metadata_text(metadata: &mut serde_json::Value, scope: &str, key: &str, value: &str) {
    let Some(root) = metadata.as_object_mut() else {
        return;
    };
    let entry = root
        .entry(scope.to_string())
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    if !entry.is_object() {
        *entry = serde_json::Value::Object(serde_json::Map::new());
    }
    if let Some(map) = entry.as_object_mut() {
        map.insert(
            key.to_string(),
            serde_json::Value::String(value.to_string()),
        );
    }
}

fn normalize_session_metadata(metadata: Option<serde_json::Value>) -> Option<serde_json::Value> {
    let contact_id = contact_id_from_metadata(metadata.as_ref());
    let agent_id = agent_id_from_metadata(metadata.as_ref());

    if contact_id.is_none() && agent_id.is_none() {
        return metadata;
    }

    let mut normalized = match metadata {
        Some(serde_json::Value::Object(map)) => serde_json::Value::Object(map),
        Some(_) | None => serde_json::Value::Object(serde_json::Map::new()),
    };

    if let Some(contact_id) = contact_id.as_deref() {
        set_metadata_text(&mut normalized, "contact", "contact_id", contact_id);
        set_metadata_text(&mut normalized, "ui_contact", "contact_id", contact_id);
    }
    if let Some(agent_id) = agent_id.as_deref() {
        set_metadata_text(&mut normalized, "contact", "agent_id", agent_id);
        set_metadata_text(&mut normalized, "ui_contact", "agent_id", agent_id);
        set_metadata_text(
            &mut normalized,
            "ui_chat_selection",
            "selected_agent_id",
            agent_id,
        );
    }

    Some(normalized)
}

fn is_duplicate_key_error(err: &mongodb::error::Error) -> bool {
    let text = err.to_string().to_ascii_lowercase();
    text.contains("e11000") || text.contains("duplicate key")
}

fn build_contact_or_conditions(contact_id: Option<&str>, agent_id: Option<&str>) -> Vec<mongodb::bson::Document> {
    let mut out = Vec::new();
    if let Some(contact_id) = normalize_optional_text(contact_id) {
        out.push(doc! {"metadata.contact.contact_id": contact_id.clone()});
        out.push(doc! {"metadata.ui_contact.contact_id": contact_id});
    }
    if let Some(agent_id) = normalize_optional_text(agent_id) {
        out.push(doc! {"metadata.contact.agent_id": agent_id.clone()});
        out.push(doc! {"metadata.ui_contact.agent_id": agent_id.clone()});
        out.push(doc! {"metadata.ui_chat_selection.selected_agent_id": agent_id});
    }
    out
}

async fn find_active_session_by_contact_project(
    db: &Db,
    user_id: &str,
    project_id: &str,
    contact_id: Option<&str>,
    agent_id: Option<&str>,
) -> Result<Option<Session>, String> {
    let conditions = build_contact_or_conditions(contact_id, agent_id);
    if conditions.is_empty() {
        return Ok(None);
    }

    let mut filter = doc! {
        "user_id": user_id,
        "status": "active",
    };
    let mut and_conditions: Vec<mongodb::bson::Document> = Vec::new();
    and_conditions.push(doc! {"$or": conditions});
    if project_id == "0" {
        and_conditions.push(doc! {
            "$or": [
                {"project_id": "0"},
                {"project_id": Bson::Null},
                {"project_id": ""},
                {"project_id": {"$exists": false}}
            ]
        });
    } else {
        and_conditions.push(doc! {"project_id": project_id});
    }
    filter.insert("$and", and_conditions);

    let options = FindOneOptions::builder()
        .sort(doc! {"updated_at": -1, "created_at": -1})
        .build();
    collection(db)
        .find_one(filter)
        .with_options(options)
        .await
        .map_err(|e| e.to_string())
}

pub async fn get_active_session_by_contact_project(
    db: &Db,
    user_id: &str,
    project_id: &str,
    contact_id: Option<&str>,
    agent_id: Option<&str>,
) -> Result<Option<Session>, String> {
    find_active_session_by_contact_project(db, user_id, project_id, contact_id, agent_id).await
}

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
        if v.trim() == "0" {
            filter.insert(
                "$and",
                vec![doc! {
                    "$or": [
                        {"project_id": "0"},
                        {"project_id": Bson::Null},
                        {"project_id": ""},
                        {"project_id": {"$exists": false}}
                    ]
                }],
            );
        } else {
            filter.insert("project_id", v);
        }
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

pub async fn list_sessions_by_agent(
    db: &Db,
    user_id: &str,
    agent_id: &str,
    project_id: Option<&str>,
    status: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<Session>, String> {
    let limit = limit.max(1).min(200) as u64;
    let offset = offset.max(0) as u64;

    let mut filter = doc! {
        "user_id": user_id,
        "$or": vec![
            doc! {"metadata.contact.agent_id": agent_id},
            doc! {"metadata.ui_contact.agent_id": agent_id},
            doc! {"metadata.ui_chat_selection.selected_agent_id": agent_id},
        ],
    };
    if let Some(v) = status {
        filter.insert("status", v);
    }

    if let Some(project_id) = normalize_optional_text(project_id) {
        if project_id == "0" {
            filter.insert(
                "$and",
                vec![doc! {
                    "$or": [
                        {"project_id": "0"},
                        {"project_id": Bson::Null},
                        {"project_id": ""},
                        {"project_id": {"$exists": false}}
                    ]
                }],
            );
        } else {
            filter.insert("project_id", project_id);
        }
    }

    let options = FindOptions::builder()
        .sort(doc! {"updated_at": -1, "created_at": -1})
        .limit(Some(limit as i64))
        .skip(Some(offset))
        .build();

    let cursor = collection(db)
        .find(filter)
        .with_options(options)
        .await
        .map_err(|e| e.to_string())?;
    let rows: Vec<Session> = cursor.try_collect().await.map_err(|e| e.to_string())?;
    let mut seen_projects = HashSet::new();
    let mut deduped = Vec::with_capacity(rows.len());
    for session in rows {
        let project_id = normalize_project_scope(session.project_id.clone());
        if seen_projects.insert(project_id) {
            deduped.push(session);
        }
    }
    Ok(deduped)
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

pub async fn list_active_user_ids(db: &Db, limit: i64) -> Result<Vec<String>, String> {
    let contact_match = doc! {
        "$or": [
            {"metadata.contact.contact_id": {"$exists": true, "$type": "string"}},
            {"metadata.ui_contact.contact_id": {"$exists": true, "$type": "string"}},
            {"metadata.contact.agent_id": {"$exists": true, "$type": "string"}},
            {"metadata.ui_contact.agent_id": {"$exists": true, "$type": "string"}},
            {"metadata.ui_chat_selection.selected_agent_id": {"$exists": true, "$type": "string"}}
        ]
    };
    let pipeline = vec![
        doc! {"$match": {"status": "active"}},
        doc! {"$match": contact_match},
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
    let docs: Vec<mongodb::bson::Document> =
        cursor.try_collect().await.map_err(|e| e.to_string())?;
    Ok(docs
        .into_iter()
        .filter_map(|doc| doc.get_str("user_id").ok().map(|v| v.to_string()))
        .collect())
}
