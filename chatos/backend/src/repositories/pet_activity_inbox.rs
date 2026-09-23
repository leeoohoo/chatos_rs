// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::core::time::now_rfc3339;
use crate::models::pet_activity_inbox::{
    PetActivityInboxRecord, PetActivityInboxStatus, PetActivityInboxUpsert,
};
use crate::repositories::db::{
    db_error, decode_all, decode_optional, json, optional_timestamp, timestamp, with_db,
};
use uuid::Uuid;

pub async fn list_pet_activities(
    user_id: &str,
    include_closed: bool,
    limit: i64,
) -> Result<Vec<PetActivityInboxRecord>, String> {
    with_db(|pool|Box::pin(async move{decode_all(sqlx::query_scalar("SELECT data FROM pet_activity_inbox WHERE user_id=$1 AND ($2 OR (inbox_status=ANY($3) AND (expires_at IS NULL OR expires_at>now()))) ORDER BY occurred_at DESC,updated_at DESC LIMIT $4").bind(user_id).bind(include_closed).bind(vec!["unread","displayed"]).bind(limit.max(1)).fetch_all(pool).await.map_err(db_error)?)})).await
}
pub async fn upsert_pet_activity(
    input: PetActivityInboxUpsert,
) -> Result<PetActivityInboxRecord, String> {
    with_db(|pool|Box::pin(async move{let mut tx=pool.begin().await.map_err(db_error)?;let existing:Option<PetActivityInboxRecord>=decode_optional(sqlx::query_scalar("SELECT data FROM pet_activity_inbox WHERE user_id=$1 AND activity_key=$2 AND activity_version=$3 FOR UPDATE").bind(&input.user_id).bind(&input.activity_key).bind(&input.activity_version).fetch_optional(&mut *tx).await.map_err(db_error)?)?;let now=now_rfc3339();let mut record=existing.unwrap_or(PetActivityInboxRecord{id:format!("pet_{}",Uuid::new_v4().simple()),user_id:input.user_id.clone(),activity_key:input.activity_key.clone(),activity_version:input.activity_version.clone(),source:input.source.clone(),kind:input.kind.clone(),title:input.title.clone(),detail:None,route:input.route.clone(),business_status:input.business_status.clone(),inbox_status:PetActivityInboxStatus::Unread,requires_action:input.requires_action,event_id:None,event_sequence:None,metadata:None,occurred_at:input.occurred_at.clone(),displayed_at:None,acknowledged_at:None,ignored_at:None,handled_at:None,resolved_at:None,expires_at:None,created_at:now.clone(),updated_at:now.clone()});record.source=input.source;record.kind=input.kind;record.title=input.title;record.detail=input.detail;record.route=input.route;record.business_status=input.business_status;record.requires_action=input.requires_action;record.event_id=input.event_id;record.event_sequence=input.event_sequence;record.metadata=input.metadata;record.occurred_at=input.occurred_at;record.expires_at=input.expires_at;record.updated_at=now.clone();if input.resolved{record.inbox_status=PetActivityInboxStatus::Resolved;record.resolved_at=Some(now);}sqlx::query("INSERT INTO pet_activity_inbox(id,user_id,activity_key,activity_version,inbox_status,occurred_at,updated_at,expires_at,data) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT(user_id,activity_key,activity_version) DO UPDATE SET inbox_status=EXCLUDED.inbox_status,occurred_at=EXCLUDED.occurred_at,updated_at=EXCLUDED.updated_at,expires_at=EXCLUDED.expires_at,data=EXCLUDED.data").bind(&record.id).bind(&record.user_id).bind(&record.activity_key).bind(&record.activity_version).bind(record.inbox_status.as_str()).bind(timestamp(&record.occurred_at)?).bind(timestamp(&record.updated_at)?).bind(optional_timestamp(record.expires_at.as_deref())?).bind(json(&record)?).execute(&mut *tx).await.map_err(db_error)?;tx.commit().await.map_err(db_error)?;Ok(record)})).await
}
pub async fn transition_pet_activity(
    user_id: &str,
    activity_id: &str,
    status: PetActivityInboxStatus,
) -> Result<Option<PetActivityInboxRecord>, String> {
    with_db(|pool|Box::pin(async move{let mut tx=pool.begin().await.map_err(db_error)?;let Some(mut record):Option<PetActivityInboxRecord>=decode_optional(sqlx::query_scalar("SELECT data FROM pet_activity_inbox WHERE id=$1 AND user_id=$2 AND inbox_status=ANY($3) FOR UPDATE").bind(activity_id).bind(user_id).bind(vec!["unread","displayed"]).fetch_optional(&mut *tx).await.map_err(db_error)?)? else{tx.rollback().await.map_err(db_error)?;return Ok(None)};let now=now_rfc3339();record.inbox_status=status;record.updated_at=now.clone();match status{PetActivityInboxStatus::Displayed=>record.displayed_at=Some(now),PetActivityInboxStatus::Acknowledged=>record.acknowledged_at=Some(now),PetActivityInboxStatus::Ignored=>record.ignored_at=Some(now),PetActivityInboxStatus::Handled=>record.handled_at=Some(now),PetActivityInboxStatus::Resolved=>record.resolved_at=Some(now),PetActivityInboxStatus::Expired=>record.expires_at=Some(now),PetActivityInboxStatus::Unread=>{}}sqlx::query("UPDATE pet_activity_inbox SET inbox_status=$1,updated_at=$2,expires_at=$3,data=$4 WHERE id=$5").bind(status.as_str()).bind(timestamp(&record.updated_at)?).bind(optional_timestamp(record.expires_at.as_deref())?).bind(json(&record)?).bind(activity_id).execute(&mut *tx).await.map_err(db_error)?;tx.commit().await.map_err(db_error)?;Ok(Some(record))})).await
}
pub async fn mark_pet_activities_displayed(
    user_id: &str,
    activity_ids: &[String],
) -> Result<(), String> {
    if activity_ids.is_empty() {
        return Ok(());
    }
    with_db(|pool|Box::pin(async move{let now=now_rfc3339();sqlx::query("UPDATE pet_activity_inbox SET inbox_status='displayed',updated_at=$1,data=jsonb_set(jsonb_set(data,'{inbox_status}','\"displayed\"'),'{displayed_at}',to_jsonb($2::text)) WHERE user_id=$3 AND id=ANY($4) AND inbox_status='unread'").bind(timestamp(&now)?).bind(&now).bind(user_id).bind(activity_ids).execute(pool).await.map(|_|()).map_err(db_error)})).await
}
pub async fn update_pet_activity_detail(
    user_id: &str,
    activity_id: &str,
    detail: &str,
) -> Result<(), String> {
    with_db(|pool|Box::pin(async move{sqlx::query("UPDATE pet_activity_inbox SET data=jsonb_set(data,'{detail}',to_jsonb($1::text)) WHERE user_id=$2 AND id=$3 AND COALESCE(data->>'detail','')=''").bind(detail).bind(user_id).bind(activity_id).execute(pool).await.map(|_|()).map_err(db_error)})).await
}
