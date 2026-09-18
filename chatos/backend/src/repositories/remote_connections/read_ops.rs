// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::decrypt_connection_for_read;
use crate::models::remote_connection::RemoteConnection;
use crate::repositories::db::{db_error, decode_all, decode_optional, with_db};

pub async fn list_remote_connections(
    user_id: Option<String>,
) -> Result<Vec<RemoteConnection>, String> {
    with_db(|pool|Box::pin(async move{let values=decode_all(sqlx::query_scalar("SELECT data FROM remote_connections WHERE ($1::text IS NULL OR user_id=$1) ORDER BY created_at DESC").bind(user_id).fetch_all(pool).await.map_err(db_error)?)?;Ok(values.into_iter().map(decrypt_connection_for_read).collect())})).await
}
pub async fn get_remote_connection_by_id(id: &str) -> Result<Option<RemoteConnection>, String> {
    with_db(|pool| {
        Box::pin(async move {
            Ok(decode_optional(
                sqlx::query_scalar("SELECT data FROM remote_connections WHERE id=$1")
                    .bind(id)
                    .fetch_optional(pool)
                    .await
                    .map_err(db_error)?,
            )?
            .map(decrypt_connection_for_read))
        })
    })
    .await
}
