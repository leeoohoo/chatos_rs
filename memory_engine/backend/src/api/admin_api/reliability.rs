// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[path = "reliability_actions.rs"]
mod actions;

pub use actions::{
    rearm_rebuild_backlog, rearm_reliability_queue, restore_record_pending,
    resummarize_covered_range, retire_summary,
};

use std::{collections::BTreeMap, sync::Arc};

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::types::Json as SqlJson;

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
        true, // The operator dashboard requires the complete audit.
    )
    .await
    .map(Json)
    .map_err(|error| {
        tracing::error!(error = %error, "build Memory Engine reliability dashboard failed");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "build reliability dashboard failed".to_string(),
        )
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
