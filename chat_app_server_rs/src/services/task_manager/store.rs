use mongodb::bson::{doc, Bson, Document};
use mongodb::options::{FindOneAndUpdateOptions, FindOptions, ReturnDocument};
use sqlx::{FromRow, QueryBuilder, Sqlite};
use uuid::Uuid;

use crate::repositories::db::with_db;

use super::mapper::{task_record_from_doc, task_record_to_doc};
use super::normalizer::{normalize_task_drafts, parse_tags_json, trimmed_non_empty};
use super::types::{TaskDraft, TaskRecord, TaskUpdatePatch, TASK_NOT_FOUND_ERR};

#[derive(Debug, Clone, FromRow)]
struct TaskRow {
    id: String,
    session_id: String,
    conversation_turn_id: String,
    title: String,
    details: String,
    priority: String,
    status: String,
    tags_json: String,
    due_at: Option<String>,
    created_at: String,
    updated_at: String,
}

impl TaskRow {
    fn into_record(self) -> TaskRecord {
        TaskRecord {
            id: self.id,
            session_id: self.session_id,
            conversation_turn_id: self.conversation_turn_id,
            title: self.title,
            details: self.details,
            priority: self.priority,
            status: self.status,
            tags: parse_tags_json(self.tags_json.as_str()),
            due_at: self.due_at,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

pub async fn create_tasks_for_turn(
    session_id: &str,
    conversation_turn_id: &str,
    draft_tasks: Vec<TaskDraft>,
) -> Result<Vec<TaskRecord>, String> {
    let session_id = trimmed_non_empty(session_id)
        .ok_or_else(|| "session_id is required".to_string())?
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
            session_id: session_id.clone(),
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
                        "INSERT INTO task_manager_tasks (id, session_id, conversation_turn_id, title, details, priority, status, tags_json, due_at, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    )
                    .bind(&task.id)
                    .bind(&task.session_id)
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

pub async fn list_tasks_for_context(
    session_id: &str,
    conversation_turn_id: Option<&str>,
    include_done: bool,
    limit: usize,
) -> Result<Vec<TaskRecord>, String> {
    let session_id = trimmed_non_empty(session_id)
        .ok_or_else(|| "session_id is required".to_string())?
        .to_string();
    let conversation_turn_id = conversation_turn_id
        .and_then(trimmed_non_empty)
        .map(|value| value.to_string());
    let limit = limit.clamp(1, 200) as i64;
    let session_id_for_mongo = session_id.clone();
    let conversation_turn_id_for_mongo = conversation_turn_id.clone();
    let session_id_for_sqlite = session_id.clone();
    let conversation_turn_id_for_sqlite = conversation_turn_id.clone();

    with_db(
        move |db| {
            let session_id = session_id_for_mongo.clone();
            let conversation_turn_id = conversation_turn_id_for_mongo.clone();
            Box::pin(async move {
                let mut filter = doc! { "session_id": session_id };
                if let Some(turn_id) = conversation_turn_id {
                    filter.insert("conversation_turn_id", Bson::String(turn_id));
                }
                if !include_done {
                    filter.insert("status", doc! { "$ne": "done" });
                }

                let find_options = FindOptions::builder()
                    .sort(doc! { "created_at": -1 })
                    .limit(limit)
                    .build();
                let mut cursor = db
                    .collection::<Document>("task_manager_tasks")
                    .find(filter, find_options)
                    .await
                    .map_err(|err| err.to_string())?;

                let mut out = Vec::new();
                while cursor.advance().await.map_err(|err| err.to_string())? {
                    let document = cursor.deserialize_current().map_err(|err| err.to_string())?;
                    if let Some(task) = task_record_from_doc(&document) {
                        out.push(task);
                    }
                }
                Ok(out)
            })
        },
        move |pool| {
            let session_id = session_id_for_sqlite.clone();
            let conversation_turn_id = conversation_turn_id_for_sqlite.clone();
            Box::pin(async move {
                let mut qb = QueryBuilder::<Sqlite>::new(
                    "SELECT id, session_id, conversation_turn_id, title, details, priority, status, tags_json, due_at, created_at, updated_at FROM task_manager_tasks WHERE session_id = ",
                );
                qb.push_bind(session_id);
                if let Some(turn_id) = conversation_turn_id {
                    qb.push(" AND conversation_turn_id = ");
                    qb.push_bind(turn_id);
                }
                if !include_done {
                    qb.push(" AND status != ");
                    qb.push_bind("done");
                }
                qb.push(" ORDER BY created_at DESC LIMIT ");
                qb.push_bind(limit);

                let rows: Vec<TaskRow> = qb
                    .build_query_as()
                    .fetch_all(pool)
                    .await
                    .map_err(|err| err.to_string())?;

                Ok(rows.into_iter().map(TaskRow::into_record).collect())
            })
        },
    )
    .await
}

pub async fn update_task_by_id(
    session_id: &str,
    task_id: &str,
    patch: TaskUpdatePatch,
) -> Result<TaskRecord, String> {
    let session_id = trimmed_non_empty(session_id)
        .ok_or_else(|| "session_id is required".to_string())?
        .to_string();
    let task_id = trimmed_non_empty(task_id)
        .ok_or_else(|| "task_id is required".to_string())?
        .to_string();

    let patch = patch.normalized()?;
    if patch.is_empty() {
        return Err("at least one task field is required".to_string());
    }

    let updated_at = crate::core::time::now_rfc3339();

    let session_id_for_mongo = session_id.clone();
    let task_id_for_mongo = task_id.clone();
    let title_for_mongo = patch.title.clone();
    let details_for_mongo = patch.details.clone();
    let priority_for_mongo = patch.priority.clone();
    let status_for_mongo = patch.status.clone();
    let tags_for_mongo = patch.tags.clone();
    let due_at_for_mongo = patch.due_at.clone();
    let updated_at_for_mongo = updated_at.clone();

    let session_id_for_sqlite = session_id.clone();
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
            let session_id = session_id_for_mongo.clone();
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
                        doc! { "session_id": session_id, "id": task_id },
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
            let session_id = session_id_for_sqlite.clone();
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

                qb.push(" WHERE session_id = ");
                qb.push_bind(&session_id);
                qb.push(" AND id = ");
                qb.push_bind(&task_id);

                let result = qb.build().execute(pool).await.map_err(|err| err.to_string())?;

                if result.rows_affected() == 0 {
                    return Err(TASK_NOT_FOUND_ERR.to_string());
                }

                let row = sqlx::query_as::<_, TaskRow>(
                    "SELECT id, session_id, conversation_turn_id, title, details, priority, status, tags_json, due_at, created_at, updated_at FROM task_manager_tasks WHERE session_id = ? AND id = ? LIMIT 1",
                )
                .bind(&session_id)
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

pub async fn complete_task_by_id(session_id: &str, task_id: &str) -> Result<TaskRecord, String> {
    update_task_by_id(
        session_id,
        task_id,
        TaskUpdatePatch {
            status: Some("done".to_string()),
            ..TaskUpdatePatch::default()
        },
    )
    .await
}

pub async fn delete_task_by_id(session_id: &str, task_id: &str) -> Result<bool, String> {
    let session_id = trimmed_non_empty(session_id)
        .ok_or_else(|| "session_id is required".to_string())?
        .to_string();
    let task_id = trimmed_non_empty(task_id)
        .ok_or_else(|| "task_id is required".to_string())?
        .to_string();

    let session_id_for_mongo = session_id.clone();
    let task_id_for_mongo = task_id.clone();
    let session_id_for_sqlite = session_id.clone();
    let task_id_for_sqlite = task_id.clone();

    with_db(
        move |db| {
            let session_id = session_id_for_mongo.clone();
            let task_id = task_id_for_mongo.clone();
            Box::pin(async move {
                let result = db
                    .collection::<Document>("task_manager_tasks")
                    .delete_one(doc! { "session_id": session_id, "id": task_id }, None)
                    .await
                    .map_err(|err| err.to_string())?;
                Ok(result.deleted_count > 0)
            })
        },
        move |pool| {
            let session_id = session_id_for_sqlite.clone();
            let task_id = task_id_for_sqlite.clone();
            Box::pin(async move {
                let result =
                    sqlx::query("DELETE FROM task_manager_tasks WHERE session_id = ? AND id = ?")
                        .bind(session_id)
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
