use mongodb::bson::{doc, Bson, Document};
use mongodb::options::{FindOneAndUpdateOptions, ReturnDocument};
use sqlx::{QueryBuilder, Sqlite};

use crate::repositories::db::with_db;
use crate::services::task_manager::mapper::task_record_from_doc;
use crate::services::task_manager::normalizer::trimmed_non_empty;
use crate::services::task_manager::types::{TaskRecord, TaskUpdatePatch, TASK_NOT_FOUND_ERR};

use super::row::TaskRow;
use super::get_task_by_id;

fn merged_task_record(mut task: TaskRecord, patch: &TaskUpdatePatch) -> TaskRecord {
    if let Some(value) = patch.title.as_ref() {
        task.title = value.clone();
    }
    if let Some(value) = patch.details.as_ref() {
        task.details = value.clone();
    }
    if let Some(value) = patch.priority.as_ref() {
        task.priority = value.clone();
    }
    if let Some(value) = patch.status.as_ref() {
        task.status = value.clone();
    }
    if let Some(values) = patch.tags.as_ref() {
        task.tags = values.clone();
    }
    if let Some(value) = patch.due_at.as_ref() {
        task.due_at = value.clone();
    }
    if let Some(value) = patch.outcome_summary.as_ref() {
        task.outcome_summary = value.clone();
    }
    if let Some(values) = patch.outcome_items.as_ref() {
        task.outcome_items = values.clone();
    }
    if let Some(value) = patch.resume_hint.as_ref() {
        task.resume_hint = value.clone();
    }
    if let Some(value) = patch.blocker_reason.as_ref() {
        task.blocker_reason = value.clone();
    }
    if let Some(values) = patch.blocker_needs.as_ref() {
        task.blocker_needs = values.clone();
    }
    if let Some(value) = patch.blocker_kind.as_ref() {
        task.blocker_kind = value.clone();
    }
    if let Some(value) = patch.completed_at.as_ref() {
        task.completed_at = value.clone();
    }
    if let Some(value) = patch.last_outcome_at.as_ref() {
        task.last_outcome_at = value.clone();
    }
    task
}

fn task_has_outcome(task: &TaskRecord) -> bool {
    !task.outcome_summary.trim().is_empty() || !task.outcome_items.is_empty()
}

fn validate_terminal_task_state(task: &TaskRecord) -> Result<(), String> {
    match task.status.as_str() {
        "done" => {
            if !task_has_outcome(task) {
                return Err(
                    "done tasks must include outcome_summary or outcome_items so later tasks can reuse the result".to_string(),
                );
            }
        }
        "blocked" => {
            if !task_has_outcome(task) {
                return Err(
                    "blocked tasks must include outcome_summary or outcome_items to record what was already tried".to_string(),
                );
            }
            if task.blocker_reason.trim().is_empty() {
                return Err(
                    "blocked tasks must include blocker_reason so the next task knows why progress stopped".to_string(),
                );
            }
        }
        _ => {}
    }

    Ok(())
}

fn apply_terminal_state_defaults(patch: &mut TaskUpdatePatch) {
    let has_outcome = patch
        .outcome_summary
        .as_deref()
        .map(str::trim)
        .map(|value| !value.is_empty())
        .unwrap_or(false)
        || patch
            .outcome_items
            .as_ref()
            .map(|items| !items.is_empty())
            .unwrap_or(false);
    let next_status = patch.status.as_deref().unwrap_or_default();

    match next_status {
        "done" => {
            if patch.completed_at.is_none() {
                patch.completed_at = Some(Some(crate::core::time::now_rfc3339()));
            }
            if patch.last_outcome_at.is_none() && has_outcome {
                patch.last_outcome_at = Some(Some(crate::core::time::now_rfc3339()));
            }
        }
        "blocked" => {
            if patch.completed_at.is_none() {
                patch.completed_at = Some(None);
            }
            if patch.last_outcome_at.is_none() && has_outcome {
                patch.last_outcome_at = Some(Some(crate::core::time::now_rfc3339()));
            }
        }
        _ => {}
    }
}

