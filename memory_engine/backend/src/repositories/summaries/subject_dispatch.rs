// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::db::Db;
use serde::Deserialize;
use sqlx::FromRow;
#[derive(Debug, Clone, Deserialize, FromRow, PartialEq, Eq)]
pub struct SubjectMemorySourceDispatchOutbox {
    pub id: String,
    pub tenant_id: String,
    pub source_id: String,
    pub thread_id: String,
    pub summary_type: String,
    pub subject_memory_source_dispatch_version: i64,
    pub subject_memory_source_dispatch_published_version: i64,
    pub subject_memory_source_dispatch_consumed_version: i64,
    pub subject_memory_source_dispatch_pending: bool,
}
const COLS:&str="id,tenant_id,source_id,thread_id,summary_type,subject_memory_source_dispatch_version,subject_memory_source_dispatch_published_version,subject_memory_source_dispatch_consumed_version,subject_memory_source_dispatch_pending";
pub async fn get_pending_subject_memory_source_dispatch(
    db: &Db,
    t: &str,
    s: &str,
    id: &str,
) -> Result<Option<SubjectMemorySourceDispatchOutbox>, String> {
    fetch(db,&format!("SELECT {COLS} FROM engine_summaries WHERE tenant_id=$1 AND source_id=$2 AND id=$3 AND subject_memory_source_dispatch_pending"),t,s,id).await
}
pub async fn get_subject_memory_source_dispatch_state(
    db: &Db,
    t: &str,
    s: &str,
    id: &str,
) -> Result<Option<SubjectMemorySourceDispatchOutbox>, String> {
    fetch(
        db,
        &format!(
            "SELECT {COLS} FROM engine_summaries WHERE tenant_id=$1 AND source_id=$2 AND id=$3"
        ),
        t,
        s,
        id,
    )
    .await
}
pub async fn list_pending_subject_memory_source_dispatches(
    db: &Db,
    limit: i64,
) -> Result<Vec<SubjectMemorySourceDispatchOutbox>, String> {
    sqlx::query_as(&format!("SELECT {COLS} FROM engine_summaries WHERE subject_memory_source_dispatch_pending ORDER BY subject_memory_source_dispatch_requested_at,updated_at LIMIT $1")).bind(limit.clamp(1,10000)).fetch_all(db).await.map_err(|e|e.to_string())
}
pub async fn mark_subject_memory_source_dispatch_published(
    db: &Db,
    e: &SubjectMemorySourceDispatchOutbox,
) -> Result<bool, String> {
    update(db,e,"subject_memory_source_dispatch_published_version=GREATEST(subject_memory_source_dispatch_published_version,$4),subject_memory_source_dispatch_published_at=now(),subject_memory_source_dispatch_last_error=NULL,subject_memory_source_dispatch_pending=(subject_memory_source_dispatch_version>$4)",None).await
}
pub async fn mark_subject_memory_source_dispatch_consumed(
    db: &Db,
    e: &SubjectMemorySourceDispatchOutbox,
) -> Result<bool, String> {
    update(db,e,"subject_memory_source_dispatch_consumed_version=GREATEST(subject_memory_source_dispatch_consumed_version,$4),subject_memory_source_dispatch_consumed_at=now(),subject_memory_source_dispatch_last_error=NULL",None).await
}
pub async fn mark_subject_memory_source_dispatch_failed(
    db: &Db,
    e: &SubjectMemorySourceDispatchOutbox,
    error: &str,
) -> Result<bool, String> {
    update(db,e,"subject_memory_source_dispatch_last_error=$5,subject_memory_source_dispatch_last_failed_at=now()",Some(error)).await
}
pub async fn mark_subject_memory_source_dispatch_dead_lettered(
    db: &Db,
    e: &SubjectMemorySourceDispatchOutbox,
    error: &str,
) -> Result<bool, String> {
    update(db,e,"subject_memory_source_dispatch_consumed_version=GREATEST(subject_memory_source_dispatch_consumed_version,$4),subject_memory_source_dispatch_dead_letter_version=$4,subject_memory_source_dispatch_dead_lettered_at=now(),subject_memory_source_dispatch_last_error=$5",Some(error)).await
}
pub async fn replay_dead_lettered_subject_memory_source_dispatch(
    db: &Db,
    t: &str,
    s: &str,
    id: &str,
    v: i64,
) -> Result<Option<SubjectMemorySourceDispatchOutbox>, String> {
    sqlx::query_as(&format!("UPDATE engine_summaries SET subject_memory_source_dispatch_version=subject_memory_source_dispatch_version+1,subject_memory_source_dispatch_requested_at=now(),subject_memory_source_dispatch_last_error=NULL,subject_memory_source_dispatch_pending=true,subject_memory_source_dispatch_dead_letter_version=NULL,subject_memory_source_dispatch_dead_lettered_at=NULL,subject_memory_source_dispatch_last_failed_at=NULL WHERE tenant_id=$1 AND source_id=$2 AND id=$3 AND status='done' AND subject_memory_source_dispatch_version=$4 AND subject_memory_source_dispatch_dead_letter_version=$4 AND subject_memory_source_dispatch_consumed_version>=$4 AND NOT subject_memory_source_dispatch_pending RETURNING {COLS}")).bind(t).bind(s).bind(id).bind(v).fetch_optional(db).await.map_err(|e|e.to_string())
}
async fn fetch(
    db: &Db,
    q: &str,
    t: &str,
    s: &str,
    id: &str,
) -> Result<Option<SubjectMemorySourceDispatchOutbox>, String> {
    sqlx::query_as(q)
        .bind(t)
        .bind(s)
        .bind(id)
        .fetch_optional(db)
        .await
        .map_err(|e| e.to_string())
}
async fn update(
    db: &Db,
    e: &SubjectMemorySourceDispatchOutbox,
    set: &str,
    error: Option<&str>,
) -> Result<bool, String> {
    let q=format!("UPDATE engine_summaries SET {set} WHERE tenant_id=$1 AND source_id=$2 AND id=$3 AND subject_memory_source_dispatch_version>=$4");
    let mut query = sqlx::query(&q)
        .bind(&e.tenant_id)
        .bind(&e.source_id)
        .bind(&e.id)
        .bind(e.subject_memory_source_dispatch_version);
    if let Some(v) = error {
        query = query.bind(v);
    }
    Ok(query
        .execute(db)
        .await
        .map_err(|e| e.to_string())?
        .rows_affected()
        > 0)
}
