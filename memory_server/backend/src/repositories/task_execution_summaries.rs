use futures_util::TryStreamExt;
use mongodb::bson::doc;
use mongodb::options::FindOptions;
use uuid::Uuid;

use crate::db::Db;
use crate::models::{CreateTaskExecutionSummaryInput, TaskExecutionSummary};

use super::now_rfc3339;
use super::task_execution_messages::build_scope_key;

fn collection(db: &Db) -> mongodb::Collection<TaskExecutionSummary> {
    db.collection::<TaskExecutionSummary>("task_execution_summaries")
}

#[derive(Debug, Clone)]
pub struct CreateTaskExecutionSummaryResult {
    pub summary: TaskExecutionSummary,
    pub inserted: bool,
}

fn is_duplicate_key_error(err: &mongodb::error::Error) -> bool {
    let text = err.to_string().to_ascii_lowercase();
    text.contains("e11000") || text.contains("duplicate key")
}

fn scope_filter(user_id: &str, contact_agent_id: &str, project_id: &str) -> mongodb::bson::Document {
    doc! {
        "user_id": user_id.trim(),
        "contact_agent_id": contact_agent_id.trim(),
        "project_id": project_id.trim(),
    }
}

pub async fn create_summary(
    db: &Db,
    input: CreateTaskExecutionSummaryInput,
) -> Result<CreateTaskExecutionSummaryResult, String> {
    let now = now_rfc3339();
    let source_digest = input
        .source_digest
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let summary = TaskExecutionSummary {
        id: Uuid::new_v4().to_string(),
        user_id: input.user_id.trim().to_string(),
        contact_agent_id: input.contact_agent_id.trim().to_string(),
        project_id: input.project_id.trim().to_string(),
        scope_key: build_scope_key(
            input.user_id.as_str(),
            input.contact_agent_id.as_str(),
            input.project_id.as_str(),
        ),
        source_digest: source_digest.clone(),
        summary_text: input.summary_text,
        summary_model: input.summary_model,
        trigger_type: input.trigger_type,
        source_start_message_id: input.source_start_message_id,
        source_end_message_id: input.source_end_message_id,
        source_message_count: input.source_message_count,
        source_estimated_tokens: input.source_estimated_tokens,
        status: input.status,
        error_message: input.error_message,
        level: input.level,
        created_at: now.clone(),
        updated_at: now,
    };

    match collection(db).insert_one(summary.clone()).await {
        Ok(_) => Ok(CreateTaskExecutionSummaryResult {
            summary,
            inserted: true,
        }),
        Err(err) if is_duplicate_key_error(&err) => {
            if let Some(digest) = source_digest.as_deref() {
                if let Some(existing) = collection(db)
                    .find_one(doc! {
                        "user_id": summary.user_id.as_str(),
                        "contact_agent_id": summary.contact_agent_id.as_str(),
                        "project_id": summary.project_id.as_str(),
                        "level": summary.level,
                        "source_digest": digest,
                    })
                    .await
                    .map_err(|e| e.to_string())?
                {
                    return Ok(CreateTaskExecutionSummaryResult {
                        summary: existing,
                        inserted: false,
                    });
                }
            }
            Err(err.to_string())
        }
        Err(err) => Err(err.to_string()),
    }
}

pub async fn list_summaries(
    db: &Db,
    user_id: &str,
    contact_agent_id: &str,
    project_id: &str,
    level: Option<i64>,
    status: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<TaskExecutionSummary>, String> {
    let mut filter = scope_filter(user_id, contact_agent_id, project_id);
    if let Some(v) = level {
        filter.insert("level", v);
    }
    if let Some(v) = status {
        filter.insert("status", v);
    }

    let options = FindOptions::builder()
        .sort(doc! {"level": -1, "created_at": 1})
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

pub async fn find_summary_by_source_digest(
    db: &Db,
    user_id: &str,
    contact_agent_id: &str,
    project_id: &str,
    level: i64,
    source_digest: &str,
) -> Result<Option<TaskExecutionSummary>, String> {
    let normalized = source_digest.trim();
    if normalized.is_empty() {
        return Ok(None);
    }

    collection(db)
        .find_one(doc! {
            "user_id": user_id.trim(),
            "contact_agent_id": contact_agent_id.trim(),
            "project_id": project_id.trim(),
            "level": level,
            "source_digest": normalized,
        })
        .await
        .map_err(|e| e.to_string())
}

pub async fn delete_summary(
    db: &Db,
    user_id: &str,
    contact_agent_id: &str,
    project_id: &str,
    summary_id: &str,
) -> Result<bool, String> {
    let result = collection(db)
        .delete_one(doc! {
            "user_id": user_id.trim(),
            "contact_agent_id": contact_agent_id.trim(),
            "project_id": project_id.trim(),
            "id": summary_id,
        })
        .await
        .map_err(|e| e.to_string())?;
    Ok(result.deleted_count > 0)
}
