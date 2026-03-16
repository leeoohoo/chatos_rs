use futures_util::TryStreamExt;
use mongodb::bson::doc;
use mongodb::options::FindOptions;

use crate::db::Db;
use crate::models::{AgentRecall, ProjectMemory};

fn project_memories_collection(db: &Db) -> mongodb::Collection<ProjectMemory> {
    db.collection::<ProjectMemory>("project_memories")
}

fn agent_recalls_collection(db: &Db) -> mongodb::Collection<AgentRecall> {
    db.collection::<AgentRecall>("agent_recalls")
}

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
