use crate::models::session_mcp_server::SessionMcpServer;
use crate::repositories::db::{doc_from_pairs, to_doc, with_db};
use futures::TryStreamExt;
use mongodb::bson::{doc, Bson, Document};
use sqlx::Row;

pub async fn list_session_mcp_servers(session_id: &str) -> Result<Vec<SessionMcpServer>, String> {
    with_db(
        |db| {
            let session_id = session_id.to_string();
            Box::pin(async move {
                let mut cursor = db
                    .collection::<Document>("session_mcp_servers")
                    .find(doc! { "session_id": session_id }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                let mut out = Vec::new();
                while let Some(doc) = cursor.try_next().await.map_err(|e| e.to_string())? {
                    let id = doc.get_str("id").unwrap_or("").to_string();
                    let session_id = doc.get_str("session_id").unwrap_or("").to_string();
                    let mcp_server_name =
                        doc.get_str("mcp_server_name").ok().map(|s| s.to_string());
                    let mcp_config_id = doc.get_str("mcp_config_id").ok().map(|s| s.to_string());
                    let created_at = doc.get_str("created_at").unwrap_or("").to_string();
                    out.push(SessionMcpServer {
                        id,
                        session_id,
                        mcp_server_name,
                        mcp_config_id,
                        created_at,
                    });
                }
                Ok(out)
            })
        },
        |pool| {
            let session_id = session_id.to_string();
            Box::pin(async move {
                let rows = sqlx::query("SELECT * FROM session_mcp_servers WHERE session_id = ?")
                    .bind(&session_id)
                    .fetch_all(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                let mut out = Vec::new();
                for row in rows {
                    let id: String = row.try_get("id").unwrap_or_default();
                    let session_id: String = row.try_get("session_id").unwrap_or_default();
                    let mcp_server_name: Option<String> = row.try_get("mcp_server_name").ok();
                    let mcp_config_id: Option<String> = row.try_get("mcp_config_id").ok();
                    let created_at: String = row.try_get("created_at").unwrap_or_default();
                    out.push(SessionMcpServer {
                        id,
                        session_id,
                        mcp_server_name,
                        mcp_config_id,
                        created_at,
                    });
                }
                Ok(out)
            })
        },
    )
    .await
}

pub async fn add_session_mcp_server(item: &SessionMcpServer) -> Result<(), String> {
    let now = if item.created_at.is_empty() {
        chrono::Utc::now().to_rfc3339()
    } else {
        item.created_at.clone()
    };
    let now_mongo = now.clone();
    let now_sqlite = now.clone();
    let item_mongo = item.clone();
    let item_sqlite = item.clone();
    with_db(
        |db| {
            let doc = to_doc(doc_from_pairs(vec![
                ("id", Bson::String(item_mongo.id.clone())),
                ("session_id", Bson::String(item_mongo.session_id.clone())),
                ("mcp_server_name", item_mongo.mcp_server_name.clone().map(Bson::String).unwrap_or(Bson::Null)),
                ("mcp_config_id", item_mongo.mcp_config_id.clone().map(Bson::String).unwrap_or(Bson::Null)),
                ("created_at", Bson::String(now_mongo.clone())),
            ]));
            Box::pin(async move {
                db.collection::<Document>("session_mcp_servers").insert_one(doc, None).await.map_err(|e| e.to_string())?;
                Ok(())
            })
        },
        |pool| {
            Box::pin(async move {
                sqlx::query("INSERT INTO session_mcp_servers (id, session_id, mcp_server_name, mcp_config_id, created_at) VALUES (?, ?, ?, ?, ?)")
                    .bind(&item_sqlite.id)
                    .bind(&item_sqlite.session_id)
                    .bind(&item_sqlite.mcp_server_name)
                    .bind(&item_sqlite.mcp_config_id)
                    .bind(&now_sqlite)
                    .execute(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(())
            })
        }
    ).await
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
                db.collection::<Document>("session_mcp_servers").delete_many(doc! { "session_id": session_id, "$or": [ { "id": &mcp_id }, { "mcp_config_id": &mcp_id } ] }, None).await.map_err(|e| e.to_string())?;
                Ok(())
            })
        },
        |pool| {
            let session_id = session_id.to_string();
            let mcp_id = mcp_config_id_or_id.to_string();
            Box::pin(async move {
                sqlx::query("DELETE FROM session_mcp_servers WHERE session_id = ? AND (id = ? OR mcp_config_id = ?)")
                    .bind(&session_id)
                    .bind(&mcp_id)
                    .bind(&mcp_id)
                    .execute(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(())
            })
        }
    ).await
}
