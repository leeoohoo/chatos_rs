use futures_util::TryStreamExt;
use mongodb::bson::{doc, Bson};
use mongodb::options::FindOptions;
use uuid::Uuid;

use crate::db::Db;
use crate::models::{CreateTaskExecutionMessageRequest, TaskExecutionMessage, TaskExecutionScope};

use super::now_rfc3339;

fn collection(db: &Db) -> mongodb::Collection<TaskExecutionMessage> {
    db.collection::<TaskExecutionMessage>("task_execution_messages")
}

pub fn build_scope_key(user_id: &str, contact_agent_id: &str, project_id: &str) -> String {
    format!(
        "{}::{}::{}",
        user_id.trim(),
        contact_agent_id.trim(),
        project_id.trim()
    )
}

fn scope_filter(user_id: &str, contact_agent_id: &str, project_id: &str) -> mongodb::bson::Document {
    doc! {
        "user_id": user_id.trim(),
        "contact_agent_id": contact_agent_id.trim(),
        "project_id": project_id.trim(),
    }
}

pub async fn create_message(
    db: &Db,
    req: CreateTaskExecutionMessageRequest,
) -> Result<TaskExecutionMessage, String> {
    let message = TaskExecutionMessage {
        id: Uuid::new_v4().to_string(),
        user_id: req.user_id.trim().to_string(),
        contact_agent_id: req.contact_agent_id.trim().to_string(),
        project_id: req.project_id.trim().to_string(),
        scope_key: build_scope_key(
            req.user_id.as_str(),
            req.contact_agent_id.as_str(),
            req.project_id.as_str(),
        ),
        task_id: req.task_id,
        source_session_id: req.source_session_id,
        role: req.role,
        content: req.content,
        message_mode: req.message_mode,
        message_source: req.message_source,
        tool_calls: req.tool_calls,
        tool_call_id: req.tool_call_id,
        reasoning: req.reasoning,
        metadata: req.metadata,
        summary_status: "pending".to_string(),
        summary_id: None,
        summarized_at: None,
        created_at: now_rfc3339(),
    };

    collection(db)
        .insert_one(message.clone())
        .await
        .map_err(|e| e.to_string())?;

    Ok(message)
}

#[derive(Debug, Clone)]
pub struct SyncTaskExecutionMessageInput {
    pub message_id: String,
    pub user_id: String,
    pub contact_agent_id: String,
    pub project_id: String,
    pub task_id: Option<String>,
    pub source_session_id: Option<String>,
    pub role: String,
    pub content: String,
    pub message_mode: Option<String>,
    pub message_source: Option<String>,
    pub tool_calls_json: Option<String>,
    pub tool_call_id: Option<String>,
    pub reasoning: Option<String>,
    pub metadata_json: Option<String>,
    pub created_at: String,
}

pub async fn upsert_message_sync(
    db: &Db,
    input: SyncTaskExecutionMessageInput,
) -> Result<TaskExecutionMessage, String> {
    let tool_calls = input
        .tool_calls_json
        .and_then(|v| serde_json::from_str::<serde_json::Value>(v.as_str()).ok());
    let metadata = input
        .metadata_json
        .and_then(|v| serde_json::from_str::<serde_json::Value>(v.as_str()).ok());
    let tool_calls_bson = tool_calls
        .as_ref()
        .and_then(|v| mongodb::bson::to_bson(v).ok())
        .unwrap_or(Bson::Null);
    let metadata_bson = metadata
        .as_ref()
        .and_then(|v| mongodb::bson::to_bson(v).ok())
        .unwrap_or(Bson::Null);
    let scope_key = build_scope_key(
        input.user_id.as_str(),
        input.contact_agent_id.as_str(),
        input.project_id.as_str(),
    );

    collection(db)
        .update_one(
            doc! {"id": &input.message_id},
            doc! {
                "$set": {
                    "user_id": input.user_id.trim(),
                    "contact_agent_id": input.contact_agent_id.trim(),
                    "project_id": input.project_id.trim(),
                    "scope_key": &scope_key,
                    "task_id": input.task_id,
                    "source_session_id": input.source_session_id,
                    "role": &input.role,
                    "content": &input.content,
                    "message_mode": input.message_mode,
                    "message_source": input.message_source,
                    "tool_calls": tool_calls_bson,
                    "tool_call_id": input.tool_call_id,
                    "reasoning": input.reasoning,
                    "metadata": metadata_bson,
                    "created_at": input.created_at,
                },
                "$setOnInsert": {
                    "id": &input.message_id,
                    "summary_status": "pending",
                    "summary_id": Bson::Null,
                    "summarized_at": Bson::Null,
                }
            },
        )
        .upsert(true)
        .await
        .map_err(|e| e.to_string())?;

    get_message_by_id(db, input.message_id.as_str())
        .await?
        .ok_or_else(|| "upserted task execution message not found".to_string())
}

