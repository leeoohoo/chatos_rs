use futures_util::TryStreamExt;
use mongodb::bson::doc;
use mongodb::options::FindOptions;
use uuid::Uuid;

use crate::db::Db;
use crate::models::{TaskResultBrief, UpsertTaskResultBriefRequest};

use super::{normalize_optional_text, now_rfc3339};

fn collection(db: &Db) -> mongodb::Collection<TaskResultBrief> {
    db.collection::<TaskResultBrief>("task_result_briefs")
}

pub async fn upsert_task_result_brief(
    db: &Db,
    input: UpsertTaskResultBriefRequest,
) -> Result<TaskResultBrief, String> {
    let task_id = input.task_id.trim().to_string();
    if task_id.is_empty() {
        return Err("task_id is required".to_string());
    }

    let now = now_rfc3339();
    let existing = collection(db)
        .find_one(doc! { "task_id": task_id.as_str() })
        .await
        .map_err(|e| e.to_string())?;

    let brief = TaskResultBrief {
        id: existing
            .as_ref()
            .map(|item| item.id.clone())
            .unwrap_or_else(|| Uuid::new_v4().to_string()),
        task_id,
        user_id: input.user_id.trim().to_string(),
        contact_agent_id: input.contact_agent_id.trim().to_string(),
        project_id: input.project_id.trim().to_string(),
        source_session_id: normalize_optional_text(input.source_session_id.as_deref()),
        source_turn_id: normalize_optional_text(input.source_turn_id.as_deref()),
        task_title: input.task_title.trim().to_string(),
        task_status: input.task_status.trim().to_string(),
        result_summary: input.result_summary.trim().to_string(),
        result_format: normalize_optional_text(input.result_format.as_deref()),
        result_message_id: normalize_optional_text(input.result_message_id.as_deref()),
        agent_memory_summarized: 0,
        agent_memory_summarized_at: None,
        finished_at: normalize_optional_text(input.finished_at.as_deref()),
        created_at: existing
            .as_ref()
            .map(|item| item.created_at.clone())
            .unwrap_or_else(|| now.clone()),
        updated_at: now,
    };

    collection(db)
        .replace_one(doc! { "task_id": brief.task_id.as_str() }, brief.clone())
        .upsert(true)
        .await
        .map_err(|e| e.to_string())?;

    Ok(brief)
}

pub async fn list_agent_ids_with_pending_agent_memory_by_user(
    db: &Db,
    user_id: &str,
    limit: i64,
) -> Result<Vec<String>, String> {
    let pipeline = vec![
        doc! {"$match": {
            "user_id": user_id.trim(),
            "agent_memory_summarized": {"$ne": 1},
        }},
        doc! {"$group": {
            "_id": "$contact_agent_id",
            "min_created_at": {"$min": "$created_at"},
        }},
        doc! {"$sort": {"min_created_at": 1}},
        doc! {"$limit": limit.max(1).min(5000)},
        doc! {"$project": {"_id": 0, "agent_id": "$_id"}},
    ];

    let cursor = db
        .collection::<mongodb::bson::Document>("task_result_briefs")
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

pub async fn list_task_result_briefs(
    db: &Db,
    user_id: &str,
    contact_agent_id: &str,
    project_id: &str,
    limit: i64,
) -> Result<Vec<TaskResultBrief>, String> {
    let options = FindOptions::builder()
        .sort(doc! { "finished_at": -1, "updated_at": -1 })
        .limit(Some(limit.max(1).min(100)))
        .build();
    let cursor = collection(db)
        .find(doc! {
            "user_id": user_id.trim(),
            "contact_agent_id": contact_agent_id.trim(),
            "project_id": project_id.trim(),
        })
        .with_options(options)
        .await
        .map_err(|e| e.to_string())?;
    cursor.try_collect().await.map_err(|e| e.to_string())
}

pub async fn get_task_result_brief_by_task_id(
    db: &Db,
    task_id: &str,
) -> Result<Option<TaskResultBrief>, String> {
    collection(db)
        .find_one(doc! { "task_id": task_id.trim() })
        .await
        .map_err(|e| e.to_string())
}

pub async fn list_pending_task_result_briefs_by_agent(
    db: &Db,
    user_id: &str,
    contact_agent_id: &str,
) -> Result<Vec<TaskResultBrief>, String> {
    let options = FindOptions::builder()
        .sort(doc! { "created_at": 1, "updated_at": 1 })
        .build();
    let cursor = collection(db)
        .find(doc! {
            "user_id": user_id.trim(),
            "contact_agent_id": contact_agent_id.trim(),
            "agent_memory_summarized": {"$ne": 1},
        })
        .with_options(options)
        .await
        .map_err(|e| e.to_string())?;
    cursor.try_collect().await.map_err(|e| e.to_string())
}

pub async fn mark_task_result_briefs_agent_memory_summarized(
    db: &Db,
    brief_ids: &[String],
) -> Result<usize, String> {
    if brief_ids.is_empty() {
        return Ok(0);
    }

    let now = now_rfc3339();
    let result = collection(db)
        .update_many(
            doc! {
                "id": {"$in": brief_ids.to_vec()},
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
