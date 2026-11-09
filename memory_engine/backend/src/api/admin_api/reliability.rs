// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::{collections::BTreeMap, sync::Arc};

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::types::Json as SqlJson;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::api::memory_auth::MemoryAuthContext;
use crate::models::{ComposeContextMeta, ComposeContextPolicy, ComposeContextRequest};
use crate::services::context;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct ReliabilityScopeQuery {
    tenant_id: String,
    source_id: String,
    limit: Option<i64>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct MigrationRunRow {
    id: String,
    migration_kind: String,
    tenant_id: String,
    source_id: String,
    status: String,
    cursor: SqlJson<serde_json::Value>,
    statistics: SqlJson<serde_json::Value>,
    error_detail: Option<SqlJson<serde_json::Value>>,
    started_at: DateTime<Utc>,
    finished_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct RebuildBacklogRow {
    tenant_id: String,
    source_id: String,
    thread_id: String,
    project_partition_key: String,
    rebuild_kind: String,
    status: String,
    migration_run_id: String,
    reason: String,
    attempt_count: i64,
    last_error: Option<SqlJson<serde_json::Value>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct CoverageRow {
    id: String,
    summary_id: String,
    record_id: String,
    thread_id: String,
    project_partition_key: String,
    ordinal: i64,
    source_ref: String,
    status: String,
    generation_id: String,
    created_at: DateTime<Utc>,
    retired_at: Option<DateTime<Utc>>,
    retired_reason: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct LineageRow {
    id: String,
    derivation_kind: String,
    output_scope_key: String,
    input_project_partition_key: String,
    output_kind: String,
    output_id: String,
    input_kind: String,
    input_id: String,
    ordinal: i64,
    status: String,
    generation_id: String,
    created_at: DateTime<Utc>,
    retired_at: Option<DateTime<Utc>>,
    retired_reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ComposeDryRunItem {
    id: String,
    item_type: String,
    pool: String,
    project_partition_key: String,
    estimated_tokens: usize,
    score_components: BTreeMap<String, f64>,
    selection_reason: String,
}

#[derive(Debug, Serialize)]
pub struct ComposeDryRunResponse {
    thread_id: String,
    selected: Vec<ComposeDryRunItem>,
    meta: ComposeContextMeta,
}

#[derive(Debug, Deserialize)]
pub struct ComposeDryRunRequest {
    tenant_id: String,
    source_id: String,
    subject_id: Option<String>,
    context_project_subject_id: Option<String>,
    thread_id: String,
    query_text: Option<String>,
    current_turn_id: Option<String>,
    model_capability_revision: String,
    policy: ComposeContextPolicy,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ReliabilityQueueRow {
    stream: String,
    item_id: String,
    project_partition_key: Option<String>,
    status: String,
    version: i64,
    error_code: Option<String>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct OperatorActionRow {
    id: String,
    tenant_id: String,
    source_id: String,
    operator_id: String,
    action_kind: String,
    object_kind: String,
    object_id: String,
    project_partition_key: Option<String>,
    reason: String,
    before_state: SqlJson<Value>,
    after_state: SqlJson<Value>,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct RearmRebuildRequest {
    tenant_id: String,
    source_id: String,
    thread_id: String,
    project_partition_key: String,
    rebuild_kind: String,
    reason: String,
}

#[derive(Debug, Deserialize)]
pub struct ResummarizeRequest {
    tenant_id: String,
    source_id: String,
    summary_id: String,
    reason: String,
}

#[derive(Debug, Deserialize)]
pub struct RetireSummaryRequest {
    tenant_id: String,
    source_id: String,
    summary_id: String,
    reason: String,
}

#[derive(Debug, Deserialize)]
pub struct RestorePendingRequest {
    tenant_id: String,
    source_id: String,
    record_id: String,
    reason: String,
}

#[derive(Debug, Deserialize)]
pub struct RearmQueueRequest {
    tenant_id: String,
    source_id: String,
    stream: String,
    item_id: String,
    project_partition_key: Option<String>,
    version: i64,
    reason: String,
}

pub async fn list_migration_runs(
    State(state): State<Arc<AppState>>,
    auth: MemoryAuthContext,
    Query(query): Query<ReliabilityScopeQuery>,
) -> Result<Json<Vec<MigrationRunRow>>, (StatusCode, String)> {
    let (tenant, source, limit) = validated_scope(&auth, &query)?;
    sqlx::query_as(
        "SELECT id,migration_kind,tenant_id,source_id,status,cursor,statistics,error_detail, \
                started_at,finished_at FROM engine_reliability_migration_runs \
         WHERE tenant_id=$1 AND source_id=$2 ORDER BY started_at DESC,id DESC LIMIT $3",
    )
    .bind(tenant)
    .bind(source)
    .bind(limit)
    .fetch_all(&state.pool)
    .await
    .map(Json)
    .map_err(internal_error)
}

pub async fn reliability_dashboard(
    State(state): State<Arc<AppState>>,
    auth: MemoryAuthContext,
    Query(query): Query<ReliabilityScopeQuery>,
) -> Result<Json<crate::reliability_audit::ReliabilityAuditReport>, (StatusCode, String)> {
    let (tenant, source, _) = validated_scope(&auth, &query)?;
    crate::reliability_audit::run_reliability_audit(
        &state.pool,
        crate::reliability_audit::ReliabilityAuditScope {
            tenant_id: Some(tenant.to_string()),
            source_id: Some(source.to_string()),
        },
    )
    .await
    .map(Json)
    .map_err(|error| {
        tracing::error!(error = %error, "build Memory Engine reliability dashboard failed");
        (StatusCode::INTERNAL_SERVER_ERROR, "build reliability dashboard failed".to_string())
    })
}

pub async fn list_rebuild_backlog(
    State(state): State<Arc<AppState>>,
    auth: MemoryAuthContext,
    Query(query): Query<ReliabilityScopeQuery>,
) -> Result<Json<Vec<RebuildBacklogRow>>, (StatusCode, String)> {
    let (tenant, source, limit) = validated_scope(&auth, &query)?;
    sqlx::query_as(
        "SELECT tenant_id,source_id,thread_id,project_partition_key,rebuild_kind,status, \
                migration_run_id,reason,attempt_count,last_error,created_at,updated_at,completed_at \
           FROM engine_partition_rebuild_backlog WHERE tenant_id=$1 AND source_id=$2 \
          ORDER BY CASE status WHEN 'failed' THEN 0 WHEN 'pending' THEN 1 WHEN 'running' THEN 2 ELSE 3 END, \
                   updated_at,thread_id,project_partition_key,rebuild_kind LIMIT $3",
    )
    .bind(tenant)
    .bind(source)
    .bind(limit)
    .fetch_all(&state.pool)
    .await
    .map(Json)
    .map_err(internal_error)
}

pub async fn record_coverage(
    State(state): State<Arc<AppState>>,
    auth: MemoryAuthContext,
    Path(record_id): Path<String>,
    Query(query): Query<ReliabilityScopeQuery>,
) -> Result<Json<Vec<CoverageRow>>, (StatusCode, String)> {
    let (tenant, source, limit) = validated_scope(&auth, &query)?;
    if record_id.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "record_id must not be empty".to_string(),
        ));
    }
    sqlx::query_as(
        "SELECT id,summary_id,record_id,thread_id,project_partition_key,ordinal,source_ref,status, \
                generation_id,created_at,retired_at,retired_reason \
           FROM engine_summary_record_coverage \
          WHERE tenant_id=$1 AND source_id=$2 AND record_id=$3 \
          ORDER BY CASE status WHEN 'active' THEN 0 WHEN 'superseded' THEN 1 ELSE 2 END,created_at DESC \
          LIMIT $4",
    )
    .bind(tenant)
    .bind(source)
    .bind(record_id.trim())
    .bind(limit)
    .fetch_all(&state.pool)
    .await
    .map(Json)
    .map_err(internal_error)
}

pub async fn output_lineage(
    State(state): State<Arc<AppState>>,
    auth: MemoryAuthContext,
    Path((output_kind, output_id)): Path<(String, String)>,
    Query(query): Query<ReliabilityScopeQuery>,
) -> Result<Json<Vec<LineageRow>>, (StatusCode, String)> {
    let (tenant, source, limit) = validated_scope(&auth, &query)?;
    if !["thread_summary", "subject_memory"].contains(&output_kind.as_str())
        || output_id.trim().is_empty()
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "invalid output identity".to_string(),
        ));
    }
    sqlx::query_as(
        "SELECT id,derivation_kind,output_scope_key,input_project_partition_key,output_kind, \
                output_id,input_kind,input_id,ordinal,status,generation_id,created_at,retired_at, \
                retired_reason FROM engine_memory_derivation_edges \
          WHERE tenant_id=$1 AND source_id=$2 AND output_kind=$3 AND output_id=$4 \
          ORDER BY CASE status WHEN 'active' THEN 0 WHEN 'superseded' THEN 1 ELSE 2 END,ordinal \
          LIMIT $5",
    )
    .bind(tenant)
    .bind(source)
    .bind(output_kind)
    .bind(output_id.trim())
    .bind(limit)
    .fetch_all(&state.pool)
    .await
    .map(Json)
    .map_err(internal_error)
}

pub async fn compose_dry_run(
    State(state): State<Arc<AppState>>,
    auth: MemoryAuthContext,
    Json(input): Json<ComposeDryRunRequest>,
) -> Result<Json<ComposeDryRunResponse>, (StatusCode, String)> {
    auth.ensure_tenant_scope(input.tenant_id.as_str())?;
    let capability = state
        .config
        .model_capabilities
        .get(input.model_capability_revision.as_str())
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                "model_capability_revision is not published".to_string(),
            )
        })?;
    let mut policy = input.policy;
    policy.max_memory_tokens = policy.max_memory_tokens.min(
        capability
            .context_window_tokens
            .saturating_sub(capability.output_reserve_tokens),
    );
    let request = ComposeContextRequest {
        tenant_id: input.tenant_id,
        source_id: input.source_id,
        subject_id: input.subject_id,
        context_project_subject_id: input.context_project_subject_id,
        thread_id: input.thread_id,
        query_text: input.query_text,
        current_turn_id: input.current_turn_id,
        model_capability_revision: input.model_capability_revision,
        runtime_context_assertion: "admin-compose-dry-run".to_string(),
        policy,
    };
    let response = context::compose_context(&state.pool, request)
        .await
        .map_err(|message| (StatusCode::BAD_REQUEST, message))?;
    let mut selected = response
        .blocks
        .into_iter()
        .map(|block| ComposeDryRunItem {
            id: block.block_id,
            item_type: block.block_type,
            pool: block.pool,
            project_partition_key: block.project_partition_key,
            estimated_tokens: block.estimated_tokens,
            score_components: block.score_components,
            selection_reason: block.selection_reason,
        })
        .collect::<Vec<_>>();
    selected.extend(response.recent_records.into_iter().map(|record| {
        ComposeDryRunItem {
            id: record.id,
            item_type: record.record_type,
            pool: "raw_tail".to_string(),
            project_partition_key: record
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("memory_project_id"))
                .and_then(Value::as_str)
                .unwrap_or("__none__")
                .to_string(),
            estimated_tokens: context::estimate_tokens(record.content.as_str()).saturating_add(12),
            score_components: BTreeMap::new(),
            selection_reason: "recent_atomic_source_unit".to_string(),
        }
    }));
    Ok(Json(ComposeDryRunResponse {
        thread_id: response.thread_id,
        selected,
        meta: response.meta,
    }))
}

