// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use serde_json::{json, Value};
use sqlx::types::Json as SqlJson;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use super::{
    internal_error, RearmQueueRequest, RearmRebuildRequest, RestorePendingRequest,
    ResummarizeRequest, RetireSummaryRequest,
};
use crate::api::memory_auth::MemoryAuthContext;
use crate::state::AppState;

pub async fn rearm_rebuild_backlog(
    State(state): State<Arc<AppState>>,
    auth: MemoryAuthContext,
    Json(request): Json<RearmRebuildRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.require_super_admin_or_operator()?;
    auth.ensure_tenant_scope(request.tenant_id.as_str())?;
    if request.reason.trim().len() < 8 {
        return Err((
            StatusCode::BAD_REQUEST,
            "reason must contain at least 8 characters".to_string(),
        ));
    }
    if ![
        "thread_l0",
        "thread_rollup",
        "project_memory",
        "agent_memory",
    ]
    .contains(&request.rebuild_kind.as_str())
    {
        return Err((StatusCode::BAD_REQUEST, "invalid rebuild_kind".to_string()));
    }
    let mut tx = state.pool.begin().await.map_err(internal_error)?;
    let before = sqlx::query_as::<_, (String, String, i64)>(
        "SELECT status,migration_run_id,attempt_count FROM engine_partition_rebuild_backlog \
          WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND project_partition_key=$4 \
            AND rebuild_kind=$5 FOR UPDATE",
    )
    .bind(request.tenant_id.trim())
    .bind(request.source_id.trim())
    .bind(request.thread_id.trim())
    .bind(request.project_partition_key.trim())
    .bind(request.rebuild_kind.as_str())
    .fetch_optional(&mut *tx)
    .await
    .map_err(internal_error)?
    .ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            "rebuild backlog item not found".to_string(),
        )
    })?;
    if before.0 != "failed" && !(before.0 == "pending" && before.2 > 0) {
        return Err((
            StatusCode::CONFLICT,
            "only failed or retry-counted pending rebuild backlog items can be rearmed".to_string(),
        ));
    }
    sqlx::query(
        "UPDATE engine_partition_rebuild_backlog SET status='pending',attempt_count=0, \
         lease_owner=NULL,lease_token=NULL,lease_expires_at=NULL,last_error=NULL,completed_at=NULL, \
         reason=$6,updated_at=now() WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 \
           AND project_partition_key=$4 AND rebuild_kind=$5 \
           AND (status='failed' OR (status='pending' AND attempt_count>0))",
    )
    .bind(request.tenant_id.trim())
    .bind(request.source_id.trim())
    .bind(request.thread_id.trim())
    .bind(request.project_partition_key.trim())
    .bind(request.rebuild_kind.as_str())
    .bind(request.reason.trim())
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;
    sqlx::query(
        "UPDATE engine_reliability_migration_runs SET status='running',finished_at=NULL, \
         error_detail=NULL,cursor=$2 WHERE id=$1 AND status='failed'",
    )
    .bind(&before.1)
    .bind(SqlJson(json!({"phase": "rearmed", "completed_items": 0})))
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;
    let action_id = format!("rop_{}", Uuid::new_v4());
    sqlx::query(
        "INSERT INTO engine_reliability_operator_actions( \
         id,tenant_id,source_id,operator_id,action_kind,object_kind,object_id, \
         project_partition_key,reason,before_state,after_state) \
         VALUES($1,$2,$3,$4,'rearm','rebuild_backlog',$5,$6,$7,$8,$9)",
    )
    .bind(&action_id)
    .bind(request.tenant_id.trim())
    .bind(request.source_id.trim())
    .bind(auth.operator_audit_id())
    .bind(format!(
        "{}:{}",
        request.thread_id.trim(),
        request.rebuild_kind
    ))
    .bind(request.project_partition_key.trim())
    .bind(request.reason.trim())
    .bind(SqlJson(
        json!({"status": before.0, "attempt_count": before.2}),
    ))
    .bind(SqlJson(json!({"status": "pending", "attempt_count": 0})))
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;
    tx.commit().await.map_err(internal_error)?;
    Ok(Json(json!({
        "operation_id": action_id,
        "status": "pending"
    })))
}

