// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use serde_json::{json, Value};
use sqlx::Row;
use tracing::{info, warn};

use crate::repositories::{
    derived_events, partition_rebuilds, subject_memory_scopes, summaries, thread_partitions,
    threads,
};
use crate::services::{control_plane as cp_service, subject_memory, summary};
use crate::state::AppState;

const REBUILD_LEASE_SECONDS: i64 = 300;

pub fn start(state: Arc<AppState>, service_id: String) {
    tokio::spawn(run(state, service_id));
}

async fn run(state: Arc<AppState>, service_id: String) {
    let mut interval = tokio::time::interval(state.config.summary_outbox_reconcile_interval);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        match process_one(&state, service_id.as_str()).await {
            Ok(true) => info!("Memory Engine reconciled one partition rebuild backlog item"),
            Ok(false) => {}
            Err(error) => warn!(error, "Memory Engine partition rebuild reconciler failed"),
        }
    }
}

async fn process_one(state: &AppState, service_id: &str) -> Result<bool, String> {
    let Some(claim) =
        partition_rebuilds::claim_next(&state.pool, service_id, REBUILD_LEASE_SECONDS).await?
    else {
        return Ok(false);
    };
    let outcome = match claim.rebuild_kind.as_str() {
        "thread_l0" => reconcile_thread_l0(state, &claim).await,
        "thread_rollup" => reconcile_thread_rollup(state, &claim).await,
        "project_memory" => reconcile_subject_memory(state, &claim, true).await,
        "agent_memory" => reconcile_subject_memory(state, &claim, false).await,
        _ => Err("unsupported partition rebuild kind".to_string()),
    };
    match outcome {
        Ok(RebuildOutcome::Completed(cursor)) => {
            partition_rebuilds::record_completed(&state.pool, &claim, cursor).await?;
        }
        Ok(RebuildOutcome::Waiting(cursor)) => {
            partition_rebuilds::record_waiting(&state.pool, &claim, cursor).await?;
        }
        Err(error) => {
            let code = rebuild_error_code(error.as_str());
            partition_rebuilds::record_failed(&state.pool, &claim, code).await?;
            warn!(
                thread_id = claim.thread_id,
                project_partition_key = claim.project_partition_key,
                rebuild_kind = claim.rebuild_kind,
                error,
                "Memory Engine partition rebuild item failed"
            );
        }
    }
    Ok(true)
}

enum RebuildOutcome {
    Waiting(Value),
    Completed(Value),
}

