use mongodb::bson::doc;
use uuid::Uuid;

use crate::db::Db;
use crate::models::{CreateSummaryInput, SessionSummary};

use super::collection;
use super::now_rfc3339;

#[derive(Debug, Clone)]
pub struct CreateSummaryResult {
    pub summary: SessionSummary,
    pub inserted: bool,
}

fn is_duplicate_key_error(err: &mongodb::error::Error) -> bool {
    let text = err.to_string().to_ascii_lowercase();
    text.contains("e11000") || text.contains("duplicate key")
}

pub async fn create_summary(
    db: &Db,
    input: CreateSummaryInput,
) -> Result<CreateSummaryResult, String> {
    let now = now_rfc3339();
    let source_digest = input
        .source_digest
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let summary = SessionSummary {
        id: Uuid::new_v4().to_string(),
        session_id: input.session_id,
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
        rollup_summary_id: None,
        rolled_up_at: None,
        agent_memory_summarized: 0,
        agent_memory_summarized_at: None,
        created_at: now.clone(),
        updated_at: now,
    };

    match collection(db).insert_one(summary.clone()).await {
        Ok(_) => {
            return Ok(CreateSummaryResult {
                summary,
                inserted: true,
            });
        }
        Err(err) if is_duplicate_key_error(&err) => {
            if let Some(digest) = source_digest.as_deref() {
                if let Some(existing) = collection(db)
                    .find_one(doc! {
                        "session_id": summary.session_id.as_str(),
                        "level": summary.level,
                        "source_digest": digest,
                    })
                    .await
                    .map_err(|e| e.to_string())?
                {
                    return Ok(CreateSummaryResult {
                        summary: existing,
                        inserted: false,
                    });
                }
            }
            return Err(err.to_string());
        }
        Err(err) => return Err(err.to_string()),
    }
}

pub async fn mark_summaries_rolled_up(
    db: &Db,
    summary_ids: &[String],
    rollup_summary_id: &str,
) -> Result<usize, String> {
    if summary_ids.is_empty() {
        return Ok(0);
    }

    let now = now_rfc3339();
    let result = collection(db)
        .update_many(
            doc! {
                "id": {"$in": summary_ids.to_vec()},
                "status": "pending",
            },
            doc! {
                "$set": {
                    "status": "summarized",
                    "rollup_summary_id": rollup_summary_id,
                    "rolled_up_at": &now,
                    "updated_at": &now,
                }
            },
        )
        .await
        .map_err(|e| e.to_string())?;

    Ok(result.modified_count as usize)
}

pub async fn mark_summaries_agent_memory_summarized(
    db: &Db,
    summary_ids: &[String],
) -> Result<usize, String> {
    if summary_ids.is_empty() {
        return Ok(0);
    }

    let now = now_rfc3339();
    let result = collection(db)
        .update_many(
            doc! {
                "id": {"$in": summary_ids.to_vec()},
                "agent_memory_summarized": {"$ne": 1},
            },
            doc! {
                "$set": {
                    "agent_memory_summarized": 1,
                    "agent_memory_summarized_at": &now,
                    "updated_at": &now,
                }
            },
        )
        .await
        .map_err(|e| e.to_string())?;

    Ok(result.modified_count as usize)
}

pub async fn delete_summary(db: &Db, session_id: &str, summary_id: &str) -> Result<bool, String> {
    let result = collection(db)
        .delete_one(doc! {"session_id": session_id, "id": summary_id})
        .await
        .map_err(|e| e.to_string())?;
    Ok(result.deleted_count > 0)
}
