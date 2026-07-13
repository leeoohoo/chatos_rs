// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use mongodb::bson::{doc, Document};

use crate::core::mongo_cursor::collect_string_field;
use crate::repositories::db::{mongo_delete_many_doc, with_db};

pub async fn get_app_ids_for_mcp_config(config_id: &str) -> Result<Vec<String>, String> {
    with_db(|db| {
        let config_id = config_id.to_string();
        Box::pin(async move {
            let cursor = db
                .collection::<Document>("mcp_config_applications")
                .find(doc! { "mcp_config_id": config_id }, None)
                .await
                .map_err(|e| e.to_string())?;
            collect_string_field(cursor, "application_id").await
        })
    })
    .await
}

pub async fn set_app_ids_for_mcp_config(config_id: &str, app_ids: &[String]) -> Result<(), String> {
    with_db(|db| {
        let config_id = config_id.to_string();
        let app_ids = app_ids.to_vec();
        Box::pin(async move {
            mongo_delete_many_doc(
                db,
                "mcp_config_applications",
                doc! { "mcp_config_id": &config_id },
            )
            .await?;
            if !app_ids.is_empty() {
                let now = crate::core::time::now_rfc3339();
                let docs: Vec<Document> = app_ids
                    .iter()
                    .map(|aid| {
                        doc! {
                            "id": format!("{}_{}", config_id, aid),
                            "mcp_config_id": &config_id,
                            "application_id": aid,
                            "created_at": &now
                        }
                    })
                    .collect();
                db.collection::<Document>("mcp_config_applications")
                    .insert_many(docs, None)
                    .await
                    .map_err(|e| e.to_string())?;
            }
            Ok(())
        })
    })
    .await
}
