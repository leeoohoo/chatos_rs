// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{encrypt_connection_for_storage, get_remote_connection_by_id};
use crate::models::remote_connection::RemoteConnection;
use crate::repositories::db::{db_error, json, timestamp, with_db};

pub async fn create_remote_connection(connection: &RemoteConnection) -> Result<String, String> {
    let mut stored = encrypt_connection_for_storage(connection.clone())?;
    let now = crate::core::time::now_rfc3339();
    stored.created_at = now.clone();
    stored.updated_at = now.clone();
    stored.last_active_at = now;
    with_db(|pool|Box::pin(async move{sqlx::query("INSERT INTO remote_connections(id,user_id,host,created_at,updated_at,last_active_at,data) VALUES($1,$2,$3,$4,$5,$6,$7)").bind(&stored.id).bind(&stored.user_id).bind(&stored.host).bind(timestamp(&stored.created_at)?).bind(timestamp(&stored.updated_at)?).bind(timestamp(&stored.last_active_at)?).bind(json(&stored)?).execute(pool).await.map_err(db_error)?;Ok(stored.id)})).await
}
pub async fn update_remote_connection(id: &str, data: &RemoteConnection) -> Result<(), String> {
    let existing = get_remote_connection_by_id(id)
        .await?
        .ok_or_else(|| "remote connection not found".to_string())?;
    let mut stored = encrypt_connection_for_storage(data.clone())?;
    stored.id = id.to_string();
    stored.user_id = existing.user_id;
    stored.created_at = existing.created_at;
    stored.last_active_at = existing.last_active_at;
    stored.updated_at = crate::core::time::now_rfc3339();
    with_db(|pool| {
        Box::pin(async move {
            sqlx::query("UPDATE remote_connections SET host=$1,updated_at=$2,data=$3 WHERE id=$4")
                .bind(&stored.host)
                .bind(timestamp(&stored.updated_at)?)
                .bind(json(&stored)?)
                .bind(id)
                .execute(pool)
                .await
                .map(|_| ())
                .map_err(db_error)
        })
    })
    .await
}
pub async fn touch_remote_connection(id: &str) -> Result<(), String> {
    let mut stored = get_remote_connection_by_id(id)
        .await?
        .ok_or_else(|| "remote connection not found".to_string())?;
    let now = crate::core::time::now_rfc3339();
    stored.updated_at = now.clone();
    stored.last_active_at = now;
    let stored = encrypt_connection_for_storage(stored)?;
    with_db(|pool| {
        Box::pin(async move {
            sqlx::query(
                "UPDATE remote_connections SET updated_at=$1,last_active_at=$2,data=$3 WHERE id=$4",
            )
            .bind(timestamp(&stored.updated_at)?)
            .bind(timestamp(&stored.last_active_at)?)
            .bind(json(&stored)?)
            .bind(id)
            .execute(pool)
            .await
            .map(|_| ())
            .map_err(db_error)
        })
    })
    .await
}
pub async fn delete_remote_connection(id: &str) -> Result<(), String> {
    with_db(|pool| {
        Box::pin(async move {
            sqlx::query("DELETE FROM remote_connections WHERE id=$1")
                .bind(id)
                .execute(pool)
                .await
                .map(|_| ())
                .map_err(db_error)
        })
    })
    .await
}
