// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use mongodb::bson::{doc, Bson, Document};
use serde_json::Value;

use crate::models::user_settings::UserSettings;
use crate::repositories::db::{mongo_find_one_doc, mongo_update_one_doc, with_db};

pub async fn get_user_settings(user_id: &str) -> Result<Option<UserSettings>, String> {
    with_db(|db| {
        let user_id = user_id.to_string();
        Box::pin(async move {
            let doc = mongo_find_one_doc(db, "user_settings", doc! { "user_id": &user_id }).await?;
            if let Some(doc) = doc {
                let settings = doc
                    .get("settings")
                    .cloned()
                    .unwrap_or(Bson::Document(Document::new()));
                return Ok(Some(UserSettings {
                    user_id,
                    settings: bson_to_json(settings),
                }));
            }
            Ok(None)
        })
    })
    .await
}

pub async fn set_user_settings(user_id: &str, settings: &Value) -> Result<(), String> {
    with_db(|db| {
        let user_id = user_id.to_string();
        let settings = settings.clone();
        Box::pin(async move {
            let now = crate::core::time::now_rfc3339();
            mongo_update_one_doc(
                db,
                "user_settings",
                doc! { "user_id": &user_id },
                doc! { "$set": { "user_id": &user_id, "settings": json_to_bson(settings), "updated_at": &now } },
                Some(mongodb::options::UpdateOptions::builder().upsert(true).build()),
            )
            .await?;
            Ok(())
        })
    })
    .await
}

pub async fn purge_managed_runtime_settings() -> Result<u64, String> {
    with_db(|db| {
        Box::pin(async move {
            db.collection::<Document>("user_settings")
                .update_many(
                    doc! {},
                    doc! {
                        "$unset": {
                            "settings.MAX_ITERATIONS": "",
                            "settings.TASK_FOLLOW_UP_MAX_ROUNDS": "",
                            "settings.LOG_LEVEL": "",
                            "settings.HISTORY_LIMIT": "",
                            "settings.CHAT_MAX_TOKENS": "",
                            "settings.ATTACHMENT_TOTAL_MAX_BYTES": "",
                            "settings.TERMINAL_UI_ENABLED": "",
                        }
                    },
                    None,
                )
                .await
                .map(|result| result.modified_count)
                .map_err(|err| err.to_string())
        })
    })
    .await
}

fn bson_to_json(bson: Bson) -> Value {
    serde_json::to_value(bson).unwrap_or(Value::Null)
}

fn json_to_bson(json: Value) -> Bson {
    mongodb::bson::to_bson(&json).unwrap_or(Bson::Null)
}