pub async fn resummarize_covered_range(
    State(state): State<Arc<AppState>>,
    auth: MemoryAuthContext,
    Json(request): Json<ResummarizeRequest>,
) -> Result<Json<crate::models::RunPendingSummaryRebuildResponse>, (StatusCode, String)> {
    auth.require_super_admin_or_operator()?;
    auth.ensure_tenant_scope(request.tenant_id.as_str())?;
    if request.summary_id.trim().is_empty() || request.reason.trim().len() < 8 {
        return Err((
            StatusCode::BAD_REQUEST,
            "summary_id is required and reason must contain at least 8 characters".to_string(),
        ));
    }
    let mut tx = state.pool.begin().await.map_err(internal_error)?;
    let prepared = crate::services::summary::prepare_covered_range_resummarize(
        &state.config,
        &state.pool,
        &mut tx,
        request.tenant_id.trim(),
        request.source_id.trim(),
        request.summary_id.trim(),
        request.reason.trim(),
    )
    .await
    .map_err(|error| (StatusCode::CONFLICT, error))?;
    insert_operator_action(
        &mut tx,
        &auth,
        request.tenant_id.trim(),
        request.source_id.trim(),
        "resummarize",
        "thread_summary",
        request.summary_id.trim(),
        Some(prepared.project_partition_key.as_str()),
        request.reason.trim(),
        json!({
            "thread_id": prepared.summary.thread_id.as_str(),
            "status": prepared.summary.status.as_str(),
            "recall_eligible": true,
        }),
        json!({
            "job_run_id": prepared.job_run_id.as_str(),
            "accepted": true,
            "running": true,
            "claim_status": "running",
        }),
    )
    .await?;
    tx.commit().await.map_err(internal_error)?;
    let response = crate::services::summary::execute_prepared_covered_range_resummarize(
        &state.config,
        &state.pool,
        request.tenant_id.trim(),
        request.source_id.trim(),
        prepared,
    )
    .await
    .map_err(|error| (StatusCode::CONFLICT, error))?;
    Ok(Json(response))
}

pub async fn retire_summary(
    State(state): State<Arc<AppState>>,
    auth: MemoryAuthContext,
    Json(request): Json<RetireSummaryRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.require_super_admin_or_operator()?;
    auth.ensure_tenant_scope(request.tenant_id.as_str())?;
    validate_write_reason(&request.summary_id, &request.reason)?;
    let mut tx = state.pool.begin().await.map_err(internal_error)?;
    let before = sqlx::query_as::<_, (String, String, String, bool)>(
        "SELECT thread_id,project_partition_key,status,recall_eligible FROM engine_summaries \
         WHERE tenant_id=$1 AND source_id=$2 AND id=$3 FOR UPDATE",
    )
    .bind(request.tenant_id.trim())
    .bind(request.source_id.trim())
    .bind(request.summary_id.trim())
    .fetch_optional(&mut *tx)
    .await
    .map_err(internal_error)?
    .ok_or_else(|| (StatusCode::NOT_FOUND, "summary not found".to_string()))?;
    let restored = crate::repositories::summaries::delete_thread_summary_in_tx(
        &mut tx,
        &before.0,
        request.summary_id.trim(),
        Some(request.tenant_id.trim()),
        Some(request.source_id.trim()),
        &before.1,
    )
    .await
    .map_err(|error| (StatusCode::CONFLICT, error))?;
    let action_id = insert_operator_action(
        &mut tx,
        &auth,
        request.tenant_id.trim(),
        request.source_id.trim(),
        "retire",
        "thread_summary",
        request.summary_id.trim(),
        Some(&before.1),
        request.reason.trim(),
        json!({"status":before.2,"recall_eligible":before.3}),
        json!({"status":"retired","recall_eligible":false,"restored_records":restored}),
    )
    .await?;
    tx.commit().await.map_err(internal_error)?;
    Ok(Json(
        json!({"operation_id":action_id,"restored_records":restored}),
    ))
}

pub async fn restore_record_pending(
    State(state): State<Arc<AppState>>,
    auth: MemoryAuthContext,
    Json(request): Json<RestorePendingRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.require_super_admin_or_operator()?;
    auth.ensure_tenant_scope(request.tenant_id.as_str())?;
    validate_write_reason(&request.record_id, &request.reason)?;
    let mut tx = state.pool.begin().await.map_err(internal_error)?;
    let result = crate::repositories::records::restore_record_pending_in_tx(
        &mut tx,
        request.tenant_id.trim(),
        request.source_id.trim(),
        request.record_id.trim(),
    )
    .await
    .map_err(|error| (StatusCode::CONFLICT, error))?;
    let action_id = insert_operator_action(
        &mut tx,
        &auth,
        request.tenant_id.trim(),
        request.source_id.trim(),
        "restore_pending",
        "record",
        request.record_id.trim(),
        Some(&result.project_partition_key),
        request.reason.trim(),
        json!({"summary_status":result.previous_status}),
        json!({"summary_status":"pending","changed":result.changed,"thread_id":result.thread_id}),
    )
    .await?;
    tx.commit().await.map_err(internal_error)?;
    Ok(Json(
        json!({"operation_id":action_id,"changed":result.changed}),
    ))
}

