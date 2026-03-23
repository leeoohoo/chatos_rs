use mongodb::bson::doc;
use uuid::Uuid;

use crate::db::Db;
use crate::models::{CreateSummaryInput, SessionSummary};

use super::collection;
use super::now_rfc3339;

pub async fn create_summary(db: &Db, input: CreateSummaryInput) -> Result<SessionSummary, String> {
    let now = now_rfc3339();
    let summary = SessionSummary {
        id: Uuid::new_v4().to_string(),
        session_id: input.session_id,
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

    collection(db)
        .insert_one(summary.clone())
        .await
        .map_err(|e| e.to_string())?;

    Ok(summary)
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
