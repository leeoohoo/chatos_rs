use mongodb::bson::{doc, Bson, Document};
use mongodb::options::{FindOneAndUpdateOptions, ReturnDocument};
use sqlx::{QueryBuilder, Sqlite};

use crate::repositories::db::with_db;
use crate::services::task_manager::mapper::task_record_from_doc;
use crate::services::task_manager::normalizer::trimmed_non_empty;
use crate::services::task_manager::types::{TaskRecord, TaskUpdatePatch, TASK_NOT_FOUND_ERR};

use super::row::TaskRow;

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

    let updated_at = crate::core::time::now_rfc3339();

    let conversation_id_for_mongo = conversation_id.clone();
    let task_id_for_mongo = task_id.clone();
    let title_for_mongo = patch.title.clone();
    let details_for_mongo = patch.details.clone();
    let priority_for_mongo = patch.priority.clone();
    let status_for_mongo = patch.status.clone();
    let tags_for_mongo = patch.tags.clone();
    let due_at_for_mongo = patch.due_at.clone();
    let updated_at_for_mongo = updated_at.clone();

    let conversation_id_for_sqlite = conversation_id.clone();
    let task_id_for_sqlite = task_id.clone();
    let title_for_sqlite = patch.title.clone();
    let details_for_sqlite = patch.details.clone();
    let priority_for_sqlite = patch.priority.clone();
    let status_for_sqlite = patch.status.clone();
    let tags_for_sqlite = patch.tags.clone();
    let due_at_for_sqlite = patch.due_at.clone();
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
                    "SELECT id, conversation_id, conversation_turn_id, title, details, priority, status, tags_json, due_at, created_at, updated_at FROM task_manager_tasks WHERE conversation_id = ? AND id = ? LIMIT 1",
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
) -> Result<TaskRecord, String> {
    update_task_by_id(
        conversation_id,
        task_id,
        TaskUpdatePatch {
            status: Some("done".to_string()),
            ..TaskUpdatePatch::default()
        },
    )
    .await
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
