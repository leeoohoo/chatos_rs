use mongodb::bson::{doc, Document};

use crate::core::mongo_cursor::collect_map_sorted_desc;
use crate::core::mongo_query::filter_optional_user_id;
use crate::core::sql_query::build_select_all_with_optional_user_id;
use crate::models::remote_connection::{RemoteConnection, RemoteConnectionRow};
use crate::repositories::db::with_db;

use super::{decrypt_connection_for_read, normalize_doc};

pub async fn list_remote_connections(
    user_id: Option<String>,
) -> Result<Vec<RemoteConnection>, String> {
    with_db(
        |db| {
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
        },
        |pool| {
            let user_id = user_id.clone();
            Box::pin(async move {
                let query = build_select_all_with_optional_user_id(
                    "remote_connections",
                    user_id.is_some(),
                    true,
                );
                let mut q = sqlx::query_as::<_, RemoteConnectionRow>(&query);
                if let Some(uid) = user_id {
                    q = q.bind(uid);
                }
                let rows = q.fetch_all(pool).await.map_err(|e| e.to_string())?;
                Ok(rows
                    .into_iter()
                    .map(RemoteConnectionRow::to_remote_connection)
                    .map(decrypt_connection_for_read)
                    .collect())
            })
        },
    )
    .await
}

pub async fn get_remote_connection_by_id(id: &str) -> Result<Option<RemoteConnection>, String> {
    with_db(
        |db| {
            let id = id.to_string();
            Box::pin(async move {
                let doc = db
                    .collection::<Document>("remote_connections")
                    .find_one(doc! { "id": id }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(doc.and_then(|d| normalize_doc(&d)))
            })
        },
        |pool| {
            let id = id.to_string();
            Box::pin(async move {
                let row = sqlx::query_as::<_, RemoteConnectionRow>(
                    "SELECT * FROM remote_connections WHERE id = ?",
                )
                .bind(&id)
                .fetch_optional(pool)
                .await
                .map_err(|e| e.to_string())?;
                Ok(row
                    .map(RemoteConnectionRow::to_remote_connection)
                    .map(decrypt_connection_for_read))
            })
        },
    )
    .await
}
