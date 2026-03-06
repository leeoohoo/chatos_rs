use chrono::{Duration, Utc};
use mongodb::bson::{doc, Bson, Document};
use mongodb::options::{FindOneAndUpdateOptions, FindOptions, ReturnDocument, UpdateOptions};
use serde_json::json;
use sqlx::{FromRow, QueryBuilder, Sqlite};

use crate::repositories::db::with_db;

use super::normalizer::{redact_prompt_payload, trimmed_non_empty};
use super::types::{UiPromptPayload, UiPromptRecord, UiPromptStatus, UI_PROMPT_NOT_FOUND_ERR};

#[derive(Debug, Clone, FromRow)]
struct UiPromptRow {
    id: String,
    session_id: String,
    conversation_turn_id: String,
    tool_call_id: Option<String>,
    kind: String,
    status: String,
    prompt_json: String,
    response_json: Option<String>,
    expires_at: Option<String>,
    created_at: String,
    updated_at: String,
}

impl UiPromptRow {
    fn into_record(self) -> UiPromptRecord {
        UiPromptRecord {
            id: self.id,
            session_id: self.session_id,
            conversation_turn_id: self.conversation_turn_id,
            tool_call_id: self.tool_call_id,
            kind: self.kind,
            status: parse_status(self.status.as_str()),
            prompt: parse_json_or_default(self.prompt_json.as_str()),
            response: self.response_json.as_deref().map(parse_json_or_default),
            expires_at: self.expires_at,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

pub async fn create_ui_prompt_record(payload: &UiPromptPayload) -> Result<UiPromptRecord, String> {
    let id = trimmed_non_empty(payload.prompt_id.as_str())
        .ok_or_else(|| "prompt_id is required".to_string())?
        .to_string();
    let session_id = trimmed_non_empty(payload.session_id.as_str())
        .ok_or_else(|| "session_id is required".to_string())?
        .to_string();
    let conversation_turn_id = trimmed_non_empty(payload.conversation_turn_id.as_str())
        .ok_or_else(|| "conversation_turn_id is required".to_string())?
        .to_string();
    let kind = trimmed_non_empty(payload.kind.as_str())
        .ok_or_else(|| "kind is required".to_string())?
        .to_string();

    let now = crate::core::time::now_rfc3339();
    let expires_at = Some(
        (Utc::now()
            + Duration::milliseconds(payload.timeout_ms.clamp(1_000, i32::MAX as u64) as i64))
        .to_rfc3339(),
    );

    let record = UiPromptRecord {
        id,
        session_id,
        conversation_turn_id,
        tool_call_id: payload
            .tool_call_id
            .as_deref()
            .and_then(trimmed_non_empty)
            .map(|value| value.to_string()),
        kind,
        status: UiPromptStatus::Pending,
        prompt: redact_prompt_payload(payload),
        response: None,
        expires_at,
        created_at: now.clone(),
        updated_at: now,
    };

    let mongo_record = record.clone();
    let sqlite_record = record.clone();

    with_db(
        move |db| {
            let record = mongo_record.clone();
            Box::pin(async move {
                let update_options = UpdateOptions::builder().upsert(true).build();
                db.collection::<Document>("ui_prompt_requests")
                    .update_one(
                        doc! { "id": record.id.clone() },
                        doc! { "$set": ui_prompt_record_to_doc(&record) },
                        update_options,
                    )
                    .await
                    .map_err(|err| err.to_string())?;
                Ok(record)
            })
        },
        move |pool| {
            let record = sqlite_record.clone();
            Box::pin(async move {
                let prompt_json =
                    serde_json::to_string(&record.prompt).unwrap_or_else(|_| "{}".to_string());

                sqlx::query(
                    "INSERT INTO ui_prompt_requests (id, session_id, conversation_turn_id, tool_call_id, kind, status, prompt_json, response_json, expires_at, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET session_id = excluded.session_id, conversation_turn_id = excluded.conversation_turn_id, tool_call_id = excluded.tool_call_id, kind = excluded.kind, status = excluded.status, prompt_json = excluded.prompt_json, response_json = excluded.response_json, expires_at = excluded.expires_at, updated_at = excluded.updated_at",
                )
                .bind(&record.id)
                .bind(&record.session_id)
                .bind(&record.conversation_turn_id)
                .bind(&record.tool_call_id)
                .bind(&record.kind)
                .bind(record.status.as_str())
                .bind(prompt_json)
                .bind(Option::<String>::None)
                .bind(&record.expires_at)
                .bind(&record.created_at)
                .bind(&record.updated_at)
                .execute(pool)
                .await
                .map_err(|err| err.to_string())?;

                Ok(record)
            })
        },
    )
    .await
}

pub async fn update_ui_prompt_response(
    prompt_id: &str,
    status: UiPromptStatus,
    response: Option<serde_json::Value>,
) -> Result<UiPromptRecord, String> {
    let prompt_id = trimmed_non_empty(prompt_id)
        .ok_or_else(|| "prompt_id is required".to_string())?
        .to_string();
    let updated_at = crate::core::time::now_rfc3339();

    let status_raw = status.as_str().to_string();
    let response_json = response
        .as_ref()
        .and_then(|value| serde_json::to_string(value).ok());

    let prompt_id_for_mongo = prompt_id.clone();
    let prompt_id_for_sqlite = prompt_id.clone();
    let status_for_mongo = status_raw.clone();
    let status_for_sqlite = status_raw.clone();
    let response_for_mongo = response_json.clone();
    let response_for_sqlite = response_json.clone();
    let updated_at_for_mongo = updated_at.clone();
    let updated_at_for_sqlite = updated_at.clone();

    with_db(
        move |db| {
            let prompt_id = prompt_id_for_mongo.clone();
            let status = status_for_mongo.clone();
            let response_json = response_for_mongo.clone();
            let updated_at = updated_at_for_mongo.clone();
            Box::pin(async move {
                let mut set_doc = doc! {
                    "status": status,
                    "updated_at": updated_at,
                };
                match response_json {
                    Some(raw) => {
                        set_doc.insert("response_json", Bson::String(raw));
                    }
                    None => {
                        set_doc.insert("response_json", Bson::Null);
                    }
                }

                let options = FindOneAndUpdateOptions::builder()
                    .return_document(ReturnDocument::After)
                    .build();

                let updated = db
                    .collection::<Document>("ui_prompt_requests")
                    .find_one_and_update(doc! { "id": prompt_id }, doc! { "$set": set_doc }, options)
                    .await
                    .map_err(|err| err.to_string())?
                    .and_then(|doc| ui_prompt_record_from_doc(&doc))
                    .ok_or_else(|| UI_PROMPT_NOT_FOUND_ERR.to_string())?;
                Ok(updated)
            })
        },
        move |pool| {
            let prompt_id = prompt_id_for_sqlite.clone();
            let status = status_for_sqlite.clone();
            let response_json = response_for_sqlite.clone();
            let updated_at = updated_at_for_sqlite.clone();
            Box::pin(async move {
                let result = sqlx::query(
                    "UPDATE ui_prompt_requests SET status = ?, response_json = ?, updated_at = ? WHERE id = ?",
                )
                .bind(status)
                .bind(response_json)
                .bind(updated_at)
                .bind(&prompt_id)
                .execute(pool)
                .await
                .map_err(|err| err.to_string())?;

                if result.rows_affected() == 0 {
                    return Err(UI_PROMPT_NOT_FOUND_ERR.to_string());
                }

                let row = sqlx::query_as::<_, UiPromptRow>(
                    "SELECT id, session_id, conversation_turn_id, tool_call_id, kind, status, prompt_json, response_json, expires_at, created_at, updated_at FROM ui_prompt_requests WHERE id = ? LIMIT 1",
                )
                .bind(&prompt_id)
                .fetch_optional(pool)
                .await
                .map_err(|err| err.to_string())?
                .ok_or_else(|| UI_PROMPT_NOT_FOUND_ERR.to_string())?;

                Ok(row.into_record())
            })
        },
    )
    .await
}

#[allow(dead_code)]
pub async fn get_ui_prompt_record_by_id(prompt_id: &str) -> Result<Option<UiPromptRecord>, String> {
    let prompt_id = trimmed_non_empty(prompt_id)
        .ok_or_else(|| "prompt_id is required".to_string())?
        .to_string();

    let prompt_id_for_mongo = prompt_id.clone();
    let prompt_id_for_sqlite = prompt_id.clone();

    with_db(
        move |db| {
            let prompt_id = prompt_id_for_mongo.clone();
            Box::pin(async move {
                let document = db
                    .collection::<Document>("ui_prompt_requests")
                    .find_one(doc! { "id": prompt_id }, None)
                    .await
                    .map_err(|err| err.to_string())?;
                Ok(document.and_then(|doc| ui_prompt_record_from_doc(&doc)))
            })
        },
        move |pool| {
            let prompt_id = prompt_id_for_sqlite.clone();
            Box::pin(async move {
                let row = sqlx::query_as::<_, UiPromptRow>(
                    "SELECT id, session_id, conversation_turn_id, tool_call_id, kind, status, prompt_json, response_json, expires_at, created_at, updated_at FROM ui_prompt_requests WHERE id = ? LIMIT 1",
                )
                .bind(prompt_id)
                .fetch_optional(pool)
                .await
                .map_err(|err| err.to_string())?;
                Ok(row.map(UiPromptRow::into_record))
            })
        },
    )
    .await
}

pub async fn list_pending_ui_prompt_records(
    session_id: &str,
    limit: usize,
) -> Result<Vec<UiPromptRecord>, String> {
    let session_id = trimmed_non_empty(session_id)
        .ok_or_else(|| "session_id is required".to_string())?
        .to_string();
    let limit = limit.clamp(1, 200) as i64;

    let session_id_for_mongo = session_id.clone();
    let session_id_for_sqlite = session_id.clone();

    with_db(
        move |db| {
            let session_id = session_id_for_mongo.clone();
            Box::pin(async move {
                let options = FindOptions::builder()
                    .sort(doc! { "created_at": 1 })
                    .limit(limit)
                    .build();
                let mut cursor = db
                    .collection::<Document>("ui_prompt_requests")
                    .find(
                        doc! {
                            "session_id": session_id,
                            "status": UiPromptStatus::Pending.as_str(),
                        },
                        options,
                    )
                    .await
                    .map_err(|err| err.to_string())?;

                let mut out = Vec::new();
                while cursor.advance().await.map_err(|err| err.to_string())? {
                    let document = cursor.deserialize_current().map_err(|err| err.to_string())?;
                    if let Some(record) = ui_prompt_record_from_doc(&document) {
                        out.push(record);
                    }
                }
                Ok(out)
            })
        },
        move |pool| {
            let session_id = session_id_for_sqlite.clone();
            Box::pin(async move {
                let mut qb = QueryBuilder::<Sqlite>::new(
                    "SELECT id, session_id, conversation_turn_id, tool_call_id, kind, status, prompt_json, response_json, expires_at, created_at, updated_at FROM ui_prompt_requests WHERE session_id = ",
                );
                qb.push_bind(session_id);
                qb.push(" AND status = ");
                qb.push_bind(UiPromptStatus::Pending.as_str());
                qb.push(" ORDER BY created_at ASC LIMIT ");
                qb.push_bind(limit);

                let rows: Vec<UiPromptRow> = qb
                    .build_query_as()
                    .fetch_all(pool)
                    .await
                    .map_err(|err| err.to_string())?;

                Ok(rows.into_iter().map(UiPromptRow::into_record).collect())
            })
        },
    )
    .await
}

pub async fn list_ui_prompt_history_records(
    session_id: &str,
    limit: usize,
    include_pending: bool,
) -> Result<Vec<UiPromptRecord>, String> {
    let session_id = trimmed_non_empty(session_id)
        .ok_or_else(|| "session_id is required".to_string())?
        .to_string();
    let limit = limit.clamp(1, 500) as i64;

    let session_id_for_mongo = session_id.clone();
    let session_id_for_sqlite = session_id.clone();

    with_db(
        move |db| {
            let session_id = session_id_for_mongo.clone();
            Box::pin(async move {
                let mut filter = doc! {
                    "session_id": session_id,
                };
                if !include_pending {
                    filter.insert(
                        "status",
                        doc! { "$ne": UiPromptStatus::Pending.as_str() },
                    );
                }

                let options = FindOptions::builder()
                    .sort(doc! { "updated_at": -1_i32, "created_at": -1_i32 })
                    .limit(limit)
                    .build();
                let mut cursor = db
                    .collection::<Document>("ui_prompt_requests")
                    .find(filter, options)
                    .await
                    .map_err(|err| err.to_string())?;

                let mut out = Vec::new();
                while cursor.advance().await.map_err(|err| err.to_string())? {
                    let document = cursor.deserialize_current().map_err(|err| err.to_string())?;
                    if let Some(record) = ui_prompt_record_from_doc(&document) {
                        out.push(record);
                    }
                }
                Ok(out)
            })
        },
        move |pool| {
            let session_id = session_id_for_sqlite.clone();
            Box::pin(async move {
                let mut qb = QueryBuilder::<Sqlite>::new(
                    "SELECT id, session_id, conversation_turn_id, tool_call_id, kind, status, prompt_json, response_json, expires_at, created_at, updated_at FROM ui_prompt_requests WHERE session_id = ",
                );
                qb.push_bind(session_id);
                if !include_pending {
                    qb.push(" AND status != ");
                    qb.push_bind(UiPromptStatus::Pending.as_str());
                }
                qb.push(" ORDER BY updated_at DESC, created_at DESC LIMIT ");
                qb.push_bind(limit);

                let rows: Vec<UiPromptRow> = qb
                    .build_query_as()
                    .fetch_all(pool)
                    .await
                    .map_err(|err| err.to_string())?;

                Ok(rows.into_iter().map(UiPromptRow::into_record).collect())
            })
        },
    )
    .await
}

fn ui_prompt_record_to_doc(record: &UiPromptRecord) -> Document {
    let prompt_json = serde_json::to_string(&record.prompt).unwrap_or_else(|_| "{}".to_string());
    let response_json = record
        .response
        .as_ref()
        .and_then(|value| serde_json::to_string(value).ok());

    let mut out = doc! {
        "id": record.id.clone(),
        "session_id": record.session_id.clone(),
        "conversation_turn_id": record.conversation_turn_id.clone(),
        "kind": record.kind.clone(),
        "status": record.status.as_str(),
        "prompt_json": prompt_json,
        "created_at": record.created_at.clone(),
        "updated_at": record.updated_at.clone(),
    };
    if let Some(value) = record.tool_call_id.clone() {
        out.insert("tool_call_id", Bson::String(value));
    }
    if let Some(value) = response_json {
        out.insert("response_json", Bson::String(value));
    }
    if let Some(value) = record.expires_at.clone() {
        out.insert("expires_at", Bson::String(value));
    }
    out
}

fn ui_prompt_record_from_doc(doc: &Document) -> Option<UiPromptRecord> {
    let id = doc.get_str("id").ok()?.to_string();
    let session_id = doc.get_str("session_id").ok()?.to_string();
    let conversation_turn_id = doc.get_str("conversation_turn_id").ok()?.to_string();
    let kind = doc.get_str("kind").ok().unwrap_or_default().to_string();
    let status = parse_status(doc.get_str("status").ok().unwrap_or("pending"));
    let prompt = doc
        .get_str("prompt_json")
        .ok()
        .map(parse_json_or_default)
        .unwrap_or_else(|| json!({}));
    let response = doc.get_str("response_json").ok().map(parse_json_or_default);
    let tool_call_id = doc
        .get_str("tool_call_id")
        .ok()
        .and_then(trimmed_non_empty)
        .map(|value| value.to_string());
    let expires_at = doc
        .get_str("expires_at")
        .ok()
        .and_then(trimmed_non_empty)
        .map(|value| value.to_string());
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

    Some(UiPromptRecord {
        id,
        session_id,
        conversation_turn_id,
        tool_call_id,
        kind,
        status,
        prompt,
        response,
        expires_at,
        created_at,
        updated_at,
    })
}

fn parse_json_or_default(raw: &str) -> serde_json::Value {
    serde_json::from_str::<serde_json::Value>(raw).unwrap_or_else(|_| json!({}))
}

fn parse_status(raw: &str) -> UiPromptStatus {
    UiPromptStatus::from_str(raw).unwrap_or(UiPromptStatus::Pending)
}