pub async fn rearm_reliability_queue(
    State(state): State<Arc<AppState>>,
    auth: MemoryAuthContext,
    Json(request): Json<RearmQueueRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    auth.require_super_admin_or_operator()?;
    auth.ensure_tenant_scope(request.tenant_id.as_str())?;
    validate_write_reason(&request.item_id, &request.reason)?;
    if request.version < 1 {
        return Err((
            StatusCode::BAD_REQUEST,
            "version must be positive".to_string(),
        ));
    }
    let mut tx = state.pool.begin().await.map_err(internal_error)?;
    let rearmed_id = match request.stream.as_str() {
        "derived_event" => crate::repositories::derived_events::rearm_dead_lettered_event_in_tx(
            &mut tx, request.tenant_id.trim(), request.source_id.trim(), request.item_id.trim(),
        ).await.map_err(|error| (StatusCode::CONFLICT, error))?,
        "summary" => {
            let partition = request.project_partition_key.as_deref().ok_or_else(|| (
                StatusCode::BAD_REQUEST, "summary queue rearm requires project_partition_key".to_string(),
            ))?;
            crate::repositories::derived_events::rearm_dead_lettered_thread_summary_in_tx(
                &mut tx, request.tenant_id.trim(), request.source_id.trim(), request.item_id.trim(),
                partition, request.version,
            ).await.map_err(|error| (StatusCode::CONFLICT, error))?
        }
        "rollup" => crate::repositories::summaries::replay_dead_lettered_rollup_dispatch_in_tx(
            &mut tx, request.tenant_id.trim(), request.source_id.trim(), request.item_id.trim(),
            request.version,
        ).await.map_err(|error| (StatusCode::CONFLICT, error))?.map(|row| row.id),
        "subject_memory" => crate::repositories::subject_memory_scopes::replay_dead_lettered_subject_memory_dispatch_in_tx(
            &mut tx, request.tenant_id.trim(), request.source_id.trim(), request.item_id.trim(),
            request.version,
        ).await.map_err(|error| (StatusCode::CONFLICT, error))?.map(|row| row.id),
        _ => return Err((StatusCode::BAD_REQUEST, "queue stream does not support rearm".to_string())),
    }.ok_or_else(|| (StatusCode::CONFLICT, "queue item is not in the requested dead-letter state".to_string()))?;
    let action_id = insert_operator_action(
        &mut tx,
        &auth,
        request.tenant_id.trim(),
        request.source_id.trim(),
        "rearm",
        "reliability_queue_item",
        request.item_id.trim(),
        request.project_partition_key.as_deref(),
        request.reason.trim(),
        json!({"stream":request.stream,"version":request.version,"status":"dead_letter"}),
        json!({"rearmed_id":rearmed_id,"status":"pending"}),
    )
    .await?;
    tx.commit().await.map_err(internal_error)?;
    Ok(Json(
        json!({"operation_id":action_id,"rearmed_id":rearmed_id}),
    ))
}

#[allow(clippy::too_many_arguments)]
async fn insert_operator_action(
    tx: &mut Transaction<'_, Postgres>,
    auth: &MemoryAuthContext,
    tenant_id: &str,
    source_id: &str,
    action_kind: &str,
    object_kind: &str,
    object_id: &str,
    project_partition_key: Option<&str>,
    reason: &str,
    before_state: Value,
    after_state: Value,
) -> Result<String, (StatusCode, String)> {
    let action_id = format!("rop_{}", Uuid::new_v4());
    sqlx::query(
        "INSERT INTO engine_reliability_operator_actions( \
         id,tenant_id,source_id,operator_id,action_kind,object_kind,object_id, \
         project_partition_key,reason,before_state,after_state) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
    )
    .bind(&action_id)
    .bind(tenant_id)
    .bind(source_id)
    .bind(auth.operator_audit_id())
    .bind(action_kind)
    .bind(object_kind)
    .bind(object_id)
    .bind(project_partition_key)
    .bind(reason)
    .bind(SqlJson(before_state))
    .bind(SqlJson(after_state))
    .execute(&mut **tx)
    .await
    .map_err(internal_error)?;
    Ok(action_id)
}

fn validate_write_reason(object_id: &str, reason: &str) -> Result<(), (StatusCode, String)> {
    if object_id.trim().is_empty() || reason.trim().len() < 8 {
        return Err((
            StatusCode::BAD_REQUEST,
            "object identity is required and reason must contain at least 8 characters".to_string(),
        ));
    }
    Ok(())
}
