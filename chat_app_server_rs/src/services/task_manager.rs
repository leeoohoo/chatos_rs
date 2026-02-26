use std::collections::HashMap;
use std::time::Duration;

use mongodb::bson::{doc, Bson, Document};
use mongodb::options::{FindOneAndUpdateOptions, FindOptions, ReturnDocument};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, QueryBuilder, Sqlite};
use tokio::sync::{oneshot, Mutex};
use uuid::Uuid;

use crate::repositories::db::with_db;

pub const REVIEW_TIMEOUT_MS_DEFAULT: u64 = 86_400_000;
pub const REVIEW_TIMEOUT_ERR: &str = "review_timeout";
pub const REVIEW_NOT_FOUND_ERR: &str = "review_not_found";
pub const TASK_NOT_FOUND_ERR: &str = "task_not_found";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDraft {
    pub title: String,
    #[serde(default)]
    pub details: String,
    #[serde(default = "default_priority")]
    pub priority: String,
    #[serde(default = "default_status")]
    pub status: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub due_at: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskUpdatePatch {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub details: Option<String>,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub due_at: Option<Option<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRecord {
    pub id: String,
    pub session_id: String,
    pub conversation_turn_id: String,
    pub title: String,
    pub details: String,
    pub priority: String,
    pub status: String,
    pub tags: Vec<String>,
    pub due_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCreateReviewPayload {
    pub review_id: String,
    pub session_id: String,
    pub conversation_turn_id: String,
    pub draft_tasks: Vec<TaskDraft>,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TaskReviewAction {
    Confirm,
    Cancel,
}

impl TaskReviewAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Confirm => "confirm",
            Self::Cancel => "cancel",
        }
    }
}

#[derive(Debug, Clone)]
pub struct TaskReviewDecision {
    pub action: TaskReviewAction,
    pub tasks: Vec<TaskDraft>,
    pub reason: Option<String>,
}

#[derive(Debug)]
struct PendingReviewEntry {
    payload: TaskCreateReviewPayload,
    sender: oneshot::Sender<TaskReviewDecision>,
}

#[derive(Debug, Default)]
struct TaskReviewHub {
    pending: Mutex<HashMap<String, PendingReviewEntry>>,
}

impl TaskReviewHub {
    async fn register(
        &self,
        payload: TaskCreateReviewPayload,
    ) -> oneshot::Receiver<TaskReviewDecision> {
        let review_id = payload.review_id.clone();
        let (sender, receiver) = oneshot::channel();
        let mut pending = self.pending.lock().await;
        pending.insert(review_id, PendingReviewEntry { payload, sender });
        receiver
    }

    async fn resolve(
        &self,
        review_id: &str,
        action: TaskReviewAction,
        tasks: Option<Vec<TaskDraft>>,
        reason: Option<String>,
    ) -> Result<TaskCreateReviewPayload, String> {
        let entry = {
            let mut pending = self.pending.lock().await;
            pending.remove(review_id)
        }
        .ok_or_else(|| REVIEW_NOT_FOUND_ERR.to_string())?;

        let resolved_tasks = match action {
            TaskReviewAction::Confirm => {
                let source_tasks = tasks.unwrap_or_else(|| entry.payload.draft_tasks.clone());
                let normalized = normalize_task_drafts(source_tasks)?;
                if normalized.is_empty() {
                    return Err("tasks is required for confirm action".to_string());
                }
                normalized
            }
            TaskReviewAction::Cancel => Vec::new(),
        };

        entry
            .sender
            .send(TaskReviewDecision {
                action,
                tasks: resolved_tasks,
                reason,
            })
            .map_err(|_| "review_listener_closed".to_string())?;

        Ok(entry.payload)
    }

    async fn remove(&self, review_id: &str) {
        let mut pending = self.pending.lock().await;
        pending.remove(review_id);
    }
}

static TASK_REVIEW_HUB: Lazy<TaskReviewHub> = Lazy::new(TaskReviewHub::default);

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

impl TaskUpdatePatch {
    fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.details.is_none()
            && self.priority.is_none()
            && self.status.is_none()
            && self.tags.is_none()
            && self.due_at.is_none()
    }

    fn normalized(mut self) -> Result<Self, String> {
        if let Some(title) = self.title.take() {
            let title = title.trim().to_string();
            if title.is_empty() {
                return Err("task title is required".to_string());
            }
            self.title = Some(title);
        }

        if let Some(details) = self.details.take() {
            self.details = Some(details.trim().to_string());
        }

        if let Some(priority) = self.priority.take() {
            self.priority = Some(normalize_priority(priority.as_str()));
        }

        if let Some(status) = self.status.take() {
            self.status = Some(normalize_status(status.as_str()));
        }

        if let Some(tags) = self.tags.take() {
            self.tags = Some(normalize_tags(tags));
        }

        if let Some(due_at) = self.due_at.take() {
            let normalized = due_at
                .as_deref()
                .and_then(trimmed_non_empty)
                .map(|value| value.to_string());
            self.due_at = Some(normalized);
        }

        Ok(self)
    }
}

