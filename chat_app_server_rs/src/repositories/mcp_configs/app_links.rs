use mongodb::bson::{doc, Document};

use crate::core::mongo_cursor::collect_string_field;
use crate::core::sql_rows::collect_string_column;
use crate::repositories::db::with_db;

pub async fn get_app_ids_for_mcp_config(config_id: &str) -> Result<Vec<String>, String> {
    with_db(
        |db| {
            let config_id = config_id.to_string();
            Box::pin(async move {
                let cursor = db
                    .collection::<Document>("mcp_config_applications")
                    .find(doc! { "mcp_config_id": config_id }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                collect_string_field(cursor, "application_id").await
            })
        },
        |pool| {
            let config_id = config_id.to_string();
            Box::pin(async move {
                let rows = sqlx::query(
                    "SELECT application_id FROM mcp_config_applications WHERE mcp_config_id = ?",
                )
                .bind(&config_id)
                .fetch_all(pool)
                .await
                .map_err(|e| e.to_string())?;
                Ok(collect_string_column(rows, "application_id"))
            })
        },
    )
    .await
}

pub async fn set_app_ids_for_mcp_config(config_id: &str, app_ids: &[String]) -> Result<(), String> {
    with_db(
        |db| {
            let config_id = config_id.to_string();
            let app_ids = app_ids.to_vec();
            Box::pin(async move {
                db.collection::<Document>("mcp_config_applications")
                    .delete_many(doc! { "mcp_config_id": &config_id }, None)
                    .await
                    .map_err(|e| e.to_string())?;
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
        },
        |pool| {
            let config_id = config_id.to_string();
            let app_ids = app_ids.to_vec();
            Box::pin(async move {
                sqlx::query("DELETE FROM mcp_config_applications WHERE mcp_config_id = ?")
                    .bind(&config_id)
                    .execute(pool)
                    .await
                    .map_err(|e| e.to_string())?;
                let now = crate::core::time::now_rfc3339();
                for aid in app_ids {
                    sqlx::query("INSERT INTO mcp_config_applications (id, mcp_config_id, application_id, created_at) VALUES (?, ?, ?, ?)")
                        .bind(format!("{}_{}", config_id, aid))
                        .bind(&config_id)
                        .bind(&aid)
                        .bind(&now)
                        .execute(pool)
                        .await
                        .map_err(|e| e.to_string())?;
                }
                Ok(())
            })
        },
    )
    .await
}