async fn reconcile_thread_l0(
    state: &AppState,
    claim: &partition_rebuilds::PartitionRebuildClaim,
) -> Result<RebuildOutcome, String> {
    let row = sqlx::query(
        "SELECT p.pending_record_count,p.pending_summary_tokens,p.summary_status, \
                p.summary_dispatch_version,p.summary_dispatch_published_version, \
                p.summary_dispatch_consumed_version,p.summary_dispatch_dead_letter_version, \
                (SELECT COUNT(*) FROM engine_records r \
                  WHERE r.tenant_id=p.tenant_id AND r.source_id=p.source_id \
                    AND r.thread_id=p.thread_id AND r.project_partition_key=p.project_partition_key \
                    AND r.recall_eligible AND NOT EXISTS ( \
                        SELECT 1 FROM engine_summary_record_coverage coverage \
                         WHERE coverage.tenant_id=r.tenant_id AND coverage.source_id=r.source_id \
                           AND coverage.record_id=r.id AND coverage.status='active')) AS uncovered_count \
           FROM engine_thread_summary_partitions p \
          WHERE p.tenant_id=$1 AND p.source_id=$2 AND p.thread_id=$3 \
            AND p.project_partition_key=$4",
    )
    .bind(&claim.tenant_id)
    .bind(&claim.source_id)
    .bind(&claim.thread_id)
    .bind(&claim.project_partition_key)
    .fetch_optional(&state.pool)
    .await
    .map_err(|error| error.to_string())?
    .ok_or_else(|| "partition rebuild source partition is missing".to_string())?;
    let pending_count: i64 = row
        .try_get("pending_record_count")
        .map_err(|e| e.to_string())?;
    let pending_tokens: i64 = row
        .try_get("pending_summary_tokens")
        .map_err(|e| e.to_string())?;
    let summary_status: String = row.try_get("summary_status").map_err(|e| e.to_string())?;
    let version: i64 = row
        .try_get("summary_dispatch_version")
        .map_err(|e| e.to_string())?;
    let published: i64 = row
        .try_get("summary_dispatch_published_version")
        .map_err(|e| e.to_string())?;
    let consumed: i64 = row
        .try_get("summary_dispatch_consumed_version")
        .map_err(|e| e.to_string())?;
    let dead_letter: Option<i64> = row
        .try_get("summary_dispatch_dead_letter_version")
        .map_err(|e| e.to_string())?;
    let uncovered: i64 = row.try_get("uncovered_count").map_err(|e| e.to_string())?;
    let cursor = json!({
        "phase": if uncovered == 0 { "thread_l0_complete" } else { "thread_l0_waiting" },
        "uncovered_records": uncovered,
        "pending_records": pending_count,
        "pending_tokens": pending_tokens,
        "summary_status": summary_status,
        "dispatch_version": version,
        "published_version": published,
        "consumed_version": consumed,
    });
    if uncovered == 0 {
        return Ok(RebuildOutcome::Completed(cursor));
    }
    if dead_letter == Some(version) {
        return Err("thread_l0_dispatch_dead_lettered".to_string());
    }
    if version <= consumed && summary_status != "running" {
        arm_forced_l0_dispatch(state, claim).await?;
    }
    Ok(RebuildOutcome::Waiting(cursor))
}

async fn arm_forced_l0_dispatch(
    state: &AppState,
    claim: &partition_rebuilds::PartitionRebuildClaim,
) -> Result<(), String> {
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|error| error.to_string())?;
    let version = thread_partitions::apply_ingest_delta(
        &mut tx,
        &claim.tenant_id,
        &claim.source_id,
        &claim.thread_id,
        &claim.project_partition_key,
        0,
        0,
        true,
    )
    .await?;
    if let Some(version) = version {
        derived_events::enqueue_thread_summary_request(
            &mut tx,
            &claim.tenant_id,
            &claim.source_id,
            &claim.thread_id,
            &claim.project_partition_key,
            version,
        )
        .await?;
    }
    tx.commit().await.map_err(|error| error.to_string())
}

async fn reconcile_thread_rollup(
    state: &AppState,
    claim: &partition_rebuilds::PartitionRebuildClaim,
) -> Result<RebuildOutcome, String> {
    let settings = cp_service::load_rollup_settings(&state.config)?;
    let prepared = summary::prepare_thread_rollup(
        &state.pool,
        &claim.tenant_id,
        &claim.source_id,
        &claim.thread_id,
        &settings,
    )
    .await?;
    let Some(prepared) = prepared else {
        return Ok(RebuildOutcome::Completed(json!({
            "phase": "thread_rollup_complete",
        })));
    };
    summaries::rearm_rollup_dispatch_if_eligible(
        &state.pool,
        &claim.tenant_id,
        &claim.source_id,
        &claim.thread_id,
    )
    .await?;
    Ok(RebuildOutcome::Waiting(json!({
        "phase": "thread_rollup_waiting",
        "selected_partition": prepared.project_partition_key,
        "source_level": prepared.level,
        "selected_count": prepared.selected.len(),
    })))
}

