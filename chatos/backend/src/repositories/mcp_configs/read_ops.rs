// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use mongodb::bson::{doc, Document};

use crate::core::mongo_cursor::{collect_and_map, collect_map_sorted_desc};
use crate::core::mongo_query::{filter_optional_user_id, insert_optional_user_id};
use crate::models::mcp_config::McpConfig;
use crate::repositories::db::{mongo_find_one_doc, with_db};

use super::normalize_doc;

pub async fn list_mcp_configs(user_id: Option<String>) -> Result<Vec<McpConfig>, String> {
    with_db(|db| {
        let user_id = user_id.clone();
        Box::pin(async move {
            let filter = filter_optional_user_id(user_id);
            let cursor = db
                .collection::<Document>("mcp_configs")
                .find(filter, None)
                .await
                .map_err(|e| e.to_string())?;
            let items: Vec<McpConfig> =
                collect_map_sorted_desc(cursor, normalize_doc, |item| item.created_at.as_str())
                    .await?;
            Ok(items)
        })
    })
    .await
}

pub async fn list_enabled_mcp_configs(user_id: Option<String>) -> Result<Vec<McpConfig>, String> {
    with_db(|db| {
        let user_id = user_id.clone();
        Box::pin(async move {
            let mut filter = doc! {};
            insert_optional_user_id(&mut filter, user_id);
            let cursor = db
                .collection::<Document>("mcp_configs")
                .find(filter, None)
                .await
                .map_err(|e| e.to_string())?;
            collect_and_map(cursor, normalize_doc).await
        })
    })
    .await
}

pub async fn list_enabled_mcp_configs_by_ids(
    user_id: Option<String>,
    ids: &[String],
) -> Result<Vec<McpConfig>, String> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    with_db(|db| {
        let user_id = user_id.clone();
        let ids = ids.to_vec();
        Box::pin(async move {
            let mut filter = doc! { "id": { "$in": ids } };
            insert_optional_user_id(&mut filter, user_id);
            let cursor = db
                .collection::<Document>("mcp_configs")
                .find(filter, None)
                .await
                .map_err(|e| e.to_string())?;
            collect_and_map(cursor, normalize_doc).await
        })
    })
    .await
}

pub async fn get_mcp_config_by_id(id: &str) -> Result<Option<McpConfig>, String> {
    with_db(|db| {
        let id = id.to_string();
        Box::pin(async move {
            let doc = mongo_find_one_doc(db, "mcp_configs", doc! { "id": id }).await?;
            Ok(doc.and_then(|d| normalize_doc(&d)))
        })
    })
    .await
}