pub async fn update_task_by_id(
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

    let conversation_id_for_sqlite = conversation_id.clone();
    let task_id_for_sqlite = task_id.clone();
    let title_for_sqlite = patch.title.clone();
    let details_for_sqlite = patch.details.clone();
    let priority_for_sqlite = patch.priority.clone();
    let status_for_sqlite = patch.status.clone();
    let tags_for_sqlite = patch.tags.clone();
    let due_at_for_sqlite = patch.due_at.clone();
    let outcome_summary_for_sqlite = patch.outcome_summary.clone();
    let outcome_items_for_sqlite = patch.outcome_items.clone();
    let resume_hint_for_sqlite = patch.resume_hint.clone();
    let blocker_reason_for_sqlite = patch.blocker_reason.clone();
    let blocker_needs_for_sqlite = patch.blocker_needs.clone();
    let blocker_kind_for_sqlite = patch.blocker_kind.clone();
    let completed_at_for_sqlite = patch.completed_at.clone();
    let last_outcome_at_for_sqlite = patch.last_outcome_at.clone();
    let updated_at_for_sqlite = updated_at.clone();

    with_db(
        move |db| {
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
        },
        move |pool| {
            let conversation_id = conversation_id_for_sqlite.clone();
            let task_id = task_id_for_sqlite.clone();
            let title = title_for_sqlite.clone();
            let details = details_for_sqlite.clone();
            let priority = priority_for_sqlite.clone();
            let status = status_for_sqlite.clone();
            let tags = tags_for_sqlite.clone();
            let due_at = due_at_for_sqlite.clone();
            let outcome_summary = outcome_summary_for_sqlite.clone();
            let outcome_items = outcome_items_for_sqlite.clone();
            let resume_hint = resume_hint_for_sqlite.clone();
            let blocker_reason = blocker_reason_for_sqlite.clone();
            let blocker_needs = blocker_needs_for_sqlite.clone();
            let blocker_kind = blocker_kind_for_sqlite.clone();
            let completed_at = completed_at_for_sqlite.clone();
            let last_outcome_at = last_outcome_at_for_sqlite.clone();
            let updated_at = updated_at_for_sqlite.clone();

            Box::pin(async move {
                let mut qb = QueryBuilder::<Sqlite>::new("UPDATE task_manager_tasks SET ");
                let mut has_assignment = false;

                if let Some(value) = title {
                    if has_assignment {
                        qb.push(", ");
                    }
                    has_assignment = true;
                    qb.push("title = ");
                    qb.push_bind(value);
                }
                if let Some(value) = details {
                    if has_assignment {
                        qb.push(", ");
                    }
                    has_assignment = true;
                    qb.push("details = ");
                    qb.push_bind(value);
                }
                if let Some(value) = priority {
                    if has_assignment {
                        qb.push(", ");
                    }
                    has_assignment = true;
                    qb.push("priority = ");
                    qb.push_bind(value);
                }
                if let Some(value) = status {
                    if has_assignment {
                        qb.push(", ");
                    }
                    has_assignment = true;
                    qb.push("status = ");
                    qb.push_bind(value);
                }
                if let Some(values) = tags {
                    if has_assignment {
                        qb.push(", ");
                    }
                    has_assignment = true;
                    qb.push("tags_json = ");
                    qb.push_bind(serde_json::to_string(&values).unwrap_or_else(|_| "[]".to_string()));
                }
                if let Some(value) = due_at {
                    if has_assignment {
                        qb.push(", ");
                    }
                    has_assignment = true;
                    match value {
                        Some(due_at) => {
                            qb.push("due_at = ");
                            qb.push_bind(due_at);
                        }
                        None => {
                            qb.push("due_at = NULL");
                        }
                    }
                }
                if let Some(value) = outcome_summary {
                    if has_assignment {
                        qb.push(", ");
                    }
                    has_assignment = true;
                    qb.push("outcome_summary = ");
                    qb.push_bind(value);
                }
                if let Some(values) = outcome_items {
                    if has_assignment {
                        qb.push(", ");
                    }
                    has_assignment = true;
                    qb.push("outcome_items_json = ");
                    qb.push_bind(
                        serde_json::to_string(&values).unwrap_or_else(|_| "[]".to_string()),
                    );
                }
                if let Some(value) = resume_hint {
                    if has_assignment {
                        qb.push(", ");
                    }
                    has_assignment = true;
                    qb.push("resume_hint = ");
                    qb.push_bind(value);
                }
                if let Some(value) = blocker_reason {
                    if has_assignment {
                        qb.push(", ");
                    }
                    has_assignment = true;
                    qb.push("blocker_reason = ");
                    qb.push_bind(value);
                }
                if let Some(values) = blocker_needs {
                    if has_assignment {
                        qb.push(", ");
                    }
                    has_assignment = true;
                    qb.push("blocker_needs_json = ");
                    qb.push_bind(
                        serde_json::to_string(&values).unwrap_or_else(|_| "[]".to_string()),
                    );
                }
                if let Some(value) = blocker_kind {
                    if has_assignment {
                        qb.push(", ");
                    }
                    has_assignment = true;
                    qb.push("blocker_kind = ");
                    qb.push_bind(value);
                }
                if let Some(value) = completed_at {
                    if has_assignment {
                        qb.push(", ");
                    }
                    has_assignment = true;
                    match value {
                        Some(completed_at) => {
                            qb.push("completed_at = ");
                            qb.push_bind(completed_at);
                        }
                        None => {
                            qb.push("completed_at = NULL");
                        }
                    }
                }
                if let Some(value) = last_outcome_at {
                    if has_assignment {
                        qb.push(", ");
                    }
                    has_assignment = true;
                    match value {
                        Some(last_outcome_at) => {
                            qb.push("last_outcome_at = ");
                            qb.push_bind(last_outcome_at);
                        }
                        None => {
                            qb.push("last_outcome_at = NULL");
                        }
                    }
                }

                if has_assignment {
                    qb.push(", ");
                }
                qb.push("updated_at = ");
                qb.push_bind(updated_at);

                qb.push(" WHERE conversation_id = ");
                qb.push_bind(&conversation_id);
                qb.push(" AND id = ");
                qb.push_bind(&task_id);

                let result = qb.build().execute(pool).await.map_err(|err| err.to_string())?;

                if result.rows_affected() == 0 {
                    return Err(TASK_NOT_FOUND_ERR.to_string());
                }

                let row = sqlx::query_as::<_, TaskRow>(
                    "SELECT id, conversation_id, conversation_turn_id, title, details, priority, status, tags_json, due_at, outcome_summary, outcome_items_json, resume_hint, blocker_reason, blocker_needs_json, blocker_kind, completed_at, last_outcome_at, created_at, updated_at FROM task_manager_tasks WHERE conversation_id = ? AND id = ? LIMIT 1",
                )
                .bind(&conversation_id)
                .bind(&task_id)
                .fetch_optional(pool)
                .await
                .map_err(|err| err.to_string())?
                .ok_or_else(|| TASK_NOT_FOUND_ERR.to_string())?;

                Ok(row.into_record())
            })
        },
    )
    .await
}

