use futures_util::TryStreamExt;
use mongodb::bson::doc;
use mongodb::options::{FindOneOptions, FindOptions};
use std::collections::HashSet;

use crate::db::Db;
use crate::models::Session;

use super::super::session_support::{
    agent_lookup_conditions, build_contact_or_conditions, insert_project_scope_filter,
    normalize_project_scope, project_scope_condition,
};
use super::{collection, normalize_optional_text};

pub(super) async fn find_active_session_by_contact_project(
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
    and_conditions.push(project_scope_condition(project_id));
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
        insert_project_scope_filter(&mut filter, v.trim());
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
        "$or": agent_lookup_conditions(agent_id),
    };
    if let Some(v) = status {
        filter.insert("status", v);
    }

    if let Some(project_id) = normalize_optional_text(project_id) {
        insert_project_scope_filter(&mut filter, project_id.as_str());
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

pub async fn get_session_by_id(db: &Db, session_id: &str) -> Result<Option<Session>, String> {
    collection(db)
        .find_one(doc! {"id": session_id})
        .await
        .map_err(|e| e.to_string())
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