pub async fn create_task_review(
    session_id: &str,
    conversation_turn_id: &str,
    draft_tasks: Vec<TaskDraft>,
    timeout_ms: u64,
) -> Result<
    (
        TaskCreateReviewPayload,
        oneshot::Receiver<TaskReviewDecision>,
    ),
    String,
> {
    let session_id = trimmed_non_empty(session_id)
        .ok_or_else(|| "session_id is required for task review".to_string())?
        .to_string();
    let conversation_turn_id = trimmed_non_empty(conversation_turn_id)
        .ok_or_else(|| "conversation_turn_id is required for task review".to_string())?
        .to_string();

    let draft_tasks = normalize_task_drafts(draft_tasks)?;
    if draft_tasks.is_empty() {
        return Err("at least one draft task is required".to_string());
    }

    let timeout_ms = timeout_ms.clamp(10_000, REVIEW_TIMEOUT_MS_DEFAULT);
    let payload = TaskCreateReviewPayload {
        review_id: format!("rev_{}", Uuid::new_v4().simple()),
        session_id,
        conversation_turn_id,
        draft_tasks,
        timeout_ms,
    };
    let receiver = TASK_REVIEW_HUB.register(payload.clone()).await;
    Ok((payload, receiver))
}

pub async fn wait_for_task_review_decision(
    review_id: &str,
    receiver: oneshot::Receiver<TaskReviewDecision>,
    timeout_ms: u64,
) -> Result<TaskReviewDecision, String> {
    let bounded_timeout = timeout_ms.clamp(1_000, REVIEW_TIMEOUT_MS_DEFAULT);
    match tokio::time::timeout(Duration::from_millis(bounded_timeout), receiver).await {
        Ok(Ok(decision)) => Ok(decision),
        Ok(Err(_)) => Err("review_listener_closed".to_string()),
        Err(_) => {
            TASK_REVIEW_HUB.remove(review_id).await;
            Err(REVIEW_TIMEOUT_ERR.to_string())
        }
    }
}

pub async fn submit_task_review_decision(
    review_id: &str,
    action: TaskReviewAction,
    tasks: Option<Vec<TaskDraft>>,
    reason: Option<String>,
) -> Result<TaskCreateReviewPayload, String> {
    let review_id =
        trimmed_non_empty(review_id).ok_or_else(|| "review_id is required".to_string())?;
    TASK_REVIEW_HUB
        .resolve(review_id, action, tasks, reason)
        .await
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
                    let tags_json = serde_json::to_string(&task.tags).unwrap_or_else(|_| "[]".to_string());
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
                    qb.push_bind(
                        serde_json::to_string(&values).unwrap_or_else(|_| "[]".to_string()),
                    );
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

                let result = qb
                    .build()
                    .execute(pool)
                    .await
                    .map_err(|err| err.to_string())?;

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

fn normalize_task_drafts(drafts: Vec<TaskDraft>) -> Result<Vec<TaskDraft>, String> {
    let mut out = Vec::new();
    for draft in drafts {
        out.push(normalize_task_draft(draft)?);
    }
    Ok(out)
}

fn normalize_task_draft(mut draft: TaskDraft) -> Result<TaskDraft, String> {
    draft.title = draft.title.trim().to_string();
    if draft.title.is_empty() {
        return Err("task title is required".to_string());
    }
    draft.details = draft.details.trim().to_string();
    draft.priority = normalize_priority(draft.priority.as_str());
    draft.status = normalize_status(draft.status.as_str());
    draft.tags = normalize_tags(draft.tags);
    draft.due_at = draft
        .due_at
        .as_deref()
        .and_then(trimmed_non_empty)
        .map(|value| value.to_string());
    Ok(draft)
}

fn normalize_priority(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "high" => "high".to_string(),
        "low" => "low".to_string(),
        _ => "medium".to_string(),
    }
}

