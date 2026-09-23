// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::application::Application;
use crate::repositories::db::{db_error, decode_all, decode_optional, json, timestamp, with_db};

pub async fn list_applications(user_id: Option<String>) -> Result<Vec<Application>, String> {
    with_db(|pool| Box::pin(async move { decode_all(sqlx::query_scalar("SELECT data FROM applications WHERE ($1::text IS NULL OR user_id=$1) ORDER BY created_at DESC").bind(user_id).fetch_all(pool).await.map_err(db_error)?) })).await
}

pub async fn get_application_by_id(id: &str) -> Result<Option<Application>, String> {
    with_db(|pool| {
        Box::pin(async move {
            decode_optional(
                sqlx::query_scalar("SELECT data FROM applications WHERE id=$1")
                    .bind(id)
                    .fetch_optional(pool)
                    .await
                    .map_err(db_error)?,
            )
        })
    })
    .await
}

pub async fn create_application(app: &Application) -> Result<Application, String> {
    let mut stored = app.clone();
    let now = crate::core::time::now_rfc3339();
    stored.created_at = now.clone();
    stored.updated_at = now;
    with_db(|pool| Box::pin(async move {
        sqlx::query("INSERT INTO applications(id,user_id,name,enabled,created_at,updated_at,data) VALUES($1,$2,$3,$4,$5,$6,$7)")
            .bind(&stored.id).bind(&stored.user_id).bind(&stored.name).bind(stored.enabled).bind(timestamp(&stored.created_at)?).bind(timestamp(&stored.updated_at)?).bind(json(&stored)?)
            .execute(pool).await.map_err(db_error)?;
        Ok(app.clone())
    })).await
}

pub async fn update_application(id: &str, updates: &Application) -> Result<(), String> {
    let mut stored = get_application_by_id(id)
        .await?
        .ok_or_else(|| "application not found".to_string())?;
    stored.name = updates.name.clone();
    stored.url = updates.url.clone();
    stored.description = updates.description.clone();
    stored.enabled = updates.enabled;
    stored.updated_at = crate::core::time::now_rfc3339();
    with_db(|pool| {
        Box::pin(async move {
            sqlx::query(
                "UPDATE applications SET name=$1,enabled=$2,updated_at=$3,data=$4 WHERE id=$5",
            )
            .bind(&stored.name)
            .bind(stored.enabled)
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

pub async fn delete_application(id: &str) -> Result<(), String> {
    with_db(|pool| {
        Box::pin(async move {
            sqlx::query("DELETE FROM applications WHERE id=$1")
                .bind(id)
                .execute(pool)
                .await
                .map(|_| ())
                .map_err(db_error)
        })
    })
    .await
}
