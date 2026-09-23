// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::support::{normalize_optional_text, normalize_project_id};
use crate::models::memory_mapping::ChatosProjectAgentLink;
use crate::repositories::db::{db_error, decode_all, decode_optional, json, timestamp, with_db};

#[derive(Debug, Clone)]
pub struct UpsertProjectAgentLinkInput {
    pub user_id: String,
    pub project_id: String,
    pub agent_id: String,
    pub contact_id: Option<String>,
    pub latest_session_id: Option<String>,
    pub last_message_at: Option<String>,
    pub status: Option<String>,
}
pub async fn upsert_project_agent_link(
    input: UpsertProjectAgentLinkInput,
) -> Result<Option<ChatosProjectAgentLink>, String> {
    let now = crate::core::time::now_rfc3339();
    let project_id = normalize_project_id(&input.project_id);
    let status =
        normalize_optional_text(input.status.as_deref()).unwrap_or_else(|| "active".to_string());
    let lookup_user_id = input.user_id.clone();
    let lookup_project_id = project_id.clone();
    let existing=with_db(|pool|Box::pin(async move{decode_optional(sqlx::query_scalar("SELECT data FROM chatos_project_agent_links WHERE user_id=$1 AND project_id=$2").bind(lookup_user_id).bind(lookup_project_id).fetch_optional(pool).await.map_err(db_error)?)})).await?;
    let replaces = existing.as_ref().is_some_and(|r: &ChatosProjectAgentLink| {
        r.agent_id != input.agent_id || r.contact_id != input.contact_id
    });
    let mut record = existing.unwrap_or(ChatosProjectAgentLink {
        id: uuid::Uuid::new_v4().to_string(),
        user_id: input.user_id.clone(),
        project_id: project_id.clone(),
        agent_id: input.agent_id.clone(),
        contact_id: input.contact_id.clone(),
        latest_session_id: None,
        first_bound_at: now.clone(),
        last_bound_at: now.clone(),
        last_message_at: None,
        status: status.clone(),
        created_at: now.clone(),
        updated_at: now.clone(),
    });
    record.agent_id = input.agent_id;
    record.contact_id = input.contact_id;
    if input.latest_session_id.is_some() || replaces {
        record.latest_session_id = input.latest_session_id
    }
    if input.last_message_at.is_some() || replaces {
        record.last_message_at = input.last_message_at
    }
    record.status = status;
    record.last_bound_at = now.clone();
    record.updated_at = now;
    with_db(|pool|Box::pin(async move{let value=sqlx::query_scalar::<_,sqlx::types::Json<serde_json::Value>>("INSERT INTO chatos_project_agent_links(id,user_id,project_id,agent_id,contact_id,status,last_bound_at,created_at,updated_at,data) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT(user_id,project_id) DO UPDATE SET agent_id=EXCLUDED.agent_id,contact_id=EXCLUDED.contact_id,status=EXCLUDED.status,last_bound_at=EXCLUDED.last_bound_at,updated_at=EXCLUDED.updated_at,data=EXCLUDED.data RETURNING data").bind(&record.id).bind(&record.user_id).bind(&record.project_id).bind(&record.agent_id).bind(&record.contact_id).bind(&record.status).bind(timestamp(&record.last_bound_at)?).bind(timestamp(&record.created_at)?).bind(timestamp(&record.updated_at)?).bind(json(&record)?).fetch_one(pool).await.map_err(db_error)?;Ok(Some(serde_json::from_value(value.0).map_err(|e|e.to_string())?))})).await
}
#[derive(Debug, Clone)]
pub struct TouchProjectAgentLinkSessionInput {
    pub user_id: String,
    pub project_id: String,
    pub agent_id: String,
    pub contact_id: String,
    pub latest_session_id: String,
    pub last_message_at: String,
}
pub async fn touch_project_agent_link_session(
    input: TouchProjectAgentLinkSessionInput,
) -> Result<Option<ChatosProjectAgentLink>, String> {
    let project_id = normalize_project_id(&input.project_id);
    let updated_at = crate::core::time::now_rfc3339();
    let value=with_db(|pool|Box::pin(async move{sqlx::query_scalar::<_,sqlx::types::Json<serde_json::Value>>("UPDATE chatos_project_agent_links SET agent_id=$1,updated_at=$4,data=jsonb_set(jsonb_set(jsonb_set(jsonb_set(data,'{agent_id}',to_jsonb($1::text)),'{latest_session_id}',to_jsonb($2::text)),'{last_message_at}',to_jsonb($3::text)),'{updated_at}',to_jsonb($5::text)) WHERE user_id=$6 AND project_id=$7 AND contact_id=$8 AND status='active' RETURNING data").bind(&input.agent_id).bind(&input.latest_session_id).bind(&input.last_message_at).bind(timestamp(&updated_at)?).bind(&updated_at).bind(&input.user_id).bind(project_id).bind(&input.contact_id).fetch_optional(pool).await.map_err(db_error)})).await?;
    decode_optional(value)
}
pub async fn list_project_agent_links_by_contact(
    user_id: &str,
    contact_id: &str,
    status: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<ChatosProjectAgentLink>, String> {
    let status = normalize_optional_text(status);
    with_db(|pool|Box::pin(async move{decode_all(sqlx::query_scalar("SELECT data FROM chatos_project_agent_links WHERE user_id=$1 AND contact_id=$2 AND ($3::text IS NULL OR status=$3) ORDER BY last_bound_at DESC,updated_at DESC LIMIT $4 OFFSET $5").bind(user_id).bind(contact_id).bind(status).bind(limit.clamp(1,500)).bind(offset.max(0)).fetch_all(pool).await.map_err(db_error)?)})).await
}
pub async fn list_project_agent_links_by_project(
    user_id: &str,
    project_id: &str,
    status: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<ChatosProjectAgentLink>, String> {
    let project_id = normalize_project_id(project_id);
    let status = normalize_optional_text(status);
    with_db(|pool|Box::pin(async move{decode_all(sqlx::query_scalar("SELECT data FROM chatos_project_agent_links WHERE user_id=$1 AND project_id=$2 AND ($3::text IS NULL OR status=$3) ORDER BY last_bound_at DESC,updated_at DESC LIMIT $4 OFFSET $5").bind(user_id).bind(project_id).bind(status).bind(limit.clamp(1,500)).bind(offset.max(0)).fetch_all(pool).await.map_err(db_error)?)})).await
}
