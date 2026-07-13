// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::core::mongo_cursor::collect_and_map;
use crate::models::session_mcp_server::SessionMcpServer;
use crate::repositories::db::{
    doc_from_pairs, mongo_delete_many_doc, mongo_insert_doc, to_doc, with_db,
};
use mongodb::bson::{doc, Bson, Document};

fn normalize_doc(doc: &Document) -> Option<SessionMcpServer> {
    Some(SessionMcpServer {
        id: doc.get_str("id").unwrap_or("").to_string(),
        session_id: doc.get_str("session_id").unwrap_or("").to_string(),
        mcp_server_name: doc.get_str("mcp_server_name").ok().map(|s| s.to_string()),
        mcp_config_id: doc.get_str("mcp_config_id").ok().map(|s| s.to_string()),
        created_at: doc.get_str("created_at").unwrap_or("").to_string(),
    })
}

pub async fn list_session_mcp_servers(session_id: &str) -> Result<Vec<SessionMcpServer>, String> {
    with_db(|db| {
        let session_id = session_id.to_string();
        Box::pin(async move {
            let cursor = db
                .collection::<Document>("session_mcp_servers")
                .find(doc! { "session_id": session_id }, None)
                .await
                .map_err(|e| e.to_string())?;
            collect_and_map(cursor, normalize_doc).await
        })
    })
    .await
}

pub async fn add_session_mcp_server(item: &SessionMcpServer) -> Result<(), String> {
    let now = if item.created_at.is_empty() {
        crate::core::time::now_rfc3339()
    } else {
        item.created_at.clone()
    };
    let now_mongo = now.clone();
    let item_mongo = item.clone();
    with_db(|db| {
        let doc = to_doc(doc_from_pairs(vec![
            ("id", Bson::String(item_mongo.id.clone())),
            ("session_id", Bson::String(item_mongo.session_id.clone())),
            (
                "mcp_server_name",
                crate::core::values::optional_string_bson(item_mongo.mcp_server_name.clone()),
            ),
            (
                "mcp_config_id",
                crate::core::values::optional_string_bson(item_mongo.mcp_config_id.clone()),
            ),
            ("created_at", Bson::String(now_mongo.clone())),
        ]));
        Box::pin(async move {
            mongo_insert_doc(db, "session_mcp_servers", doc).await?;
            Ok(())
        })
    })
    .await
}

pub async fn delete_session_mcp_server(
    session_id: &str,
    mcp_config_id_or_id: &str,
) -> Result<(), String> {
    with_db(
        |db| {
            let session_id = session_id.to_string();
            let mcp_id = mcp_config_id_or_id.to_string();
            Box::pin(async move {
                mongo_delete_many_doc(
                    db,
                    "session_mcp_servers",
                    doc! { "session_id": session_id, "$or": [ { "id": &mcp_id }, { "mcp_config_id": &mcp_id } ] },
                )
                .await?;
                Ok(())
            })
        }).await
}