fn normalize_status(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "doing" => "doing".to_string(),
        "blocked" => "blocked".to_string(),
        "done" => "done".to_string(),
        _ => "todo".to_string(),
    }
}

fn normalize_tags(tags: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    for tag in tags {
        let trimmed = tag.trim();
        if trimmed.is_empty() {
            continue;
        }
        if out.iter().any(|existing: &String| existing == trimmed) {
            continue;
        }
        out.push(trimmed.to_string());
    }
    out
}

fn parse_tags_json(raw: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(raw)
        .ok()
        .map(normalize_tags)
        .unwrap_or_default()
}

fn task_record_to_doc(task: &TaskRecord) -> Document {
    let tags = task
        .tags
        .iter()
        .cloned()
        .map(Bson::String)
        .collect::<Vec<Bson>>();

    let mut doc = doc! {
        "id": task.id.clone(),
        "session_id": task.session_id.clone(),
        "conversation_turn_id": task.conversation_turn_id.clone(),
        "title": task.title.clone(),
        "details": task.details.clone(),
        "priority": task.priority.clone(),
        "status": task.status.clone(),
        "tags": Bson::Array(tags),
        "created_at": task.created_at.clone(),
        "updated_at": task.updated_at.clone(),
    };
    if let Some(due_at) = task.due_at.clone() {
        doc.insert("due_at", Bson::String(due_at));
    }
    doc
}

fn task_record_from_doc(doc: &Document) -> Option<TaskRecord> {
    let id = doc.get_str("id").ok()?.to_string();
    let session_id = doc.get_str("session_id").ok()?.to_string();
    let conversation_turn_id = doc.get_str("conversation_turn_id").ok()?.to_string();
    let title = doc.get_str("title").ok()?.to_string();
    let details = doc.get_str("details").ok().unwrap_or_default().to_string();
    let priority = doc.get_str("priority").ok().unwrap_or("medium").to_string();
    let status = doc.get_str("status").ok().unwrap_or("todo").to_string();
    let created_at = doc
        .get_str("created_at")
        .ok()
        .unwrap_or_default()
        .to_string();
    let updated_at = doc
        .get_str("updated_at")
        .ok()
        .unwrap_or_default()
        .to_string();

    let tags = match doc.get("tags") {
        Some(Bson::Array(arr)) => arr
            .iter()
            .filter_map(|value| value.as_str().map(|item| item.to_string()))
            .collect::<Vec<String>>(),
        Some(Bson::String(raw)) => parse_tags_json(raw),
        _ => Vec::new(),
    };

    let due_at = doc
        .get_str("due_at")
        .ok()
        .and_then(trimmed_non_empty)
        .map(|value| value.to_string());

    Some(TaskRecord {
        id,
        session_id,
        conversation_turn_id,
        title,
        details,
        priority: normalize_priority(priority.as_str()),
        status: normalize_status(status.as_str()),
        tags: normalize_tags(tags),
        due_at,
        created_at,
        updated_at,
    })
}

