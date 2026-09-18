// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::support::normalize_optional_text;
use crate::models::memory_mapping::ChatosContact;
use crate::repositories::db::{db_error, decode_all, decode_optional, json, timestamp, with_db};

#[derive(Debug, Clone)]
pub struct UpdateContactTaskRunnerConfigInput {
    pub enabled: bool,
    pub base_url: Option<String>,
    pub agent_account_id: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub clear_password: bool,
}
pub async fn list_contacts(
    user_id: &str,
    status: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<ChatosContact>, String> {
    let status = normalize_optional_text(status);
    with_db(|pool|Box::pin(async move{decode_all(sqlx::query_scalar("SELECT data FROM chatos_contacts WHERE user_id=$1 AND ($2::text IS NULL OR status=$2) ORDER BY updated_at DESC,created_at DESC LIMIT $3 OFFSET $4").bind(user_id).bind(status).bind(limit.clamp(1,500)).bind(offset.max(0)).fetch_all(pool).await.map_err(db_error)?)})).await
}
pub async fn get_contact_by_id(id: &str) -> Result<Option<ChatosContact>, String> {
    with_db(|pool| {
        Box::pin(async move {
            decode_optional(
                sqlx::query_scalar("SELECT data FROM chatos_contacts WHERE id=$1")
                    .bind(id)
                    .fetch_optional(pool)
                    .await
                    .map_err(db_error)?,
            )
        })
    })
    .await
}
pub async fn get_contact_by_user_and_agent(
    user_id: &str,
    agent_id: &str,
) -> Result<Option<ChatosContact>, String> {
    with_db(|pool| {
        Box::pin(async move {
            decode_optional(
                sqlx::query_scalar(
                    "SELECT data FROM chatos_contacts WHERE user_id=$1 AND agent_id=$2",
                )
                .bind(user_id)
                .bind(agent_id)
                .fetch_optional(pool)
                .await
                .map_err(db_error)?,
            )
        })
    })
    .await
}
pub async fn list_contacts_by_ids(
    user_id: &str,
    ids: &[String],
    status: Option<&str>,
) -> Result<Vec<ChatosContact>, String> {
    let ids = ids
        .iter()
        .filter_map(|v| normalize_optional_text(Some(v)))
        .collect::<Vec<_>>();
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let status = normalize_optional_text(status);
    with_db(|pool|Box::pin(async move{decode_all(sqlx::query_scalar("SELECT data FROM chatos_contacts WHERE user_id=$1 AND id=ANY($2) AND ($3::text IS NULL OR status=$3) ORDER BY updated_at DESC,created_at DESC").bind(user_id).bind(ids).bind(status).fetch_all(pool).await.map_err(db_error)?)})).await
}
pub async fn create_contact_idempotent(
    user_id: &str,
    agent_id: &str,
    agent_name_snapshot: Option<String>,
) -> Result<(ChatosContact, bool), String> {
    let contact = ChatosContact::new(
        user_id.to_string(),
        agent_id.to_string(),
        agent_name_snapshot,
        "active".to_string(),
    );
    with_db(|pool|Box::pin(async move{let inserted=sqlx::query_scalar::<_,sqlx::types::Json<serde_json::Value>>("INSERT INTO chatos_contacts(id,user_id,agent_id,status,created_at,updated_at,data) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(user_id,agent_id) DO NOTHING RETURNING data").bind(&contact.id).bind(&contact.user_id).bind(&contact.agent_id).bind(&contact.status).bind(timestamp(&contact.created_at)?).bind(timestamp(&contact.updated_at)?).bind(json(&contact)?).fetch_optional(pool).await.map_err(db_error)?;if let Some(value)=inserted{return Ok((serde_json::from_value(value.0).map_err(|e|e.to_string())?,true))}let existing=decode_optional(sqlx::query_scalar("SELECT data FROM chatos_contacts WHERE user_id=$1 AND agent_id=$2").bind(&contact.user_id).bind(&contact.agent_id).fetch_optional(pool).await.map_err(db_error)?)?.ok_or_else(||"contact conflict record disappeared".to_string())?;Ok((existing,false))})).await
}
pub async fn update_contact_task_runner_config(
    contact_id: &str,
    input: UpdateContactTaskRunnerConfigInput,
) -> Result<Option<ChatosContact>, String> {
    let Some(mut record) = get_contact_by_id(contact_id).await? else {
        return Ok(None);
    };
    record.task_runner_base_url = normalize_optional_text(input.base_url.as_deref());
    record.task_runner_agent_account_id =
        normalize_optional_text(input.agent_account_id.as_deref());
    record.task_runner_username = normalize_optional_text(input.username.as_deref());
    record.task_runner_password = match normalize_optional_text(input.password.as_deref()) {
        Some(v) => Some(v),
        None if input.clear_password => None,
        None => record.task_runner_password,
    };
    record.task_runner_enabled = input.enabled
        && record.task_runner_base_url.is_some()
        && (record.task_runner_agent_account_id.is_some()
            || (record.task_runner_username.is_some() && record.task_runner_password.is_some()));
    record.updated_at = crate::core::time::now_rfc3339();
    with_db(|pool| {
        Box::pin(async move {
            let result =
                sqlx::query("UPDATE chatos_contacts SET updated_at=$1,data=$2 WHERE id=$3")
                    .bind(timestamp(&record.updated_at)?)
                    .bind(json(&record)?)
                    .bind(contact_id)
                    .execute(pool)
                    .await
                    .map_err(db_error)?;
            Ok((result.rows_affected() == 1).then_some(record))
        })
    })
    .await
}
pub async fn delete_contact_by_id(id: &str) -> Result<bool, String> {
    with_db(|pool| {
        Box::pin(async move {
            sqlx::query("DELETE FROM chatos_contacts WHERE id=$1")
                .bind(id)
                .execute(pool)
                .await
                .map(|r| r.rows_affected() > 0)
                .map_err(db_error)
        })
    })
    .await
}
