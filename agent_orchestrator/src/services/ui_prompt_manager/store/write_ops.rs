use chrono::{Duration, Utc};
use mongodb::bson::{doc, Bson, Document};
use mongodb::options::{FindOneAndUpdateOptions, ReturnDocument, UpdateOptions};

use crate::repositories::db::with_db;
use crate::services::ui_prompt_manager::normalizer::{redact_prompt_payload, trimmed_non_empty};
use crate::services::ui_prompt_manager::types::{
    UiPromptPayload, UiPromptRecord, UiPromptStatus, UI_PROMPT_NOT_FOUND_ERR,
};

use super::codec::{ui_prompt_record_from_doc, ui_prompt_record_to_doc};
use super::row::UiPromptRow;

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
                    .find_one_and_update(
                        doc! { "id": prompt_id },
                        doc! { "$set": set_doc },
                        options,
                    )
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
