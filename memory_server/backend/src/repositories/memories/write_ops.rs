use mongodb::bson::{doc, Bson};
use uuid::Uuid;

use crate::db::Db;
use crate::models::{AgentRecall, ProjectMemory};

use super::{agent_recalls_collection, now_rfc3339, project_memories_collection};

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
    pub source_digest: Option<String>,
    pub recall_text: String,
    pub level: i64,
    pub source_kind: Option<String>,
    pub source_scope_kind: Option<String>,
    pub contact_agent_id: Option<String>,
    pub project_ids: Vec<String>,
    pub task_ids: Vec<String>,
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
        "updated_at": now.as_str(),
        "project_ids": input.project_ids,
        "task_ids": input.task_ids,
    };
    if let Some(source_digest) = input
        .source_digest
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        set_doc.insert("source_digest", source_digest);
    } else {
        set_doc.insert("source_digest", Bson::Null);
    }
    if let Some(confidence) = input.confidence {
        set_doc.insert("confidence", confidence);
    }
    if let Some(source_kind) = input
        .source_kind
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        set_doc.insert("source_kind", source_kind);
    } else {
        set_doc.insert("source_kind", Bson::Null);
    }
    if let Some(source_scope_kind) = input
        .source_scope_kind
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        set_doc.insert("source_scope_kind", source_scope_kind);
    } else {
        set_doc.insert("source_scope_kind", Bson::Null);
    }
    if let Some(contact_agent_id) = input
        .contact_agent_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        set_doc.insert("contact_agent_id", contact_agent_id);
    } else {
        set_doc.insert("contact_agent_id", Bson::Null);
    }
    if let Some(last_seen_at) = input.last_seen_at.as_deref() {
        if !last_seen_at.trim().is_empty() {
            set_doc.insert("last_seen_at", last_seen_at);
        }
    }

    let update_doc = doc! {
        "$set": set_doc,
        "$setOnInsert": {
            "id": Uuid::new_v4().to_string(),
            "user_id": input.user_id.as_str(),
            "agent_id": input.agent_id.as_str(),
            "recall_key": input.recall_key.as_str(),
            "rolled_up": 0,
            "rollup_recall_key": Bson::Null,
            "rolled_up_at": Bson::Null,
        }
    };

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
