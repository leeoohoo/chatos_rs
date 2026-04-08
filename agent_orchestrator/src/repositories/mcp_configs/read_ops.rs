use mongodb::bson::{doc, Document};

use crate::core::mongo_cursor::{collect_and_map, collect_map_sorted_desc};
use crate::core::mongo_query::{filter_optional_user_id, insert_optional_user_id};
use crate::core::sql_query::{
    append_optional_user_id_filter, build_select_all_with_optional_user_id,
};
use crate::models::mcp_config::{McpConfig, McpConfigRow};
use crate::repositories::db::with_db;

use super::normalize_doc;

pub async fn list_mcp_configs(user_id: Option<String>) -> Result<Vec<McpConfig>, String> {
    with_db(
        |db| {
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
        },
        |pool| {
            let user_id = user_id.clone();
            Box::pin(async move {
                let query =
                    build_select_all_with_optional_user_id("mcp_configs", user_id.is_some(), true);
                let mut q = sqlx::query_as::<_, McpConfigRow>(&query);
                if let Some(uid) = user_id {
                    q = q.bind(uid);
                }
                let rows = q.fetch_all(pool).await.map_err(|e| e.to_string())?;
                Ok(rows.into_iter().map(|r| r.to_config()).collect())
            })
        },
    )
    .await
}

pub async fn list_enabled_mcp_configs(user_id: Option<String>) -> Result<Vec<McpConfig>, String> {
    with_db(
        |db| {
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
        },
        |pool| {
            let user_id = user_id.clone();
            Box::pin(async move {
                let query =
                    build_select_all_with_optional_user_id("mcp_configs", user_id.is_some(), false);
                let mut q = sqlx::query_as::<_, McpConfigRow>(&query);
                if let Some(uid) = user_id {
                    q = q.bind(uid);
                }
                let rows = q.fetch_all(pool).await.map_err(|e| e.to_string())?;
                Ok(rows.into_iter().map(|r| r.to_config()).collect())
            })
        },
    )
    .await
}

pub async fn list_enabled_mcp_configs_by_ids(
    user_id: Option<String>,
    ids: &[String],
) -> Result<Vec<McpConfig>, String> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    with_db(
        |db| {
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
        },
        |pool| {
            let user_id = user_id.clone();
            let ids = ids.to_vec();
            Box::pin(async move {
                let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
                let mut query = format!("SELECT * FROM mcp_configs WHERE id IN ({})", placeholders);
                append_optional_user_id_filter(&mut query, user_id.is_some(), true);
                let mut q = sqlx::query_as::<_, McpConfigRow>(&query);
                for id in &ids {
                    q = q.bind(id);
                }
                if let Some(uid) = user_id {
                    q = q.bind(uid);
                }
                let rows = q.fetch_all(pool).await.map_err(|e| e.to_string())?;
                Ok(rows.into_iter().map(|r| r.to_config()).collect())
            })
        },
    )
    .await
}

pub async fn get_mcp_config_by_id(id: &str) -> Result<Option<McpConfig>, String> {
    with_db(
        |db| {
            let id = id.to_string();
            Box::pin(async move {
                let doc = db
                    .collection::<Document>("mcp_configs")
                    .find_one(doc! { "id": id }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(doc.and_then(|d| normalize_doc(&d)))
            })
        },
        |pool| {
            let id = id.to_string();
            Box::pin(async move {
                let row =
                    sqlx::query_as::<_, McpConfigRow>("SELECT * FROM mcp_configs WHERE id = ?")
                        .bind(&id)
                        .fetch_optional(pool)
                        .await
                        .map_err(|e| e.to_string())?;
                Ok(row.map(|r| r.to_config()))
            })
        },
    )
    .await
}