pub async fn complete_task_by_id(
    conversation_id: &str,
    task_id: &str,
    patch: Option<TaskUpdatePatch>,
) -> Result<TaskRecord, String> {
    let mut patch = patch.unwrap_or_default();
    patch.status = Some("done".to_string());
    update_task_by_id(conversation_id, task_id, patch).await
}

pub async fn delete_task_by_id(conversation_id: &str, task_id: &str) -> Result<bool, String> {
    let conversation_id = trimmed_non_empty(conversation_id)
        .ok_or_else(|| "conversation_id is required".to_string())?
        .to_string();
    let task_id = trimmed_non_empty(task_id)
        .ok_or_else(|| "task_id is required".to_string())?
        .to_string();

    let conversation_id_for_mongo = conversation_id.clone();
    let task_id_for_mongo = task_id.clone();
    let conversation_id_for_sqlite = conversation_id.clone();
    let task_id_for_sqlite = task_id.clone();

    with_db(
        move |db| {
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
        },
        move |pool| {
            let conversation_id = conversation_id_for_sqlite.clone();
            let task_id = task_id_for_sqlite.clone();
            Box::pin(async move {
                let result = sqlx::query(
                    "DELETE FROM task_manager_tasks WHERE conversation_id = ? AND id = ?",
                )
                .bind(conversation_id)
                .bind(task_id)
                .execute(pool)
                .await
                .map_err(|err| err.to_string())?;
                Ok(result.rows_affected() > 0)
            })
        },
    )
    .await
}
