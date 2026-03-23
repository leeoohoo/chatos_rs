use futures_util::TryStreamExt;
use mongodb::bson::doc;
use mongodb::options::FindOptions;

use crate::db::Db;
use crate::models::{AgentRecall, ProjectMemory};

use super::{agent_recalls_collection, project_memories_collection};

pub async fn list_project_memories(
    db: &Db,
    user_id: &str,
    contact_id: &str,
    project_id: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<ProjectMemory>, String> {
    let options = FindOptions::builder()
        .sort(doc! {"updated_at": -1})
        .limit(Some(limit.max(1).min(500)))
        .skip(Some(offset.max(0) as u64))
        .build();

    let cursor = project_memories_collection(db)
        .find(doc! {
            "user_id": user_id,
            "contact_id": contact_id,
            "project_id": project_id,
        })
        .with_options(options)
        .await
        .map_err(|e| e.to_string())?;
    cursor.try_collect().await.map_err(|e| e.to_string())
}

pub async fn list_project_memories_by_contact(
    db: &Db,
    user_id: &str,
    contact_id: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<ProjectMemory>, String> {
    let options = FindOptions::builder()
        .sort(doc! {"updated_at": -1})
        .limit(Some(limit.max(1).min(500)))
        .skip(Some(offset.max(0) as u64))
        .build();

    let cursor = project_memories_collection(db)
        .find(doc! {
            "user_id": user_id,
            "contact_id": contact_id,
        })
        .with_options(options)
        .await
        .map_err(|e| e.to_string())?;
    cursor.try_collect().await.map_err(|e| e.to_string())
}

pub async fn list_agent_ids_with_pending_recall_rollup_by_user(
    db: &Db,
    user_id: &str,
    max_level: i64,
    limit: i64,
) -> Result<Vec<String>, String> {
    let pipeline = vec![
        doc! {"$match": {
            "user_id": user_id,
            "rolled_up": {"$ne": 1},
            "level": {"$lt": max_level.max(1)},
        }},
        doc! {"$group": {"_id": "$agent_id", "min_updated_at": {"$min": "$updated_at"}}},
        doc! {"$sort": {"min_updated_at": 1}},
        doc! {"$limit": limit.max(1).min(5000)},
        doc! {"$project": {"_id": 0, "agent_id": "$_id"}},
    ];

    let cursor = db
        .collection::<mongodb::bson::Document>("agent_recalls")
        .aggregate(pipeline)
        .await
        .map_err(|e| e.to_string())?;
    let docs: Vec<mongodb::bson::Document> =
        cursor.try_collect().await.map_err(|e| e.to_string())?;

    Ok(docs
        .into_iter()
        .filter_map(|doc| doc.get_str("agent_id").ok().map(|value| value.to_string()))
        .collect())
}

pub async fn list_agent_recalls(
    db: &Db,
    user_id: &str,
    agent_id: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<AgentRecall>, String> {
    let options = FindOptions::builder()
        .sort(doc! {"updated_at": -1})
        .limit(Some(limit.max(1).min(500)))
        .skip(Some(offset.max(0) as u64))
        .build();

    let cursor = agent_recalls_collection(db)
        .find(doc! {
            "user_id": user_id,
            "agent_id": agent_id,
        })
        .with_options(options)
        .await
        .map_err(|e| e.to_string())?;
    cursor.try_collect().await.map_err(|e| e.to_string())
}

pub async fn list_pending_agent_recalls_by_level(
    db: &Db,
    user_id: &str,
    agent_id: &str,
    level: i64,
) -> Result<Vec<AgentRecall>, String> {
    let options = FindOptions::builder().sort(doc! {"updated_at": 1}).build();
    let cursor = agent_recalls_collection(db)
        .find(doc! {
            "user_id": user_id,
            "agent_id": agent_id,
            "level": level,
            "rolled_up": {"$ne": 1},
        })
        .with_options(options)
        .await
        .map_err(|e| e.to_string())?;
    cursor.try_collect().await.map_err(|e| e.to_string())
}
