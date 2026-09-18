// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::db::Db;
use serde::Deserialize;
use sqlx::FromRow;
#[derive(Debug, Clone, Deserialize, FromRow, PartialEq, Eq)]
pub struct RollupDispatchOutbox {
    pub id: String,
    pub tenant_id: String,
    pub source_id: String,
    pub thread_id: String,
    pub rollup_dispatch_version: i64,
    pub rollup_dispatch_published_version: i64,
    pub rollup_dispatch_consumed_version: i64,
    pub rollup_dispatch_pending: bool,
}
const COLS:&str="id,tenant_id,source_id,thread_id,rollup_dispatch_version,rollup_dispatch_published_version,rollup_dispatch_consumed_version,rollup_dispatch_pending";
pub async fn get_pending_rollup_dispatch(
    db: &Db,
    t: &str,
    s: &str,
    id: &str,
) -> Result<Option<RollupDispatchOutbox>, String> {
    fetch(db,&format!("SELECT {COLS} FROM engine_summaries WHERE tenant_id=$1 AND source_id=$2 AND id=$3 AND rollup_dispatch_pending"),t,s,id).await
}
pub async fn get_rollup_dispatch_state(
    db: &Db,
    t: &str,
    s: &str,
    id: &str,
) -> Result<Option<RollupDispatchOutbox>, String> {
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
pub async fn list_pending_rollup_dispatches(
    db: &Db,
    limit: i64,
) -> Result<Vec<RollupDispatchOutbox>, String> {
    sqlx::query_as(&format!("SELECT {COLS} FROM engine_summaries WHERE rollup_dispatch_pending ORDER BY rollup_dispatch_requested_at,updated_at LIMIT $1")).bind(limit.clamp(1,10000)).fetch_all(db).await.map_err(|e|e.to_string())
}
pub async fn mark_rollup_dispatch_published(
    db: &Db,
    e: &RollupDispatchOutbox,
) -> Result<bool, String> {
    update(db,e,"rollup_dispatch_published_version=GREATEST(rollup_dispatch_published_version,$4),rollup_dispatch_published_at=now(),rollup_dispatch_last_error=NULL,rollup_dispatch_pending=(rollup_dispatch_version>$4)",None).await
}
pub async fn mark_rollup_dispatch_consumed(
    db: &Db,
    e: &RollupDispatchOutbox,
) -> Result<bool, String> {
    update(db,e,"rollup_dispatch_consumed_version=GREATEST(rollup_dispatch_consumed_version,$4),rollup_dispatch_consumed_at=now(),rollup_dispatch_last_error=NULL",None).await
}
pub async fn mark_rollup_dispatch_failed(
    db: &Db,
    e: &RollupDispatchOutbox,
    error: &str,
) -> Result<bool, String> {
    update(
        db,
        e,
        "rollup_dispatch_last_error=$5,rollup_dispatch_last_failed_at=now()",
        Some(error),
    )
    .await
}
pub async fn mark_rollup_dispatch_dead_lettered(
    db: &Db,
    e: &RollupDispatchOutbox,
    error: &str,
) -> Result<bool, String> {
    update(db,e,"rollup_dispatch_consumed_version=GREATEST(rollup_dispatch_consumed_version,$4),rollup_dispatch_dead_letter_version=$4,rollup_dispatch_dead_lettered_at=now(),rollup_dispatch_last_error=$5",Some(error)).await
}
pub async fn rearm_rollup_dispatch_if_eligible(
    db: &Db,
    t: &str,
    s: &str,
    thread: &str,
    max_level: i64,
) -> Result<Option<RollupDispatchOutbox>, String> {
    if let Some(value)=sqlx::query_as::<_,RollupDispatchOutbox>(&format!("SELECT {COLS} FROM engine_summaries WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND summary_type='thread_incremental' AND status='done' AND rollup_status='pending' AND level<=$4 AND rollup_dispatch_consumed_version<rollup_dispatch_version ORDER BY created_at LIMIT 1")).bind(t).bind(s).bind(thread).bind(max_level.max(0)).fetch_optional(db).await.map_err(|e|e.to_string())?{return Ok(Some(value));}
    sqlx::query_as(&format!("UPDATE engine_summaries SET rollup_dispatch_version=rollup_dispatch_version+1,rollup_dispatch_requested_at=now(),rollup_dispatch_last_error=NULL,rollup_dispatch_pending=true WHERE id=(SELECT id FROM engine_summaries WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND summary_type='thread_incremental' AND status='done' AND rollup_status='pending' AND level<=$4 AND rollup_dispatch_consumed_version>=rollup_dispatch_version AND COALESCE(rollup_dispatch_dead_letter_version,-1)<rollup_dispatch_version ORDER BY created_at LIMIT 1 FOR UPDATE SKIP LOCKED) RETURNING {COLS}"))
        .bind(t).bind(s).bind(thread).bind(max_level.max(0)).fetch_optional(db).await.map_err(|e|e.to_string())
}
pub async fn replay_dead_lettered_rollup_dispatch(
    db: &Db,
    t: &str,
    s: &str,
    id: &str,
    v: i64,
) -> Result<Option<RollupDispatchOutbox>, String> {
    sqlx::query_as(&format!("UPDATE engine_summaries SET rollup_dispatch_version=rollup_dispatch_version+1,rollup_dispatch_requested_at=now(),rollup_dispatch_last_error=NULL,rollup_dispatch_pending=true,rollup_dispatch_dead_letter_version=NULL,rollup_dispatch_dead_lettered_at=NULL,rollup_dispatch_last_failed_at=NULL WHERE tenant_id=$1 AND source_id=$2 AND id=$3 AND status='done' AND rollup_status='pending' AND rollup_dispatch_version=$4 AND rollup_dispatch_dead_letter_version=$4 AND rollup_dispatch_consumed_version>=$4 AND NOT rollup_dispatch_pending RETURNING {COLS}")).bind(t).bind(s).bind(id).bind(v).fetch_optional(db).await.map_err(|e|e.to_string())
}
async fn fetch(
    db: &Db,
    q: &str,
    t: &str,
    s: &str,
    id: &str,
) -> Result<Option<RollupDispatchOutbox>, String> {
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
    e: &RollupDispatchOutbox,
    set: &str,
    error: Option<&str>,
) -> Result<bool, String> {
    let q=format!("UPDATE engine_summaries SET {set} WHERE tenant_id=$1 AND source_id=$2 AND id=$3 AND rollup_dispatch_version>=$4");
    let mut query = sqlx::query(&q)
        .bind(&e.tenant_id)
        .bind(&e.source_id)
        .bind(&e.id)
        .bind(e.rollup_dispatch_version);
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
