// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;

use crate::repositories::db::with_db;
use crate::services::ask_user_prompt_manager::normalizer::trimmed_non_empty;
use crate::services::ask_user_prompt_manager::types::{AskUserPromptRecord, AskUserPromptStatus};

use super::codec::ask_user_prompt_record_from_doc;

pub async fn get_ask_user_prompt_record(
    prompt_id: &str,
) -> Result<Option<AskUserPromptRecord>, String> {
    let prompt_id = trimmed_non_empty(prompt_id)
        .ok_or_else(|| "prompt_id is required".to_string())?
        .to_string();
    let prompt_id_for_mongo = prompt_id.clone();

    with_db(move |db| {
        let prompt_id = prompt_id_for_mongo.clone();
        Box::pin(async move {
            let doc = db
                .collection::<Document>("ask_user_prompt_requests")
                .find_one(doc! { "id": prompt_id }, None)
                .await
                .map_err(|err| err.to_string())?;
            Ok(doc.and_then(|doc| ask_user_prompt_record_from_doc(&doc)))
        })
    })
    .await
}

pub async fn list_ask_user_prompt_history_records(
    conversation_id: &str,
    limit: usize,
    include_pending: bool,
) -> Result<Vec<AskUserPromptRecord>, String> {
    let conversation_id = trimmed_non_empty(conversation_id)
        .ok_or_else(|| "conversation_id is required".to_string())?
        .to_string();
    let limit = limit.clamp(1, 500) as i64;

    let conversation_id_for_mongo = conversation_id.clone();

    with_db(move |db| {
        let conversation_id = conversation_id_for_mongo.clone();
        Box::pin(async move {
            let mut filter = doc! {
                "conversation_id": conversation_id,
            };
            if !include_pending {
                filter.insert(
                    "status",
                    doc! { "$ne": AskUserPromptStatus::Pending.as_str() },
                );
            }

            let options = FindOptions::builder()
                .sort(doc! { "updated_at": -1_i32, "created_at": -1_i32 })
                .limit(limit)
                .build();
            let mut cursor = db
                .collection::<Document>("ask_user_prompt_requests")
                .find(filter, options)
                .await
                .map_err(|err| err.to_string())?;

            let mut out = Vec::new();
            while cursor.advance().await.map_err(|err| err.to_string())? {
                let document = cursor
                    .deserialize_current()
                    .map_err(|err| err.to_string())?;
                if let Some(record) = ask_user_prompt_record_from_doc(&document) {
                    out.push(record);
                }
            }
            Ok(out)
        })
    })
    .await
}
