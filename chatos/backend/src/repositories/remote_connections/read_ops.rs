// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use mongodb::bson::{doc, Document};

use crate::core::mongo_cursor::collect_map_sorted_desc;
use crate::core::mongo_query::filter_optional_user_id;
use crate::models::remote_connection::RemoteConnection;
use crate::repositories::db::{mongo_find_one_doc, with_db};

use super::normalize_doc;

pub async fn list_remote_connections(
    user_id: Option<String>,
) -> Result<Vec<RemoteConnection>, String> {
    with_db(|db| {
        let user_id = user_id.clone();
        Box::pin(async move {
            let filter = filter_optional_user_id(user_id);
            let cursor = db
                .collection::<Document>("remote_connections")
                .find(filter, None)
                .await
                .map_err(|e| e.to_string())?;
            let items: Vec<RemoteConnection> =
                collect_map_sorted_desc(cursor, normalize_doc, |item| item.created_at.as_str())
                    .await?;
            Ok(items)
        })
    })
    .await
}

pub async fn get_remote_connection_by_id(id: &str) -> Result<Option<RemoteConnection>, String> {
    with_db(|db| {
        let id = id.to_string();
        Box::pin(async move {
            let doc = mongo_find_one_doc(db, "remote_connections", doc! { "id": id }).await?;
            Ok(doc.and_then(|d| normalize_doc(&d)))
        })
    })
    .await
}
