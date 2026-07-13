// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use mongodb::bson::{doc, Bson, Document};
use mongodb::options::{FindOneAndUpdateOptions, ReturnDocument};

use crate::repositories::db::with_db;
use crate::services::task_manager::mapper::task_record_from_doc;
use crate::services::task_manager::normalizer::trimmed_non_empty;
use crate::services::task_manager::types::{TaskRecord, TaskUpdatePatch, TASK_NOT_FOUND_ERR};

use super::super::get_task_by_id;
use super::state_rules::{
    apply_terminal_state_defaults, merged_task_record, validate_terminal_task_state,
};

pub(super) async fn update_task_by_id_impl(
    conversation_id: &str,
    task_id: &str,
    patch: TaskUpdatePatch,
) -> Result<TaskRecord, String> {
    let conversation_id = trimmed_non_empty(conversation_id)
        .ok_or_else(|| "conversation_id is required".to_string())?
        .to_string();
    let task_id = trimmed_non_empty(task_id)
        .ok_or_else(|| "task_id is required".to_string())?
        .to_string();

    let patch = patch.normalized()?;
    if patch.is_empty() {
        return Err("at least one task field is required".to_string());
    }
    let current = get_task_by_id(conversation_id.as_str(), task_id.as_str()).await?;
    let mut patch = patch;
    apply_terminal_state_defaults(&mut patch);
    let next = merged_task_record(current, &patch);
    validate_terminal_task_state(&next)?;

    let updated_at = crate::core::time::now_rfc3339();

    let conversation_id_for_mongo = conversation_id.clone();
    let task_id_for_mongo = task_id.clone();
    let title_for_mongo = patch.title.clone();
    let details_for_mongo = patch.details.clone();
    let priority_for_mongo = patch.priority.clone();
    let status_for_mongo = patch.status.clone();
    let tags_for_mongo = patch.tags.clone();
    let due_at_for_mongo = patch.due_at.clone();
    let outcome_summary_for_mongo = patch.outcome_summary.clone();
    let outcome_items_for_mongo = patch.outcome_items.clone();
    let resume_hint_for_mongo = patch.resume_hint.clone();
    let blocker_reason_for_mongo = patch.blocker_reason.clone();
    let blocker_needs_for_mongo = patch.blocker_needs.clone();
    let blocker_kind_for_mongo = patch.blocker_kind.clone();
    let completed_at_for_mongo = patch.completed_at.clone();
    let last_outcome_at_for_mongo = patch.last_outcome_at.clone();
    let updated_at_for_mongo = updated_at.clone();

    with_db(move |db| {
        let conversation_id = conversation_id_for_mongo.clone();
        let task_id = task_id_for_mongo.clone();
        let title = title_for_mongo.clone();
        let details = details_for_mongo.clone();
        let priority = priority_for_mongo.clone();
        let status = status_for_mongo.clone();
        let tags = tags_for_mongo.clone();
        let due_at = due_at_for_mongo.clone();
        let outcome_summary = outcome_summary_for_mongo.clone();
        let outcome_items = outcome_items_for_mongo.clone();
        let resume_hint = resume_hint_for_mongo.clone();
        let blocker_reason = blocker_reason_for_mongo.clone();
        let blocker_needs = blocker_needs_for_mongo.clone();
        let blocker_kind = blocker_kind_for_mongo.clone();
        let completed_at = completed_at_for_mongo.clone();
        let last_outcome_at = last_outcome_at_for_mongo.clone();
        let updated_at = updated_at_for_mongo.clone();

        Box::pin(async move {
            let mut set_doc = doc! { "updated_at": updated_at };

            if let Some(value) = title {
                set_doc.insert("title", Bson::String(value));
            }
            if let Some(value) = details {
                set_doc.insert("details", Bson::String(value));
            }
            if let Some(value) = priority {
                set_doc.insert("priority", Bson::String(value));
            }
            if let Some(value) = status {
                set_doc.insert("status", Bson::String(value));
            }
            if let Some(values) = tags {
                set_doc.insert(
                    "tags",
                    Bson::Array(values.into_iter().map(Bson::String).collect()),
                );
            }
            if let Some(value) = due_at {
                match value {
                    Some(due_at) => {
                        set_doc.insert("due_at", Bson::String(due_at));
                    }
                    None => {
                        set_doc.insert("due_at", Bson::Null);
                    }
                }
            }
            if let Some(value) = outcome_summary {
                set_doc.insert("outcome_summary", Bson::String(value));
            }
            if let Some(values) = outcome_items {
                let bson_items = values
                    .iter()
                    .map(|item| {
                        let mut doc = doc! {
                            "kind": item.kind.clone(),
                            "text": item.text.clone(),
                            "refs": Bson::Array(
                                item.refs.iter().cloned().map(Bson::String).collect()
                            ),
                        };
                        if let Some(importance) = item.importance.clone() {
                            doc.insert("importance", importance);
                        }
                        Bson::Document(doc)
                    })
                    .collect::<Vec<Bson>>();
                set_doc.insert("outcome_items", Bson::Array(bson_items));
            }
            if let Some(value) = resume_hint {
                set_doc.insert("resume_hint", Bson::String(value));
            }
            if let Some(value) = blocker_reason {
                set_doc.insert("blocker_reason", Bson::String(value));
            }
            if let Some(values) = blocker_needs {
                set_doc.insert(
                    "blocker_needs",
                    Bson::Array(values.into_iter().map(Bson::String).collect()),
                );
            }
            if let Some(value) = blocker_kind {
                set_doc.insert("blocker_kind", Bson::String(value));
            }
            if let Some(value) = completed_at {
                match value {
                    Some(completed_at) => {
                        set_doc.insert("completed_at", Bson::String(completed_at));
                    }
                    None => {
                        set_doc.insert("completed_at", Bson::Null);
                    }
                }
            }
            if let Some(value) = last_outcome_at {
                match value {
                    Some(last_outcome_at) => {
                        set_doc.insert("last_outcome_at", Bson::String(last_outcome_at));
                    }
                    None => {
                        set_doc.insert("last_outcome_at", Bson::Null);
                    }
                }
            }

            let options = FindOneAndUpdateOptions::builder()
                .return_document(ReturnDocument::After)
                .build();

            let updated = db
                .collection::<Document>("task_manager_tasks")
                .find_one_and_update(
                    doc! { "conversation_id": conversation_id, "id": task_id },
                    doc! { "$set": set_doc },
                    options,
                )
                .await
                .map_err(|err| err.to_string())?
                .and_then(|document| task_record_from_doc(&document))
                .ok_or_else(|| TASK_NOT_FOUND_ERR.to_string())?;

            Ok(updated)
        })
    })
    .await
}

pub(super) async fn delete_task_by_id_impl(
    conversation_id: &str,
    task_id: &str,
) -> Result<bool, String> {
    let conversation_id = trimmed_non_empty(conversation_id)
        .ok_or_else(|| "conversation_id is required".to_string())?
        .to_string();
    let task_id = trimmed_non_empty(task_id)
        .ok_or_else(|| "task_id is required".to_string())?
        .to_string();

    let conversation_id_for_mongo = conversation_id.clone();
    let task_id_for_mongo = task_id.clone();

    with_db(move |db| {
        let conversation_id = conversation_id_for_mongo.clone();
        let task_id = task_id_for_mongo.clone();
        Box::pin(async move {
            let result = db
                .collection::<Document>("task_manager_tasks")
                .delete_one(
                    doc! { "conversation_id": conversation_id, "id": task_id },
                    None,
                )
                .await
                .map_err(|err| err.to_string())?;
            Ok(result.deleted_count > 0)
        })
    })
    .await
}
