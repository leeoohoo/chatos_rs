// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use mongodb::bson::{doc, Bson, Document};
use mongodb::options::FindOptions;

use crate::repositories::db::with_db;
use crate::services::task_manager::mapper::task_record_from_doc;
use crate::services::task_manager::normalizer::trimmed_non_empty;
use crate::services::task_manager::types::TaskRecord;

pub async fn get_task_by_id(conversation_id: &str, task_id: &str) -> Result<TaskRecord, String> {
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
            db.collection::<Document>("task_manager_tasks")
                .find_one(
                    doc! { "conversation_id": conversation_id, "id": task_id },
                    None,
                )
                .await
                .map_err(|err| err.to_string())?
                .and_then(|document| task_record_from_doc(&document))
                .ok_or_else(|| crate::services::task_manager::TASK_NOT_FOUND_ERR.to_string())
        })
    })
    .await
}

pub async fn list_tasks_for_context(
    conversation_id: &str,
    conversation_turn_id: Option<&str>,
    include_done: bool,
    limit: usize,
) -> Result<Vec<TaskRecord>, String> {
    let conversation_id = trimmed_non_empty(conversation_id)
        .ok_or_else(|| "conversation_id is required".to_string())?
        .to_string();
    let conversation_turn_id = conversation_turn_id
        .and_then(trimmed_non_empty)
        .map(|value| value.to_string());
    let limit = limit.clamp(1, 200) as i64;
    let conversation_id_for_mongo = conversation_id.clone();
    let conversation_turn_id_for_mongo = conversation_turn_id.clone();

    with_db(move |db| {
        let conversation_id = conversation_id_for_mongo.clone();
        let conversation_turn_id = conversation_turn_id_for_mongo.clone();
        Box::pin(async move {
            let mut filter = doc! { "conversation_id": conversation_id };
            if let Some(turn_id) = conversation_turn_id {
                filter.insert("conversation_turn_id", Bson::String(turn_id));
            }
            if !include_done {
                filter.insert("status", doc! { "$ne": "done" });
            }

            let find_options = FindOptions::builder()
                .sort(doc! { "created_at": 1 })
                .limit(limit)
                .build();
            let mut cursor = db
                .collection::<Document>("task_manager_tasks")
                .find(filter, find_options)
                .await
                .map_err(|err| err.to_string())?;

            let mut out = Vec::new();
            while cursor.advance().await.map_err(|err| err.to_string())? {
                let document = cursor
                    .deserialize_current()
                    .map_err(|err| err.to_string())?;
                if let Some(task) = task_record_from_doc(&document) {
                    out.push(task);
                }
            }
            Ok(out)
        })
    })
    .await
}
