// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::Serialize;
use serde_json::{json, Value};
use sqlx::types::Json;
use std::collections::HashMap;
use uuid::Uuid;

use crate::db::Db;

#[derive(Debug, Clone, Serialize)]
pub struct HistoryMigrationReport {
    pub migration_run_id: String,
    pub rebuild_run_id: String,
    pub tenant_id: String,
    pub source_id: String,
    pub restored_records: i64,
    pub retired_summaries: i64,
    pub retired_subject_memories: i64,
    pub rebuild_backlog_items: i64,
    pub pending_partitions: i64,
}

/// Quarantines every historical derived output whose coverage, lineage, structured payload, or
/// partition cannot be proven. Source records are restored to the authoritative partition queue;
/// no source or derived row is physically deleted.
pub async fn run_m2a_history_repair(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    operator_id: &str,
    reason: &str,
) -> Result<HistoryMigrationReport, String> {
    for (name, value) in [
        ("tenant_id", tenant_id),
        ("source_id", source_id),
        ("operator_id", operator_id),
        ("reason", reason),
    ] {
        if value.trim().is_empty() {
            return Err(format!("{name} must not be empty"));
        }
    }
    let migration_run_id = Uuid::new_v4().to_string();
    let rebuild_run_id = Uuid::new_v4().to_string();
    let mut tx = db.begin().await.map_err(|error| error.to_string())?;

    // A transaction-scoped lock makes an operator retry deterministic even before the partial
    // unique index reports a competing run.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1),hashtext($2))")
        .bind(tenant_id)
        .bind(source_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    sqlx::query(
        "INSERT INTO engine_reliability_migration_runs \
         (id,migration_kind,tenant_id,source_id,status,cursor,statistics) \
         VALUES($1,'m2a_history_repair',$2,$3,'running',$4,$5)",
    )
    .bind(&migration_run_id)
    .bind(tenant_id)
    .bind(source_id)
    .bind(Json(json!({"phase": "quarantine"})))
    .bind(Json(json!({
        "operator_id": operator_id,
        "reason": reason,
    })))
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("create reliability migration run failed: {error}"))?;
    sqlx::query(
        "INSERT INTO engine_reliability_migration_runs \
         (id,migration_kind,tenant_id,source_id,status,cursor,statistics) \
         VALUES($1,'m3b_partition_rebuild',$2,$3,'running',$4,$5)",
    )
    .bind(&rebuild_run_id)
    .bind(tenant_id)
    .bind(source_id)
    .bind(Json(
        json!({"phase": "waiting_thread_l0", "completed_items": 0}),
    ))
    .bind(Json(json!({
        "m2a_migration_run_id": migration_run_id,
        "operator_id": operator_id,
        "reason": reason,
    })))
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("create partition rebuild migration run failed: {error}"))?;

    // Preserve an exact before/after audit row before mutating any untrusted derived output.
    sqlx::query(
        "INSERT INTO engine_reliability_migration_actions \
         (migration_run_id,tenant_id,source_id,action_kind,object_kind,object_id, \
          project_partition_key,before_state,after_state) \
         SELECT $1,s.tenant_id,s.source_id,'retire_summary','thread_summary',s.id, \
                COALESCE(s.project_partition_key,'__none__'), \
                jsonb_build_object('status',s.status,'recall_eligible',s.recall_eligible, \
                    'retired_reason',s.retired_reason,'generation_id',s.generation_id, \
                    'payload_schema',s.payload_schema), \
                jsonb_build_object('status','retired','recall_eligible',false, \
                    'retired_reason',CASE \
                      WHEN s.summary_type='thread_repair' THEN 'legacy_repair_untrusted' \
                      WHEN s.level>0 THEN 'legacy_rollup_untrusted' \
                      ELSE 'legacy_summary_untrusted' END) \
           FROM engine_summaries s \
          WHERE s.tenant_id=$2 AND s.source_id=$3 AND s.recall_eligible \
            AND (s.generation_id IS NULL OR s.structured_payload IS NULL \
                 OR s.project_partition_key IS NULL \
                 OR NOT EXISTS (SELECT 1 FROM engine_memory_derivation_edges edge \
                       WHERE edge.tenant_id=s.tenant_id AND edge.source_id=s.source_id \
                         AND edge.output_kind='thread_summary' AND edge.output_id=s.id \
                         AND edge.status='active') AND s.level>0) \
         ON CONFLICT DO NOTHING",
    )
    .bind(&migration_run_id)
    .bind(tenant_id)
    .bind(source_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("audit untrusted summaries failed: {error}"))?;

    sqlx::query(
        "INSERT INTO engine_reliability_migration_actions \
         (migration_run_id,tenant_id,source_id,action_kind,object_kind,object_id, \
          project_partition_key,before_state,after_state) \
         SELECT $1,m.tenant_id,m.source_id,'retire_subject_memory','subject_memory',m.id, \
                COALESCE(m.project_partition_key,'__none__'), \
                jsonb_build_object('status',m.status,'recall_eligible',m.recall_eligible, \
                    'retired_reason',m.retired_reason,'generation_id',m.generation_id, \
                    'payload_schema',m.payload_schema), \
                jsonb_build_object('status','retired','recall_eligible',false, \
                    'retired_reason','legacy_memory_lineage_untrusted') \
           FROM engine_subject_memories m \
          WHERE m.tenant_id=$2 AND m.source_id=$3 AND m.recall_eligible \
            AND (m.generation_id IS NULL OR m.structured_payload IS NULL \
                 OR m.project_partition_key IS NULL \
                 OR NOT EXISTS (SELECT 1 FROM engine_memory_derivation_edges edge \
                       WHERE edge.tenant_id=m.tenant_id AND edge.source_id=m.source_id \
                         AND edge.output_kind='subject_memory' AND edge.output_id=m.id \
                         AND edge.status='active')) \
         ON CONFLICT DO NOTHING",
    )
    .bind(&migration_run_id)
    .bind(tenant_id)
    .bind(source_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("audit untrusted subject memories failed: {error}"))?;

    // An explicit summary_id is recorded as a candidate for operator evidence, but it is not
    // promoted to active coverage unless the generated output already satisfies the new contract.
    sqlx::query(
        "INSERT INTO engine_coverage_migration_candidates \
         (migration_run_id,tenant_id,source_id,summary_id,record_id,inference_kind, \
          validation_status,evidence,validated_at) \
         SELECT $1,r.tenant_id,r.source_id,r.summary_id,r.id,'explicit_summary_id', \
                CASE WHEN s.generation_id IS NOT NULL AND s.structured_payload IS NOT NULL \
                          AND s.project_partition_key IS NOT DISTINCT FROM r.project_partition_key \
                     THEN 'confirmed' ELSE 'rejected' END, \
                jsonb_build_object('record_summary_status',r.summary_status, \
                    'summary_generation_id',s.generation_id, \
                    'partition_matches',s.project_partition_key IS NOT DISTINCT FROM r.project_partition_key), \
                now() \
           FROM engine_records r \
           JOIN engine_summaries s ON s.tenant_id=r.tenant_id AND s.source_id=r.source_id \
                                  AND s.id=r.summary_id AND s.thread_id=r.thread_id \
          WHERE r.tenant_id=$2 AND r.source_id=$3 AND r.summary_status='summarized' \
            AND r.summary_id IS NOT NULL AND r.summary_id<>'skipped_single_record_token_limit' \
         ON CONFLICT DO NOTHING",
    )
    .bind(&migration_run_id)
    .bind(tenant_id)
    .bind(source_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("record coverage migration candidates failed: {error}"))?;

    sqlx::query(
        "INSERT INTO engine_reliability_migration_actions \
         (migration_run_id,tenant_id,source_id,action_kind,object_kind,object_id, \
          project_partition_key,before_state,after_state) \
         SELECT $1,r.tenant_id,r.source_id,'restore_record_pending','record',r.id, \
                COALESCE(r.project_partition_key,'__none__'), \
                jsonb_build_object('summary_status',r.summary_status,'summary_id',r.summary_id), \
                jsonb_build_object('summary_status','pending','summary_id',NULL) \
           FROM engine_records r \
          WHERE r.tenant_id=$2 AND r.source_id=$3 AND r.recall_eligible \
            AND r.project_partition_key IS NOT NULL \
            AND (r.summary_id='skipped_single_record_token_limit' \
                 OR (r.summary_status='summarized' AND NOT EXISTS ( \
                     SELECT 1 FROM engine_summary_record_coverage coverage \
                      WHERE coverage.tenant_id=r.tenant_id AND coverage.source_id=r.source_id \
                        AND coverage.record_id=r.id AND coverage.status='active'))) \
         ON CONFLICT DO NOTHING",
    )
    .bind(&migration_run_id)
    .bind(tenant_id)
    .bind(source_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("audit restored source records failed: {error}"))?;
    sqlx::query(
        "INSERT INTO engine_reliability_migration_actions \
         (migration_run_id,tenant_id,source_id,action_kind,object_kind,object_id, \
          project_partition_key,before_state,after_state) \
         SELECT $1,r.tenant_id,r.source_id,'retire_record_untrusted_partition','record',r.id, \
                '__none__', \
                jsonb_build_object('summary_status',r.summary_status,'summary_id',r.summary_id, \
                    'recall_eligible',r.recall_eligible,'project_partition_key',NULL), \
                jsonb_build_object('summary_status','retired','recall_eligible',false, \
                    'retired_reason','legacy_partition_untrusted') \
           FROM engine_records r \
          WHERE r.tenant_id=$2 AND r.source_id=$3 AND r.recall_eligible \
            AND r.project_partition_key IS NULL \
            AND (r.summary_id='skipped_single_record_token_limit' \
                 OR r.summary_status IN ('pending','summarizing','summarized')) \
         ON CONFLICT DO NOTHING",
    )
    .bind(&migration_run_id)
    .bind(tenant_id)
    .bind(source_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("audit unpartitioned source records failed: {error}"))?;

    sqlx::query(
        "UPDATE engine_summaries s SET status='retired',recall_eligible=false,retired_at=now(), \
                retired_reason=a.after_state->>'retired_reason',updated_at=now(), \
                rollup_dispatch_pending=false,subject_memory_source_dispatch_pending=false \
           FROM engine_reliability_migration_actions a \
          WHERE a.migration_run_id=$1 AND a.action_kind='retire_summary' \
            AND s.tenant_id=a.tenant_id AND s.source_id=a.source_id AND s.id=a.object_id",
    )
    .bind(&migration_run_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("retire untrusted summaries failed: {error}"))?;
    sqlx::query(
        "UPDATE engine_subject_memories m SET status='retired',recall_eligible=false, \
                retired_at=now(),retired_reason='legacy_memory_lineage_untrusted', \
                updated_at=now(),rollup_status='retired' \
           FROM engine_reliability_migration_actions a \
          WHERE a.migration_run_id=$1 AND a.action_kind='retire_subject_memory' \
            AND m.tenant_id=a.tenant_id AND m.source_id=a.source_id AND m.id=a.object_id",
    )
    .bind(&migration_run_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("retire untrusted subject memories failed: {error}"))?;
    sqlx::query(
        "UPDATE engine_records r SET summary_status='pending',summary_id=NULL, \
                summary_job_run_id=NULL,summary_started_at=NULL \
           FROM engine_reliability_migration_actions a \
          WHERE a.migration_run_id=$1 AND a.action_kind='restore_record_pending' \
            AND r.tenant_id=a.tenant_id AND r.source_id=a.source_id AND r.id=a.object_id",
    )
    .bind(&migration_run_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("restore source records pending failed: {error}"))?;
    sqlx::query(
        "UPDATE engine_records r SET summary_status='retired',recall_eligible=false,retired_at=now(), \
                retired_reason='legacy_partition_untrusted',summary_id=NULL,summary_job_run_id=NULL, \
                summary_started_at=NULL \
           FROM engine_reliability_migration_actions a \
          WHERE a.migration_run_id=$1 AND a.action_kind='retire_record_untrusted_partition' \
            AND r.tenant_id=a.tenant_id AND r.source_id=a.source_id AND r.id=a.object_id",
    )
    .bind(&migration_run_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("retire unpartitioned source records failed: {error}"))?;

    sqlx::query(
        "INSERT INTO engine_thread_summary_partitions \
         (tenant_id,source_id,thread_id,project_partition_key,summary_status, \
          pending_record_count,pending_summary_tokens,summary_dispatch_pending, \
          summary_dispatch_version,summary_dispatch_force_version, \
          summary_dispatch_requested_at,created_at,updated_at) \
         SELECT r.tenant_id,r.source_id,r.thread_id,r.project_partition_key, \
                CASE WHEN COUNT(*) FILTER (WHERE r.summary_status='pending')>0 \
                     THEN 'pending' ELSE 'idle' END, \
                COUNT(*) FILTER (WHERE r.summary_status='pending'), \
                0, \
                COUNT(*) FILTER (WHERE r.summary_status='pending')>0, \
                CASE WHEN COUNT(*) FILTER (WHERE r.summary_status='pending')>0 THEN 1 ELSE 0 END, \
                CASE WHEN COUNT(*) FILTER (WHERE r.summary_status='pending')>0 THEN 1 ELSE 0 END, \
                CASE WHEN COUNT(*) FILTER (WHERE r.summary_status='pending')>0 THEN now() ELSE NULL END, \
                now(),now() \
           FROM engine_records r \
          WHERE r.tenant_id=$1 AND r.source_id=$2 AND r.recall_eligible \
          GROUP BY r.tenant_id,r.source_id,r.thread_id,r.project_partition_key \
         ON CONFLICT (tenant_id,source_id,thread_id,project_partition_key) DO UPDATE SET \
           summary_status=EXCLUDED.summary_status, \
           pending_record_count=EXCLUDED.pending_record_count, \
           pending_summary_tokens=EXCLUDED.pending_summary_tokens, \
           summary_dispatch_pending=EXCLUDED.summary_dispatch_pending, \
           summary_dispatch_version=CASE WHEN EXCLUDED.summary_dispatch_pending \
               THEN engine_thread_summary_partitions.summary_dispatch_version+1 \
               ELSE engine_thread_summary_partitions.summary_dispatch_version END, \
           summary_dispatch_force_version=CASE WHEN EXCLUDED.summary_dispatch_pending \
               THEN engine_thread_summary_partitions.summary_dispatch_version+1 \
               ELSE engine_thread_summary_partitions.summary_dispatch_force_version END, \
           summary_dispatch_requested_at=EXCLUDED.summary_dispatch_requested_at,updated_at=now()",
    )
    .bind(tenant_id)
    .bind(source_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("rebuild partition counters failed: {error}"))?;

    let pending_rows = sqlx::query_as::<_, (String, String, Json<Value>)>(
        "SELECT thread_id,project_partition_key,data FROM engine_records \
          WHERE tenant_id=$1 AND source_id=$2 AND recall_eligible \
            AND summary_status='pending' AND project_partition_key IS NOT NULL",
    )
    .bind(tenant_id)
    .bind(source_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|error| format!("load restored records for token accounting failed: {error}"))?;
    let mut partition_tokens = HashMap::<(String, String), i64>::new();
    for (thread_id, project_partition_key, data) in pending_rows {
        let record = crate::repositories::postgres::decode(data)
            .map_err(|error| format!("decode restored record failed: {error}"))?;
        let tokens = crate::repositories::records::estimate_pending_record_tokens(&record);
        *partition_tokens
            .entry((thread_id, project_partition_key))
            .or_default() += tokens;
    }
    for ((thread_id, project_partition_key), tokens) in partition_tokens {
        sqlx::query(
            "UPDATE engine_thread_summary_partitions SET pending_summary_tokens=$5,updated_at=now() \
              WHERE tenant_id=$1 AND source_id=$2 AND thread_id=$3 AND project_partition_key=$4",
        )
        .bind(tenant_id)
        .bind(source_id)
        .bind(thread_id)
        .bind(project_partition_key)
        .bind(tokens.max(0))
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("update restored partition token accounting failed: {error}"))?;
    }

    let partition_dispatches = sqlx::query_as::<_, (String, String, i64)>(
        "SELECT thread_id,project_partition_key,summary_dispatch_version \
           FROM engine_thread_summary_partitions WHERE tenant_id=$1 AND source_id=$2 \
            AND pending_record_count>0",
    )
    .bind(tenant_id)
    .bind(source_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|error| format!("load restored partition dispatches failed: {error}"))?;
    for (thread_id, project_partition_key, dispatch_version) in &partition_dispatches {
        crate::repositories::derived_events::enqueue_thread_summary_request(
            &mut tx,
            tenant_id,
            source_id,
            thread_id,
            project_partition_key,
            *dispatch_version,
        )
        .await
        .map_err(|error| format!("enqueue restored partition dispatch failed: {error}"))?;
    }

    sqlx::query(
        "INSERT INTO engine_partition_rebuild_backlog \
         (tenant_id,source_id,thread_id,project_partition_key,rebuild_kind,status, \
          migration_run_id,reason,created_at,updated_at) \
         SELECT p.tenant_id,p.source_id,p.thread_id,p.project_partition_key,kind,'pending',$1,$4,now(),now() \
           FROM engine_thread_summary_partitions p \
          CROSS JOIN (VALUES ('thread_l0'),('thread_rollup'),('project_memory'),('agent_memory')) kinds(kind) \
          WHERE p.tenant_id=$2 AND p.source_id=$3 AND p.pending_record_count>0 \
         ON CONFLICT (tenant_id,source_id,thread_id,project_partition_key,rebuild_kind) DO UPDATE SET \
           status='pending',migration_run_id=EXCLUDED.migration_run_id,reason=EXCLUDED.reason, \
           lease_owner=NULL,lease_token=NULL,lease_expires_at=NULL,last_error=NULL,updated_at=now(), \
           completed_at=NULL",
    )
    .bind(&rebuild_run_id)
    .bind(tenant_id)
    .bind(source_id)
    .bind(reason)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("enqueue partition rebuild backlog failed: {error}"))?;

    sqlx::query(
        "INSERT INTO engine_reliability_migration_actions \
         (migration_run_id,tenant_id,source_id,action_kind,object_kind,object_id, \
          project_partition_key,before_state,after_state) \
         SELECT $1,b.tenant_id,b.source_id,'enqueue_partition_rebuild','thread_partition', \
                b.thread_id || ':' || b.project_partition_key || ':' || b.rebuild_kind, \
                b.project_partition_key,'{}'::jsonb, \
                jsonb_build_object('status',b.status,'rebuild_kind',b.rebuild_kind,'reason',b.reason) \
           FROM engine_partition_rebuild_backlog b \
          WHERE b.migration_run_id=$2 \
         ON CONFLICT DO NOTHING",
    )
    .bind(&migration_run_id)
    .bind(&rebuild_run_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("audit partition rebuild backlog failed: {error}"))?;

    let counts = sqlx::query_as::<_, (i64, i64, i64, i64, i64)>(
        "SELECT \
          COUNT(*) FILTER (WHERE action_kind='restore_record_pending'), \
          COUNT(*) FILTER (WHERE action_kind='retire_summary'), \
          COUNT(*) FILTER (WHERE action_kind='retire_subject_memory'), \
          COUNT(*) FILTER (WHERE action_kind='enqueue_partition_rebuild'), \
          (SELECT COUNT(*) FROM engine_thread_summary_partitions p \
            WHERE p.tenant_id=$2 AND p.source_id=$3 AND p.pending_record_count>0) \
         FROM engine_reliability_migration_actions WHERE migration_run_id=$1",
    )
    .bind(&migration_run_id)
    .bind(tenant_id)
    .bind(source_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|error| format!("load reliability migration statistics failed: {error}"))?;
    let report = HistoryMigrationReport {
        migration_run_id: migration_run_id.clone(),
        rebuild_run_id: rebuild_run_id.clone(),
        tenant_id: tenant_id.to_string(),
        source_id: source_id.to_string(),
        restored_records: counts.0,
        retired_summaries: counts.1,
        retired_subject_memories: counts.2,
        rebuild_backlog_items: counts.3,
        pending_partitions: counts.4,
    };
    let statistics: Value = serde_json::to_value(&report).map_err(|error| error.to_string())?;
    sqlx::query(
        "UPDATE engine_reliability_migration_runs SET status='completed',cursor=$2,statistics=$3, \
                finished_at=now() WHERE id=$1 AND status='running'",
    )
    .bind(&migration_run_id)
    .bind(Json(json!({"phase": "m2a_complete"})))
    .bind(Json(statistics))
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("complete reliability migration run failed: {error}"))?;
    if report.rebuild_backlog_items == 0 {
        sqlx::query(
            "UPDATE engine_reliability_migration_runs SET status='completed', \
             cursor=$2,statistics=statistics || $3,finished_at=now() WHERE id=$1 AND status='running'",
        )
        .bind(&rebuild_run_id)
        .bind(Json(json!({"phase": "m3b_complete", "completed_items": 0})))
        .bind(Json(json!({"backlog_items": 0})))
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("complete empty partition rebuild run failed: {error}"))?;
    }
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(report)
}