async fn reconcile_subject_memory(
    state: &AppState,
    claim: &partition_rebuilds::PartitionRebuildClaim,
    project_scope: bool,
) -> Result<RebuildOutcome, String> {
    let thread = threads::get_thread_by_id(
        &state.pool,
        &claim.tenant_id,
        &claim.source_id,
        &claim.thread_id,
    )
    .await?
    .ok_or_else(|| "subject_memory_rebuild_thread_missing".to_string())?;
    let (scope_key, derivation_kind) = if project_scope {
        if !claim.project_partition_key.starts_with("project:") {
            return Ok(RebuildOutcome::Completed(json!({
                "phase": "project_memory_not_applicable",
            })));
        }
        subject_memory_scopes::ensure_project_identity_scope(
            &state.pool,
            &claim.tenant_id,
            &claim.source_id,
            &claim.project_partition_key,
        )
        .await?;
        (
            format!("identity:{}", claim.project_partition_key),
            "project_memory",
        )
    } else {
        subject_memory_scopes::ensure_agent_identity_scope(
            &state.pool,
            &claim.tenant_id,
            &claim.source_id,
            &thread.subject_id,
        )
        .await?;
        (format!("identity:{}", thread.subject_id), "agent_memory")
    };
    restore_missing_scope_lineage(state, claim, scope_key.as_str(), derivation_kind).await?;
    let scope = subject_memory_scopes::get_subject_memory_scope(
        &state.pool,
        &claim.tenant_id,
        &claim.source_id,
        scope_key.as_str(),
    )
    .await?
    .ok_or_else(|| "subject_memory_rebuild_scope_missing".to_string())?;
    if subject_memory::scope_has_pending_work(&state.config, &state.pool, &scope).await? {
        subject_memory_scopes::rearm_subject_memory_dispatch(
            &state.pool,
            &claim.tenant_id,
            &claim.source_id,
            scope_key.as_str(),
        )
        .await?;
        return Ok(RebuildOutcome::Waiting(json!({
            "phase": "subject_memory_waiting",
            "scope_key": scope_key,
            "memory_type": scope.memory_type,
        })));
    }
    Ok(RebuildOutcome::Completed(json!({
        "phase": "subject_memory_complete",
        "scope_key": scope_key,
        "memory_type": scope.memory_type,
    })))
}

async fn restore_missing_scope_lineage(
    state: &AppState,
    claim: &partition_rebuilds::PartitionRebuildClaim,
    scope_key: &str,
    derivation_kind: &str,
) -> Result<(), String> {
    sqlx::query(
        "UPDATE engine_summaries summary SET \
             subject_memory_scope_keys=array_remove(summary.subject_memory_scope_keys,$5), \
             subject_memory_summarized=CASE WHEN cardinality(array_remove(summary.subject_memory_scope_keys,$5))=0 \
                 THEN 0 ELSE summary.subject_memory_summarized END,updated_at=now() \
          WHERE summary.tenant_id=$1 AND summary.source_id=$2 AND summary.thread_id=$3 \
            AND summary.project_partition_key=$4 AND summary.recall_eligible AND summary.status='done' \
            AND $5=ANY(summary.subject_memory_scope_keys) AND NOT EXISTS ( \
                SELECT 1 FROM engine_memory_derivation_edges edge \
                JOIN engine_subject_memories memory \
                  ON memory.tenant_id=edge.tenant_id AND memory.source_id=edge.source_id \
                 AND memory.id=edge.output_id AND memory.status='active' AND memory.recall_eligible \
                 WHERE edge.tenant_id=summary.tenant_id AND edge.source_id=summary.source_id \
                   AND edge.input_kind='thread_summary' AND edge.input_id=summary.id \
                   AND edge.output_kind='subject_memory' AND edge.derivation_kind=$6 \
                   AND edge.status='active')",
    )
    .bind(&claim.tenant_id)
    .bind(&claim.source_id)
    .bind(&claim.thread_id)
    .bind(&claim.project_partition_key)
    .bind(scope_key)
    .bind(derivation_kind)
    .execute(&state.pool)
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn rebuild_error_code(error: &str) -> &'static str {
    match error {
        "thread_l0_dispatch_dead_lettered" => "thread_l0_dispatch_dead_lettered",
        "thread_rollup_policy_disabled" => "thread_rollup_policy_disabled",
        "subject_memory_rebuild_thread_missing" => "subject_memory_rebuild_thread_missing",
        "subject_memory_rebuild_scope_missing" => "subject_memory_rebuild_scope_missing",
        _ => "partition_rebuild_error",
    }
}