pub async fn list_reliability_queues(
    State(state): State<Arc<AppState>>,
    auth: MemoryAuthContext,
    Query(query): Query<ReliabilityScopeQuery>,
) -> Result<Json<Vec<ReliabilityQueueRow>>, (StatusCode, String)> {
    let (tenant, source, limit) = validated_scope(&auth, &query)?;
    sqlx::query_as(
        "SELECT * FROM ( \
         SELECT 'derived_event'::TEXT AS stream,event_id AS item_id,project_partition_key,status, \
                dispatch_version AS version,last_error_code AS error_code, \
                COALESCE(dead_lettered_at,published_at,lease_expires_at,created_at) AS updated_at \
           FROM engine_derived_event_outbox WHERE tenant_id=$1 AND source_id=$2 \
             AND status IN ('pending','claimed','dead_letter') \
         UNION ALL \
         SELECT 'summary'::TEXT,thread_id,project_partition_key, \
                CASE WHEN summary_dispatch_dead_letter_version IS NOT NULL THEN 'dead_letter' \
                     WHEN summary_status='running' THEN 'running' ELSE 'pending' END, \
                summary_dispatch_version,summary_dispatch_last_error,updated_at \
           FROM engine_thread_summary_partitions WHERE tenant_id=$1 AND source_id=$2 \
             AND (pending_record_count>0 OR summary_dispatch_dead_letter_version IS NOT NULL) \
         UNION ALL \
         SELECT 'rollup'::TEXT,id,project_partition_key, \
                CASE WHEN rollup_dispatch_dead_letter_version IS NOT NULL THEN 'dead_letter' \
                     ELSE 'pending' END,rollup_dispatch_version,rollup_dispatch_last_error,updated_at \
           FROM engine_summaries WHERE tenant_id=$1 AND source_id=$2 AND recall_eligible \
             AND (rollup_dispatch_pending OR rollup_dispatch_dead_letter_version IS NOT NULL) \
         UNION ALL \
         SELECT 'subject_memory'::TEXT,scope_key,NULL::TEXT, \
                CASE WHEN subject_memory_dispatch_dead_letter_version IS NOT NULL THEN 'dead_letter' \
                     WHEN subject_memory_status='running' THEN 'running' ELSE 'pending' END, \
                subject_memory_dispatch_version,subject_memory_dispatch_last_error,updated_at \
           FROM engine_subject_memory_scopes WHERE tenant_id=$1 AND source_id=$2 \
             AND (subject_memory_dispatch_pending OR subject_memory_dispatch_dead_letter_version IS NOT NULL) \
         UNION ALL \
         SELECT 'rebuild'::TEXT,thread_id || ':' || rebuild_kind,project_partition_key,status, \
                attempt_count,NULL::TEXT,updated_at FROM engine_partition_rebuild_backlog \
          WHERE tenant_id=$1 AND source_id=$2 AND status<>'completed' \
         UNION ALL \
         SELECT 'unprocessable'::TEXT,id,project_partition_key,'retired'::TEXT,0,retired_reason, \
                COALESCE(retired_at,created_at) FROM engine_records \
          WHERE tenant_id=$1 AND source_id=$2 AND NOT recall_eligible \
            AND retired_reason='legacy_partition_untrusted' \
         ) queue ORDER BY CASE status WHEN 'dead_letter' THEN 0 WHEN 'failed' THEN 1 \
                 WHEN 'running' THEN 2 ELSE 3 END,updated_at,item_id LIMIT $3",
    )
    .bind(tenant)
    .bind(source)
    .bind(limit)
    .fetch_all(&state.pool)
    .await
    .map(Json)
    .map_err(internal_error)
}

