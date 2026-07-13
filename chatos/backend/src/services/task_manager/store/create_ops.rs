// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use mongodb::bson::Document;
use uuid::Uuid;

use crate::repositories::db::with_db;
use crate::services::realtime::{publish_task_board_updated, resolve_conversation_scope};
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
            outcome_summary: draft.outcome_summary,
            outcome_items: draft.outcome_items,
            resume_hint: draft.resume_hint,
            blocker_reason: draft.blocker_reason,
            blocker_needs: draft.blocker_needs,
            blocker_kind: draft.blocker_kind,
            completed_at: None,
            last_outcome_at: None,
            created_at: now.clone(),
            updated_at: now.clone(),
        })
        .collect();

    let mongo_records = records.clone();

    let created = with_db(move |db| {
        let records = mongo_records.clone();
        Box::pin(async move {
            let docs: Vec<Document> = records.iter().map(task_record_to_doc).collect();
            db.collection::<Document>("task_manager_tasks")
                .insert_many(docs, None)
                .await
                .map_err(|err| err.to_string())?;
            Ok(records)
        })
    })
    .await?;

    publish_created_tasks(&conversation_id, &conversation_turn_id, &created).await;
    Ok(created)
}

async fn publish_created_tasks(
    conversation_id: &str,
    conversation_turn_id: &str,
    records: &[TaskRecord],
) {
    if records.is_empty() {
        return;
    }
    let Ok(scope) = resolve_conversation_scope(conversation_id).await else {
        return;
    };
    let Some(user_id) = scope.user_id.as_deref() else {
        return;
    };

    for record in records {
        publish_task_board_updated(
            user_id,
            conversation_id,
            Some(conversation_turn_id),
            None,
            Some(record.id.as_str()),
            "task_created",
            Some(record.clone()),
            None,
            None,
        );
    }
}
