// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::system_context::SystemContext;
use crate::repositories::db::{db_error, decode_all, decode_optional, json, timestamp, with_db};

pub async fn list_system_contexts(user_id: &str) -> Result<Vec<SystemContext>, String> {
    with_db(|pool| {
        Box::pin(async move {
            decode_all(
                sqlx::query_scalar(
                    "SELECT data FROM system_contexts WHERE user_id=$1 ORDER BY created_at DESC",
                )
                .bind(user_id)
                .fetch_all(pool)
                .await
                .map_err(db_error)?,
            )
        })
    })
    .await
}
pub async fn get_active_system_context(user_id: &str) -> Result<Option<SystemContext>, String> {
    with_db(|pool| {
        Box::pin(async move {
            decode_optional(
                sqlx::query_scalar(
                    "SELECT data FROM system_contexts WHERE user_id=$1 AND is_active",
                )
                .bind(user_id)
                .fetch_optional(pool)
                .await
                .map_err(db_error)?,
            )
        })
    })
    .await
}
pub async fn get_system_context_by_id(id: &str) -> Result<Option<SystemContext>, String> {
    with_db(|pool| {
        Box::pin(async move {
            decode_optional(
                sqlx::query_scalar("SELECT data FROM system_contexts WHERE id=$1")
                    .bind(id)
                    .fetch_optional(pool)
                    .await
                    .map_err(db_error)?,
            )
        })
    })
    .await
}
pub async fn create_system_context(ctx: &SystemContext) -> Result<(), String> {
    let mut stored = ctx.clone();
    let now = crate::core::time::now_rfc3339();
    stored.created_at = now.clone();
    stored.updated_at = now;
    with_db(|pool|Box::pin(async move{sqlx::query("INSERT INTO system_contexts(id,user_id,is_active,created_at,updated_at,data) VALUES($1,$2,$3,$4,$5,$6)").bind(&stored.id).bind(&stored.user_id).bind(stored.is_active).bind(timestamp(&stored.created_at)?).bind(timestamp(&stored.updated_at)?).bind(json(&stored)?).execute(pool).await.map(|_|()).map_err(db_error)})).await
}
pub async fn update_system_context(id: &str, updates: &SystemContext) -> Result<(), String> {
    let mut stored = get_system_context_by_id(id)
        .await?
        .ok_or_else(|| "system context not found".to_string())?;
    stored.name = updates.name.clone();
    stored.content = updates.content.clone();
    stored.is_active = updates.is_active;
    stored.updated_at = crate::core::time::now_rfc3339();
    with_db(|pool| {
        Box::pin(async move {
            sqlx::query("UPDATE system_contexts SET is_active=$1,updated_at=$2,data=$3 WHERE id=$4")
                .bind(stored.is_active)
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
pub async fn delete_system_context(id: &str) -> Result<(), String> {
    with_db(|pool| {
        Box::pin(async move {
            sqlx::query("DELETE FROM system_contexts WHERE id=$1")
                .bind(id)
                .execute(pool)
                .await
                .map(|_| ())
                .map_err(db_error)
        })
    })
    .await
}
pub async fn activate_system_context(context_id: &str, user_id: &str) -> Result<(), String> {
    let now = crate::core::time::now_rfc3339();
    with_db(|pool| {
        Box::pin(async move {
            let mut tx = pool.begin().await.map_err(db_error)?;
            let records = sqlx::query_scalar::<_, sqlx::types::Json<serde_json::Value>>(
                "SELECT data FROM system_contexts WHERE user_id=$1 FOR UPDATE",
            )
            .bind(user_id)
            .fetch_all(&mut *tx)
            .await
            .map_err(db_error)?;
            let mut records = records
                .into_iter()
                .map(|value| serde_json::from_value::<SystemContext>(value.0))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            if !records.iter().any(|record| record.id == context_id) {
                return Err("system context not found for user".to_string());
            }

            // Clear the previous active row first. Activating the target while another row is
            // still active would violate the partial unique index for the user.
            for record in &mut records {
                record.is_active = false;
                sqlx::query(
                    "UPDATE system_contexts SET is_active=$1,updated_at=$2,data=$3 WHERE id=$4",
                )
                .bind(false)
                .bind(timestamp(&record.updated_at)?)
                .bind(json(&*record)?)
                .bind(&record.id)
                .execute(&mut *tx)
                .await
                .map_err(db_error)?;
            }

            let target = records
                .iter_mut()
                .find(|record| record.id == context_id)
                .ok_or_else(|| "system context target disappeared during activation".to_string())?;
            target.is_active = true;
            target.updated_at = now;
            sqlx::query(
                "UPDATE system_contexts SET is_active=true,updated_at=$1,data=$2 WHERE id=$3 AND user_id=$4",
            )
            .bind(timestamp(&target.updated_at)?)
            .bind(json(&*target)?)
            .bind(context_id)
            .bind(user_id)
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
            tx.commit().await.map_err(db_error)
        })
    })
    .await
}
pub async fn get_app_ids_for_system_context(context_id: &str) -> Result<Vec<String>, String> {
    with_db(|pool|Box::pin(async move{sqlx::query_scalar("SELECT application_id FROM system_context_applications WHERE system_context_id=$1 ORDER BY application_id").bind(context_id).fetch_all(pool).await.map_err(db_error)})).await
}
pub async fn set_app_ids_for_system_context(
    context_id: &str,
    app_ids: &[String],
) -> Result<(), String> {
    with_db(|pool|Box::pin(async move{let mut tx=pool.begin().await.map_err(db_error)?;sqlx::query("DELETE FROM system_context_applications WHERE system_context_id=$1").bind(context_id).execute(&mut *tx).await.map_err(db_error)?;for app_id in app_ids{sqlx::query("INSERT INTO system_context_applications(id,system_context_id,application_id,created_at) VALUES($1,$2,$3,now())").bind(format!("{context_id}_{app_id}")).bind(context_id).bind(app_id).execute(&mut *tx).await.map_err(db_error)?;}tx.commit().await.map_err(db_error)})).await
}
