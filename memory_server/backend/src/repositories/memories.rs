use futures_util::TryStreamExt;
use mongodb::bson::{doc, Bson};
use mongodb::options::FindOptions;
use uuid::Uuid;

use crate::db::Db;
use crate::models::{AgentRecall, ProjectMemory};

use super::now_rfc3339;

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

pub async fn list_agent_ids_with_pending_project_memories_by_user(
    db: &Db,
    user_id: &str,
    limit: i64,
) -> Result<Vec<String>, String> {
    let pipeline = vec![
        doc! {"$match": {
            "user_id": user_id,
            "recall_summarized": {"$ne": 1},
        }},
        doc! {"$group": {"_id": "$agent_id", "min_updated_at": {"$min": "$updated_at"}}},
        doc! {"$sort": {"min_updated_at": 1}},
        doc! {"$limit": limit.max(1).min(5000)},
        doc! {"$project": {"_id": 0, "agent_id": "$_id"}},
    ];

    let cursor = db
        .collection::<mongodb::bson::Document>("project_memories")
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

pub async fn list_pending_project_memories_by_agent(
    db: &Db,
    user_id: &str,
    agent_id: &str,
) -> Result<Vec<ProjectMemory>, String> {
    let options = FindOptions::builder().sort(doc! {"updated_at": 1}).build();
    let cursor = project_memories_collection(db)
        .find(doc! {
            "user_id": user_id,
            "agent_id": agent_id,
            "recall_summarized": {"$ne": 1},
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

pub async fn mark_project_memories_recalled(
    db: &Db,
    user_id: &str,
    memory_ids: &[String],
) -> Result<usize, String> {
    if memory_ids.is_empty() {
        return Ok(0);
    }

    let now = now_rfc3339();
    let result = project_memories_collection(db)
        .update_many(
            doc! {
                "user_id": user_id,
                "id": {"$in": memory_ids.to_vec()},
            },
            doc! {
                "$set": {
                    "recall_summarized": 1,
                    "recall_summarized_at": &now,
                    "updated_at": &now,
                }
            },
        )
        .await
        .map_err(|e| e.to_string())?;
    Ok(result.modified_count as usize)
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

pub async fn mark_agent_recalls_rolled_up(
    db: &Db,
    user_id: &str,
    agent_id: &str,
    recall_ids: &[String],
    rollup_recall_key: &str,
) -> Result<usize, String> {
    if recall_ids.is_empty() {
        return Ok(0);
    }

    let now = now_rfc3339();
    let result = agent_recalls_collection(db)
        .update_many(
            doc! {
                "user_id": user_id,
                "agent_id": agent_id,
                "id": {"$in": recall_ids.to_vec()},
                "rolled_up": {"$ne": 1},
            },
            doc! {
                "$set": {
                    "rolled_up": 1,
                    "rollup_recall_key": rollup_recall_key,
                    "rolled_up_at": &now,
                    "updated_at": &now,
                }
            },
        )
        .await
        .map_err(|e| e.to_string())?;
    Ok(result.modified_count as usize)
}

pub struct UpsertProjectMemoryInput {
    pub user_id: String,
    pub contact_id: String,
    pub agent_id: String,
    pub project_id: String,
    pub memory_text: String,
    pub last_source_at: Option<String>,
}

pub async fn upsert_project_memory(
    db: &Db,
    input: UpsertProjectMemoryInput,
) -> Result<Option<ProjectMemory>, String> {
    let now = now_rfc3339();
    let filter = doc! {
        "user_id": input.user_id.as_str(),
        "contact_id": input.contact_id.as_str(),
        "project_id": input.project_id.as_str(),
    };

    let mut set_doc = doc! {
        "user_id": input.user_id.as_str(),
        "contact_id": input.contact_id.as_str(),
        "agent_id": input.agent_id.as_str(),
        "project_id": input.project_id.as_str(),
        "memory_text": input.memory_text.as_str(),
        "recall_summarized": 0,
        "recall_summarized_at": Bson::Null,
        "updated_at": now.as_str(),
    };
    if let Some(last_source_at) = input.last_source_at.as_deref() {
        if !last_source_at.trim().is_empty() {
            set_doc.insert("last_source_at", last_source_at);
        }
    }

    project_memories_collection(db)
        .update_one(
            filter.clone(),
            doc! {
                "$set": set_doc,
                "$inc": {"memory_version": 1},
                "$setOnInsert": {
                    "id": Uuid::new_v4().to_string(),
                    "memory_version": 0,
                    "recall_summarized": 0,
                    "recall_summarized_at": Bson::Null,
                }
            },
        )
        .upsert(true)
        .await
        .map_err(|e| e.to_string())?;

    project_memories_collection(db)
        .find_one(filter)
        .await
        .map_err(|e| e.to_string())
}

pub struct UpsertAgentRecallInput {
    pub user_id: String,
    pub agent_id: String,
    pub recall_key: String,
    pub recall_text: String,
    pub level: i64,
    pub source_project_ids: Vec<String>,
    pub confidence: Option<f64>,
    pub last_seen_at: Option<String>,
}

pub async fn upsert_agent_recall(
    db: &Db,
    input: UpsertAgentRecallInput,
) -> Result<Option<AgentRecall>, String> {
    let now = now_rfc3339();
    let filter = doc! {
        "user_id": input.user_id.as_str(),
        "agent_id": input.agent_id.as_str(),
        "recall_key": input.recall_key.as_str(),
    };

    let mut set_doc = doc! {
        "recall_text": input.recall_text.as_str(),
        "level": input.level.max(0),
        "rolled_up": 0,
        "rollup_recall_key": Bson::Null,
        "rolled_up_at": Bson::Null,
        "updated_at": now.as_str(),
    };
    if let Some(confidence) = input.confidence {
        set_doc.insert("confidence", confidence);
    }
    if let Some(last_seen_at) = input.last_seen_at.as_deref() {
        if !last_seen_at.trim().is_empty() {
            set_doc.insert("last_seen_at", last_seen_at);
        }
    }

    let mut update_doc = doc! {
        "$set": set_doc,
        "$setOnInsert": {
            "id": Uuid::new_v4().to_string(),
            "user_id": input.user_id.as_str(),
            "agent_id": input.agent_id.as_str(),
            "recall_key": input.recall_key.as_str(),
            "source_project_ids": Vec::<String>::new(),
            "rolled_up": 0,
            "rollup_recall_key": Bson::Null,
            "rolled_up_at": Bson::Null,
        }
    };
    let source_project_ids: Vec<String> = input
        .source_project_ids
        .iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect();
    if !source_project_ids.is_empty() {
        update_doc.insert(
            "$addToSet",
            doc! {"source_project_ids": {"$each": source_project_ids}},
        );
    }

    agent_recalls_collection(db)
        .update_one(filter.clone(), update_doc)
        .upsert(true)
        .await
        .map_err(|e| e.to_string())?;

    agent_recalls_collection(db)
        .find_one(filter)
        .await
        .map_err(|e| e.to_string())
}
