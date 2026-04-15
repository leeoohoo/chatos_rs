use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use sqlx::{QueryBuilder, Sqlite};

use crate::repositories::db::with_db;
use crate::services::ui_prompt_manager::normalizer::trimmed_non_empty;
use crate::services::ui_prompt_manager::types::{UiPromptRecord, UiPromptStatus};

use super::codec::ui_prompt_record_from_doc;
use super::row::UiPromptRow;

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
                    "SELECT id, conversation_id, conversation_turn_id, tool_call_id, kind, status, prompt_json, response_json, expires_at, created_at, updated_at FROM ui_prompt_requests WHERE id = ? LIMIT 1",
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
    conversation_id: &str,
    limit: usize,
) -> Result<Vec<UiPromptRecord>, String> {
    let conversation_id = trimmed_non_empty(conversation_id)
        .ok_or_else(|| "conversation_id is required".to_string())?
        .to_string();
    let limit = limit.clamp(1, 200) as i64;

    let conversation_id_for_mongo = conversation_id.clone();
    let conversation_id_for_sqlite = conversation_id.clone();

    with_db(
        move |db| {
            let conversation_id = conversation_id_for_mongo.clone();
            Box::pin(async move {
                let options = FindOptions::builder()
                    .sort(doc! { "created_at": 1 })
                    .limit(limit)
                    .build();
                let mut cursor = db
                    .collection::<Document>("ui_prompt_requests")
                    .find(
                        doc! {
                            "conversation_id": conversation_id,
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
            let conversation_id = conversation_id_for_sqlite.clone();
            Box::pin(async move {
                let mut qb = QueryBuilder::<Sqlite>::new(
                    "SELECT id, conversation_id, conversation_turn_id, tool_call_id, kind, status, prompt_json, response_json, expires_at, created_at, updated_at FROM ui_prompt_requests WHERE conversation_id = ",
                );
                qb.push_bind(conversation_id);
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
    conversation_id: &str,
    limit: usize,
    include_pending: bool,
) -> Result<Vec<UiPromptRecord>, String> {
    let conversation_id = trimmed_non_empty(conversation_id)
        .ok_or_else(|| "conversation_id is required".to_string())?
        .to_string();
    let limit = limit.clamp(1, 500) as i64;

    let conversation_id_for_mongo = conversation_id.clone();
    let conversation_id_for_sqlite = conversation_id.clone();

    with_db(
        move |db| {
            let conversation_id = conversation_id_for_mongo.clone();
            Box::pin(async move {
                let mut filter = doc! {
                    "conversation_id": conversation_id,
                };
                if !include_pending {
                    filter.insert("status", doc! { "$ne": UiPromptStatus::Pending.as_str() });
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
            let conversation_id = conversation_id_for_sqlite.clone();
            Box::pin(async move {
                let mut qb = QueryBuilder::<Sqlite>::new(
                    "SELECT id, conversation_id, conversation_turn_id, tool_call_id, kind, status, prompt_json, response_json, expires_at, created_at, updated_at FROM ui_prompt_requests WHERE conversation_id = ",
                );
                qb.push_bind(conversation_id);
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