fn trimmed_non_empty(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn default_priority() -> String {
    "medium".to_string()
}

fn default_status() -> String {
    "todo".to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        create_task_review, normalize_task_draft, submit_task_review_decision,
        wait_for_task_review_decision, TaskDraft, TaskReviewAction, TaskUpdatePatch,
    };

    #[test]
    fn normalize_task_draft_applies_defaults() {
        let draft = TaskDraft {
            title: "  Build review panel  ".to_string(),
            details: "  Some details  ".to_string(),
            priority: "unknown".to_string(),
            status: "invalid".to_string(),
            tags: vec![" ui ".to_string(), "ui".to_string(), "".to_string()],
            due_at: Some("  ".to_string()),
        };

        let normalized = normalize_task_draft(draft).expect("normalize should succeed");
        assert_eq!(normalized.title, "Build review panel");
        assert_eq!(normalized.details, "Some details");
        assert_eq!(normalized.priority, "medium");
        assert_eq!(normalized.status, "todo");
        assert_eq!(normalized.tags, vec!["ui"]);
        assert_eq!(normalized.due_at, None);
    }

    #[test]
    fn normalize_update_patch_applies_defaults() {
        let patch = TaskUpdatePatch {
            title: Some("  Refine workbar  ".to_string()),
            details: Some("  trim me  ".to_string()),
            priority: Some("unknown".to_string()),
            status: Some("invalid".to_string()),
            tags: Some(vec![" ui ".to_string(), "ui".to_string(), "".to_string()]),
            due_at: Some(Some("  ".to_string())),
        };

        let normalized = patch.normalized().expect("patch normalize should succeed");
        assert_eq!(normalized.title.as_deref(), Some("Refine workbar"));
        assert_eq!(normalized.details.as_deref(), Some("trim me"));
        assert_eq!(normalized.priority.as_deref(), Some("medium"));
        assert_eq!(normalized.status.as_deref(), Some("todo"));
        assert_eq!(normalized.tags, Some(vec!["ui".to_string()]));
        assert_eq!(normalized.due_at, Some(None));
    }

    #[tokio::test]
    async fn review_confirm_flow_returns_updated_tasks() {
        let draft = TaskDraft {
            title: "Initial task".to_string(),
            details: "detail".to_string(),
            priority: "medium".to_string(),
            status: "todo".to_string(),
            tags: vec!["one".to_string()],
            due_at: None,
        };

        let (payload, receiver) =
            create_task_review("session_test", "turn_test", vec![draft], 30_000)
                .await
                .expect("create review should succeed");

        let updated_tasks = vec![TaskDraft {
            title: "Updated task".to_string(),
            details: "updated".to_string(),
            priority: "high".to_string(),
            status: "doing".to_string(),
            tags: vec!["backend".to_string()],
            due_at: Some("2026-03-01T10:00:00Z".to_string()),
        }];

        submit_task_review_decision(
            payload.review_id.as_str(),
            TaskReviewAction::Confirm,
            Some(updated_tasks.clone()),
            None,
        )
        .await
        .expect("submit decision should succeed");

        let decision = wait_for_task_review_decision(payload.review_id.as_str(), receiver, 5_000)
            .await
            .expect("wait decision should succeed");

        assert_eq!(decision.action, TaskReviewAction::Confirm);
        assert_eq!(decision.tasks.len(), 1);
        assert_eq!(decision.tasks[0].title, "Updated task");
        assert_eq!(decision.tasks[0].priority, "high");
        assert_eq!(decision.tasks[0].status, "doing");
    }

    #[tokio::test]
    async fn review_cancel_flow_returns_cancel_action() {
        let draft = TaskDraft {
            title: "Cancel me".to_string(),
            details: String::new(),
            priority: "medium".to_string(),
            status: "todo".to_string(),
            tags: Vec::new(),
            due_at: None,
        };

        let (payload, receiver) =
            create_task_review("session_test", "turn_cancel", vec![draft], 30_000)
                .await
                .expect("create review should succeed");

        submit_task_review_decision(
            payload.review_id.as_str(),
            TaskReviewAction::Cancel,
            None,
            Some("user_cancelled".to_string()),
        )
        .await
        .expect("cancel decision should succeed");

        let decision = wait_for_task_review_decision(payload.review_id.as_str(), receiver, 5_000)
            .await
            .expect("wait decision should succeed");

        assert_eq!(decision.action, TaskReviewAction::Cancel);
        assert!(decision.tasks.is_empty());
        assert_eq!(decision.reason.as_deref(), Some("user_cancelled"));
    }
}