pub async fn list_operator_actions(
    State(state): State<Arc<AppState>>,
    auth: MemoryAuthContext,
    Query(query): Query<ReliabilityScopeQuery>,
) -> Result<Json<Vec<OperatorActionRow>>, (StatusCode, String)> {
    let (tenant, source, limit) = validated_scope(&auth, &query)?;
    sqlx::query_as(
        "SELECT id,tenant_id,source_id,operator_id,action_kind,object_kind,object_id, \
                project_partition_key,reason,before_state,after_state,created_at \
           FROM engine_reliability_operator_actions WHERE tenant_id=$1 AND source_id=$2 \
          ORDER BY created_at DESC,id DESC LIMIT $3",
    )
    .bind(tenant)
    .bind(source)
    .bind(limit)
    .fetch_all(&state.pool)
    .await
    .map(Json)
    .map_err(internal_error)
}

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
    if before.0 != "failed" {
        return Err((
            StatusCode::CONFLICT,
            "only failed rebuild backlog items can be rearmed".to_string(),
        ));
    }
    sqlx::query(
        "UPDATE engine_partition_rebuild_backlog SET status='pending',attempt_count=0, \
         lease_owner=NULL,lease_token=NULL,lease_expires_at=NULL,last_error=NULL,completed_at=NULL, \
         reason=$6,updated_at=now() WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 \
           AND project_partition_key=$4 AND rebuild_kind=$5 AND status='failed'",
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
    .bind(request.tenant_id.trim()).bind(request.source_id.trim()).bind(request.summary_id.trim())
    .fetch_optional(&mut *tx).await.map_err(internal_error)?
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
    ).await?;
    tx.commit().await.map_err(internal_error)?;
    Ok(Json(json!({"operation_id":action_id,"restored_records":restored})))
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
    ).await.map_err(|error| (StatusCode::CONFLICT, error))?;
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
    ).await?;
    tx.commit().await.map_err(internal_error)?;
    Ok(Json(json!({"operation_id":action_id,"changed":result.changed})))
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
        return Err((StatusCode::BAD_REQUEST, "version must be positive".to_string()));
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
    ).await?;
    tx.commit().await.map_err(internal_error)?;
    Ok(Json(json!({"operation_id":action_id,"rearmed_id":rearmed_id})))
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
    ).bind(&action_id).bind(tenant_id).bind(source_id).bind(auth.operator_audit_id())
        .bind(action_kind).bind(object_kind).bind(object_id).bind(project_partition_key)
        .bind(reason).bind(SqlJson(before_state)).bind(SqlJson(after_state))
        .execute(&mut **tx).await.map_err(internal_error)?;
    Ok(action_id)
}

fn validate_write_reason(object_id: &str, reason: &str) -> Result<(), (StatusCode, String)> {
    if object_id.trim().is_empty() || reason.trim().len() < 8 {
        return Err((StatusCode::BAD_REQUEST,
            "object identity is required and reason must contain at least 8 characters".to_string()));
    }
    Ok(())
}

fn validated_scope<'a>(
    auth: &MemoryAuthContext,
    query: &'a ReliabilityScopeQuery,
) -> Result<(&'a str, &'a str, i64), (StatusCode, String)> {
    let tenant = query.tenant_id.trim();
    let source = query.source_id.trim();
    if tenant.is_empty() || source.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "tenant_id and source_id are required".to_string(),
        ));
    }
    auth.ensure_tenant_scope(tenant)?;
    Ok((tenant, source, query.limit.unwrap_or(200).clamp(1, 1_000)))
}

fn internal_error(error: sqlx::Error) -> (StatusCode, String) {
    tracing::error!(error = %error, "load Memory Engine reliability administration data failed");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "load reliability administration data failed".to_string(),
    )
}