pub async fn get_message_by_id(
    db: &Db,
    message_id: &str,
) -> Result<Option<TaskExecutionMessage>, String> {
    collection(db)
        .find_one(doc! {"id": message_id})
        .await
        .map_err(|e| e.to_string())
}

pub async fn list_messages(
    db: &Db,
    user_id: &str,
    contact_agent_id: &str,
    project_id: &str,
    limit: i64,
    offset: i64,
    asc: bool,
) -> Result<Vec<TaskExecutionMessage>, String> {
    let sort_order = if asc { 1 } else { -1 };
    let options = FindOptions::builder()
        .sort(doc! {"created_at": sort_order})
        .limit(Some(limit.max(1).min(2000)))
        .skip(Some(offset.max(0) as u64))
        .build();

    let cursor = collection(db)
        .find(scope_filter(user_id, contact_agent_id, project_id))
        .with_options(options)
        .await
        .map_err(|e| e.to_string())?;

    cursor.try_collect().await.map_err(|e| e.to_string())
}

pub async fn list_pending_messages(
    db: &Db,
    user_id: &str,
    contact_agent_id: &str,
    project_id: &str,
    limit: Option<i64>,
) -> Result<Vec<TaskExecutionMessage>, String> {
    let options = if let Some(v) = limit {
        FindOptions::builder()
            .sort(doc! {"created_at": 1})
            .limit(Some(v.max(1)))
            .build()
    } else {
        FindOptions::builder().sort(doc! {"created_at": 1}).build()
    };

    let cursor = collection(db)
        .find(doc! {
            "user_id": user_id.trim(),
            "contact_agent_id": contact_agent_id.trim(),
            "project_id": project_id.trim(),
            "summary_status": "pending",
        })
        .with_options(options)
        .await
        .map_err(|e| e.to_string())?;

    cursor.try_collect().await.map_err(|e| e.to_string())
}

pub async fn delete_messages(
    db: &Db,
    user_id: &str,
    contact_agent_id: &str,
    project_id: &str,
) -> Result<i64, String> {
    let result = collection(db)
        .delete_many(scope_filter(user_id, contact_agent_id, project_id))
        .await
        .map_err(|e| e.to_string())?;
    Ok(result.deleted_count as i64)
}

pub async fn mark_messages_summarized(
    db: &Db,
    user_id: &str,
    contact_agent_id: &str,
    project_id: &str,
    message_ids: &[String],
    summary_id: &str,
) -> Result<usize, String> {
    if message_ids.is_empty() {
        return Ok(0);
    }

    let now = now_rfc3339();
    let result = collection(db)
        .update_many(
            doc! {
                "user_id": user_id.trim(),
                "contact_agent_id": contact_agent_id.trim(),
                "project_id": project_id.trim(),
                "id": {"$in": message_ids.to_vec()},
                "summary_status": "pending",
            },
            doc! {
                "$set": {
                    "summary_status": "summarized",
                    "summary_id": summary_id,
                    "summarized_at": &now,
                }
            },
        )
        .await
        .map_err(|e| e.to_string())?;

    Ok(result.modified_count as usize)
}

pub async fn list_pending_scopes_by_user(
    db: &Db,
    user_id: &str,
    limit: i64,
) -> Result<Vec<TaskExecutionScope>, String> {
    let pipeline = vec![
        doc! {"$match": {"user_id": user_id.trim(), "summary_status": "pending"}},
        doc! {"$group": {
            "_id": {
                "user_id": "$user_id",
                "contact_agent_id": "$contact_agent_id",
                "project_id": "$project_id",
                "scope_key": "$scope_key",
            },
            "min_created_at": {"$min": "$created_at"}
        }},
        doc! {"$sort": {"min_created_at": 1}},
        doc! {"$limit": limit.max(1).min(5000)},
        doc! {"$project": {
            "_id": 0,
            "user_id": "$_id.user_id",
            "contact_agent_id": "$_id.contact_agent_id",
            "project_id": "$_id.project_id",
            "scope_key": "$_id.scope_key",
        }},
    ];

    let cursor = db
        .collection::<mongodb::bson::Document>("task_execution_messages")
        .aggregate(pipeline)
        .await
        .map_err(|e| e.to_string())?;
    let docs: Vec<mongodb::bson::Document> =
        cursor.try_collect().await.map_err(|e| e.to_string())?;

    let mut out = Vec::new();
    for doc in docs {
        let Some(scope_user_id) = doc.get_str("user_id").ok() else {
            continue;
        };
        let Some(contact_agent_id) = doc.get_str("contact_agent_id").ok() else {
            continue;
        };
        let Some(project_id) = doc.get_str("project_id").ok() else {
            continue;
        };
        let scope_key = doc
            .get_str("scope_key")
            .ok()
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| build_scope_key(scope_user_id, contact_agent_id, project_id));
        out.push(TaskExecutionScope {
            user_id: scope_user_id.to_string(),
            contact_agent_id: contact_agent_id.to_string(),
            project_id: project_id.to_string(),
            scope_key,
        });
    }
    Ok(out)
}
