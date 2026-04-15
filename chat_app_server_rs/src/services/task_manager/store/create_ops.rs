use mongodb::bson::Document;
use uuid::Uuid;

use crate::repositories::db::with_db;
use crate::services::task_manager::mapper::task_record_to_doc;
use crate::services::task_manager::normalizer::{normalize_task_drafts, trimmed_non_empty};
use crate::services::task_manager::types::{TaskDraft, TaskRecord};

pub async fn create_tasks_for_turn(
    conversation_id: &str,
    conversation_turn_id: &str,
    draft_tasks: Vec<TaskDraft>,
) -> Result<Vec<TaskRecord>, String> {
    let conversation_id = trimmed_non_empty(conversation_id)
        .ok_or_else(|| "conversation_id is required".to_string())?
        .to_string();
    let conversation_turn_id = trimmed_non_empty(conversation_turn_id)
        .ok_or_else(|| "conversation_turn_id is required".to_string())?
        .to_string();
    let draft_tasks = normalize_task_drafts(draft_tasks)?;
    if draft_tasks.is_empty() {
        return Ok(Vec::new());
    }

    let now = crate::core::time::now_rfc3339();
    let records: Vec<TaskRecord> = draft_tasks
        .into_iter()
        .map(|draft| TaskRecord {
            id: Uuid::new_v4().to_string(),
            conversation_id: conversation_id.clone(),
            conversation_turn_id: conversation_turn_id.clone(),
            title: draft.title,
            details: draft.details,
            priority: draft.priority,
            status: draft.status,
            tags: draft.tags,
            due_at: draft.due_at,
            created_at: now.clone(),
            updated_at: now.clone(),
        })
        .collect();

    let mongo_records = records.clone();
    let sqlite_records = records.clone();

    with_db(
        move |db| {
            let records = mongo_records.clone();
            Box::pin(async move {
                let docs: Vec<Document> = records.iter().map(task_record_to_doc).collect();
                db.collection::<Document>("task_manager_tasks")
                    .insert_many(docs, None)
                    .await
                    .map_err(|err| err.to_string())?;
                Ok(records)
            })
        },
        move |pool| {
            let records = sqlite_records.clone();
            Box::pin(async move {
                let mut tx = pool.begin().await.map_err(|err| err.to_string())?;
                for task in &records {
                    let tags_json =
                        serde_json::to_string(&task.tags).unwrap_or_else(|_| "[]".to_string());
                    sqlx::query(
                        "INSERT INTO task_manager_tasks (id, conversation_id, conversation_turn_id, title, details, priority, status, tags_json, due_at, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    )
                    .bind(&task.id)
                    .bind(&task.conversation_id)
                    .bind(&task.conversation_turn_id)
                    .bind(&task.title)
                    .bind(&task.details)
                    .bind(&task.priority)
                    .bind(&task.status)
                    .bind(tags_json)
                    .bind(&task.due_at)
                    .bind(&task.created_at)
                    .bind(&task.updated_at)
                    .execute(&mut *tx)
                    .await
                    .map_err(|err| err.to_string())?;
                }
                tx.commit().await.map_err(|err| err.to_string())?;
                Ok(records)
            })
        },
    )
    .await
}
