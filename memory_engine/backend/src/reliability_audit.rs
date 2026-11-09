// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::types::Json;

const READINESS_INVENTORY: &str = include_str!("../audit/m0_runtime_readiness.json");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReliabilityAuditScope {
    pub tenant_id: Option<String>,
    pub source_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ReliabilityAuditFinding {
    pub finding_kind: String,
    pub tenant_id: String,
    pub source_id: String,
    pub thread_id: Option<String>,
    pub project_partition_key: String,
    pub object_kind: String,
    pub object_id: String,
    pub related_object_ids: Vec<String>,
    pub detail: Json<Value>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ReliabilityAuditBaseline {
    pub pending_threads: i64,
    pub pending_records: i64,
    pub pending_tokens: i64,
    pub summary_dispatch_pending: i64,
    pub summary_dispatch_dead_lettered: i64,
    pub rollup_inputs_pending: i64,
    pub rollup_dispatch_pending: i64,
    pub subject_memory_source_dispatch_pending: i64,
    pub subject_memory_rollup_pending: i64,
    pub running_jobs: i64,
    pub failed_jobs_last_24h: i64,
    pub outputs_with_provider_overflow_retry: i64,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct SourceContractReadiness {
    pub tenant_id: String,
    pub source_id: String,
    pub source_type: String,
    pub total_records: i64,
    pub ready_records: i64,
    pub readiness_state: String,
    pub missing_conversation_turn_id: i64,
    pub missing_tool_exchange_contract: i64,
    pub missing_project_context_assertion_id: i64,
    pub missing_content_identity_digest: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeReadinessInventory {
    pub schema_version: String,
    pub reviewed_at: String,
    pub components: Vec<RuntimeReadinessComponent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeReadinessComponent {
    pub component: String,
    pub state: String,
    pub source_unit_ready: bool,
    pub runtime_assertion_ready: bool,
    pub model_capability_ready: bool,
    pub present: Vec<String>,
    pub missing: Vec<String>,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReliabilityAuditReport {
    pub schema_version: &'static str,
    pub generated_at: String,
    pub scope: ReliabilityAuditScope,
    pub finding_count: usize,
    pub findings: Vec<ReliabilityAuditFinding>,
    pub baseline: ReliabilityAuditBaseline,
    pub source_contract_readiness: Vec<SourceContractReadiness>,
    pub runtime_readiness: RuntimeReadinessInventory,
    pub unavailable_baselines: Vec<&'static str>,
}

pub fn runtime_readiness_inventory() -> Result<RuntimeReadinessInventory, String> {
    serde_json::from_str(READINESS_INVENTORY)
        .map_err(|error| format!("decode M0 runtime readiness inventory failed: {error}"))
}

pub async fn run_reliability_audit(
    db: &chatos_postgres::PgPool,
    scope: ReliabilityAuditScope,
) -> Result<ReliabilityAuditReport, String> {
    let findings = load_findings(db, &scope).await?;
    let baseline = load_baseline(db, &scope).await?;
    let source_contract_readiness = load_source_contract_readiness(db, &scope).await?;
    Ok(ReliabilityAuditReport {
        schema_version: "memory_reliability_audit_v1",
        generated_at: chrono::Utc::now().to_rfc3339(),
        scope,
        finding_count: findings.len(),
        findings,
        baseline,
        source_contract_readiness,
        runtime_readiness: runtime_readiness_inventory()?,
        unavailable_baselines: vec![
            "pre-M0 compose token history is not persisted; live totals and max are exposed by /metrics",
            "generation replay identity does not exist before M1b",
        ],
    })
}

async fn load_findings(
    db: &chatos_postgres::PgPool,
    scope: &ReliabilityAuditScope,
) -> Result<Vec<ReliabilityAuditFinding>, String> {
    sqlx::query_as::<_, ReliabilityAuditFinding>(FINDINGS_SQL)
        .bind(scope.tenant_id.as_deref())
        .bind(scope.source_id.as_deref())
        .fetch_all(db)
        .await
        .map_err(|error| format!("load Memory Engine reliability findings failed: {error}"))
}

async fn load_baseline(
    db: &chatos_postgres::PgPool,
    scope: &ReliabilityAuditScope,
) -> Result<ReliabilityAuditBaseline, String> {
    sqlx::query_as::<_, ReliabilityAuditBaseline>(BASELINE_SQL)
        .bind(scope.tenant_id.as_deref())
        .bind(scope.source_id.as_deref())
        .fetch_one(db)
        .await
        .map_err(|error| format!("load Memory Engine reliability baseline failed: {error}"))
}

async fn load_source_contract_readiness(
    db: &chatos_postgres::PgPool,
    scope: &ReliabilityAuditScope,
) -> Result<Vec<SourceContractReadiness>, String> {
    sqlx::query_as::<_, SourceContractReadiness>(SOURCE_READINESS_SQL)
        .bind(scope.tenant_id.as_deref())
        .bind(scope.source_id.as_deref())
        .fetch_all(db)
        .await
        .map_err(|error| format!("load Memory Engine source readiness failed: {error}"))
}

const FINDINGS_SQL: &str = r#"
WITH record_rows AS (
    SELECT r.*,
           CASE
             WHEN NULLIF(BTRIM(r.data #>> '{metadata,memory_project_id}'),'') IS NULL THEN '__none__'
             WHEN BTRIM(r.data #>> '{metadata,memory_project_id}') LIKE 'project:%'
               THEN BTRIM(r.data #>> '{metadata,memory_project_id}')
             ELSE 'project:' || BTRIM(r.data #>> '{metadata,memory_project_id}')
           END AS partition_key
      FROM engine_records r
     WHERE ($1::TEXT IS NULL OR r.tenant_id=$1)
       AND ($2::TEXT IS NULL OR r.source_id=$2)
), summary_rows AS (
    SELECT s.*,
           COALESCE(
             NULLIF(BTRIM(s.data #>> '{metadata,project_subject_id}'),''),
             NULLIF(BTRIM(s.data #>> '{metadata,project_id}'),''),
             NULLIF(BTRIM(s.data #>> '{metadata,projectId}'),''),
             '__none__'
           ) AS partition_key
      FROM engine_summaries s
     WHERE ($1::TEXT IS NULL OR s.tenant_id=$1)
       AND ($2::TEXT IS NULL OR s.source_id=$2)
), findings AS (
    SELECT 'legacy_thread_repair'::TEXT AS finding_kind,
           s.tenant_id,s.source_id,s.thread_id,s.partition_key AS project_partition_key,
           'thread_summary'::TEXT AS object_kind,s.id AS object_id,
           ARRAY[]::TEXT[] AS related_object_ids,
           jsonb_build_object('summary_type',s.summary_type,'status',s.status,'level',s.level) AS detail
      FROM summary_rows s WHERE s.summary_type='thread_repair'
    UNION ALL
    SELECT 'oversized_record_sentinel',r.tenant_id,r.source_id,r.thread_id,r.partition_key,
           'record',r.id,ARRAY[]::TEXT[],
           jsonb_build_object('summary_status',r.summary_status,'summary_id',r.summary_id)
      FROM record_rows r WHERE r.summary_id='skipped_single_record_token_limit'
    UNION ALL
    SELECT 'unreachable_summarized_record',r.tenant_id,r.source_id,r.thread_id,r.partition_key,
           'record',r.id,ARRAY_REMOVE(ARRAY[r.summary_id],NULL),
           jsonb_build_object('summary_status',r.summary_status,'summary_id',r.summary_id)
      FROM record_rows r
 LEFT JOIN summary_rows s ON s.tenant_id=r.tenant_id AND s.source_id=r.source_id
                         AND s.thread_id=r.thread_id AND s.id=r.summary_id
     WHERE r.summary_status='summarized'
       AND (r.summary_id IS NULL OR r.summary_id='skipped_single_record_token_limit' OR s.id IS NULL)
    UNION ALL
    SELECT 'high_level_summary_oversized_skip',s.tenant_id,s.source_id,s.thread_id,s.partition_key,
           'thread_summary',s.id,ARRAY[]::TEXT[],
           jsonb_build_object('level',s.level,'source_digest',s.source_digest)
      FROM summary_rows s
     WHERE s.level>0 AND (
           s.data->>'summary_text' LIKE '%oversized source items were skipped or marked without merging%'
        OR s.data->>'summary_text' LIKE 'All % exceeded token_limit=%only marks the batch as rolled up.%')
    UNION ALL
    SELECT 'subject_memory_oversized_skip',m.tenant_id,m.source_id,NULL,
           COALESCE(NULLIF(BTRIM(m.data #>> '{metadata,project_subject_id}'),''),'__none__'),
           'subject_memory',m.id,ARRAY[]::TEXT[],
           jsonb_build_object('subject_id',m.subject_id,'memory_key',m.memory_key,'level',m.level)
      FROM engine_subject_memories m
     WHERE ($1::TEXT IS NULL OR m.tenant_id=$1)
       AND ($2::TEXT IS NULL OR m.source_id=$2)
       AND m.level>0
       AND m.data->>'text' LIKE '%oversized source memories were marked rolled up without being merged%'
    UNION ALL
    SELECT 'cross_project_rollup',output.tenant_id,output.source_id,output.thread_id,'__mixed__',
           'thread_summary',output.id,ARRAY_AGG(input.id ORDER BY input.created_at,input.id),
           jsonb_build_object('input_partitions',ARRAY_AGG(DISTINCT input.partition_key ORDER BY input.partition_key))
      FROM summary_rows input
      JOIN summary_rows output ON output.tenant_id=input.tenant_id AND output.source_id=input.source_id
                              AND output.thread_id=input.thread_id AND output.id=input.rollup_summary_id
     GROUP BY output.tenant_id,output.source_id,output.thread_id,output.id
    HAVING COUNT(DISTINCT input.partition_key)>1
    UNION ALL
    SELECT 'duplicate_ancestor_descendant_derivation',input.tenant_id,input.source_id,input.thread_id,
           input.partition_key,'thread_summary',output.id,ARRAY[input.id,output.id],
           jsonb_build_object('ancestor_id',input.id,'descendant_id',output.id)
      FROM summary_rows input
      JOIN summary_rows output ON output.tenant_id=input.tenant_id AND output.source_id=input.source_id
                              AND output.thread_id=input.thread_id AND output.id=input.rollup_summary_id
     WHERE (input.subject_memory_scope_keys && output.subject_memory_scope_keys)
        OR (CARDINALITY(input.subject_memory_scope_keys)=0
            AND CARDINALITY(output.subject_memory_scope_keys)=0
            AND input.subject_memory_summarized>0 AND output.subject_memory_summarized>0)
)
SELECT finding_kind,tenant_id,source_id,thread_id,project_partition_key,object_kind,object_id,
       related_object_ids,detail
  FROM findings
 ORDER BY tenant_id,source_id,project_partition_key,finding_kind,object_id
"#;

const BASELINE_SQL: &str = r#"
SELECT
  (SELECT COUNT(*) FROM engine_threads t WHERE ($1::TEXT IS NULL OR t.tenant_id=$1) AND ($2::TEXT IS NULL OR t.source_id=$2) AND t.pending_record_count>0)::BIGINT AS pending_threads,
  (SELECT COALESCE(SUM(t.pending_record_count),0) FROM engine_threads t WHERE ($1::TEXT IS NULL OR t.tenant_id=$1) AND ($2::TEXT IS NULL OR t.source_id=$2))::BIGINT AS pending_records,
  (SELECT COALESCE(SUM(t.pending_summary_tokens),0) FROM engine_threads t WHERE ($1::TEXT IS NULL OR t.tenant_id=$1) AND ($2::TEXT IS NULL OR t.source_id=$2))::BIGINT AS pending_tokens,
  (SELECT COUNT(*) FROM engine_threads t WHERE ($1::TEXT IS NULL OR t.tenant_id=$1) AND ($2::TEXT IS NULL OR t.source_id=$2) AND t.summary_dispatch_pending)::BIGINT AS summary_dispatch_pending,
  (SELECT COUNT(*) FROM engine_threads t WHERE ($1::TEXT IS NULL OR t.tenant_id=$1) AND ($2::TEXT IS NULL OR t.source_id=$2) AND t.summary_dispatch_dead_letter_version IS NOT NULL)::BIGINT AS summary_dispatch_dead_lettered,
  (SELECT COUNT(*) FROM engine_summaries s WHERE ($1::TEXT IS NULL OR s.tenant_id=$1) AND ($2::TEXT IS NULL OR s.source_id=$2) AND s.status='done' AND s.rollup_status='pending')::BIGINT AS rollup_inputs_pending,
  (SELECT COUNT(*) FROM engine_summaries s WHERE ($1::TEXT IS NULL OR s.tenant_id=$1) AND ($2::TEXT IS NULL OR s.source_id=$2) AND s.rollup_dispatch_pending)::BIGINT AS rollup_dispatch_pending,
  (SELECT COUNT(*) FROM engine_summaries s WHERE ($1::TEXT IS NULL OR s.tenant_id=$1) AND ($2::TEXT IS NULL OR s.source_id=$2) AND s.subject_memory_source_dispatch_pending)::BIGINT AS subject_memory_source_dispatch_pending,
  (SELECT COUNT(*) FROM engine_subject_memories m WHERE ($1::TEXT IS NULL OR m.tenant_id=$1) AND ($2::TEXT IS NULL OR m.source_id=$2) AND m.status='active' AND m.rollup_status='pending')::BIGINT AS subject_memory_rollup_pending,
  (SELECT COUNT(*) FROM engine_job_runs j WHERE ($1::TEXT IS NULL OR j.tenant_id=$1) AND ($2::TEXT IS NULL OR j.source_id=$2) AND j.status='running')::BIGINT AS running_jobs,
  (SELECT COUNT(*) FROM engine_job_runs j WHERE ($1::TEXT IS NULL OR j.tenant_id=$1) AND ($2::TEXT IS NULL OR j.source_id=$2) AND j.status='failed' AND j.started_at>=NOW()-INTERVAL '24 hours')::BIGINT AS failed_jobs_last_24h,
  ((SELECT COUNT(*) FROM engine_summaries s WHERE ($1::TEXT IS NULL OR s.tenant_id=$1) AND ($2::TEXT IS NULL OR s.source_id=$2) AND s.data->>'summary_text' LIKE '%Context overflow retry count:%')
   +(SELECT COUNT(*) FROM engine_subject_memories m WHERE ($1::TEXT IS NULL OR m.tenant_id=$1) AND ($2::TEXT IS NULL OR m.source_id=$2) AND m.data->>'text' LIKE '%Context overflow retry count:%'))::BIGINT AS outputs_with_provider_overflow_retry
"#;

const SOURCE_READINESS_SQL: &str = r#"
WITH scoped_sources AS (
  SELECT COALESCE(s.tenant_id,'') AS tenant_id,s.source_id,s.source_type
    FROM engine_sources s
   WHERE ($1::TEXT IS NULL OR s.tenant_id=$1)
     AND ($2::TEXT IS NULL OR s.source_id=$2)
), record_readiness AS (
  SELECT r.tenant_id,r.source_id,COUNT(*)::BIGINT AS total_records,
         COUNT(*) FILTER (WHERE
           NULLIF(BTRIM(r.data #>> '{metadata,conversation_turn_id}'),'') IS NOT NULL
           AND NULLIF(BTRIM(r.data #>> '{metadata,project_context_assertion_id}'),'') IS NOT NULL
           AND NULLIF(BTRIM(r.data #>> '{metadata,content_identity_digest}'),'') IS NOT NULL
           AND CASE WHEN r.role='tool' OR r.data #> '{metadata,structured_result}' IS NOT NULL THEN
             NULLIF(BTRIM(r.data #>> '{metadata,assistant_record_id}'),'') IS NOT NULL
             AND NULLIF(BTRIM(r.data #>> '{metadata,tool_exchange_id}'),'') IS NOT NULL
             AND NULLIF(BTRIM(r.data #>> '{metadata,tool_call_id}'),'') IS NOT NULL
             AND r.data #> '{metadata,tool_result_ordinal}' IS NOT NULL
             AND r.data #> '{metadata,tool_result_count}' IS NOT NULL
           ELSE TRUE END
         )::BIGINT AS ready_records,
         COUNT(*) FILTER (WHERE NULLIF(BTRIM(r.data #>> '{metadata,conversation_turn_id}'),'') IS NULL)::BIGINT AS missing_conversation_turn_id,
         COUNT(*) FILTER (WHERE (r.role='tool' OR r.data #> '{metadata,structured_result}' IS NOT NULL) AND (
             NULLIF(BTRIM(r.data #>> '{metadata,assistant_record_id}'),'') IS NULL
             OR NULLIF(BTRIM(r.data #>> '{metadata,tool_exchange_id}'),'') IS NULL
             OR NULLIF(BTRIM(r.data #>> '{metadata,tool_call_id}'),'') IS NULL
             OR r.data #> '{metadata,tool_result_ordinal}' IS NULL
             OR r.data #> '{metadata,tool_result_count}' IS NULL
         ))::BIGINT AS missing_tool_exchange_contract,
         COUNT(*) FILTER (WHERE NULLIF(BTRIM(r.data #>> '{metadata,project_context_assertion_id}'),'') IS NULL)::BIGINT AS missing_project_context_assertion_id,
         COUNT(*) FILTER (WHERE NULLIF(BTRIM(r.data #>> '{metadata,content_identity_digest}'),'') IS NULL)::BIGINT AS missing_content_identity_digest
    FROM engine_records r
   WHERE ($1::TEXT IS NULL OR r.tenant_id=$1)
     AND ($2::TEXT IS NULL OR r.source_id=$2)
   GROUP BY r.tenant_id,r.source_id
)
SELECT s.tenant_id,s.source_id,s.source_type,
       COALESCE(r.total_records,0)::BIGINT AS total_records,
       COALESCE(r.ready_records,0)::BIGINT AS ready_records,
       CASE WHEN COALESCE(r.total_records,0)=0 THEN 'unknown'
            WHEN r.ready_records=r.total_records THEN 'ready' ELSE 'not_ready' END AS readiness_state,
       COALESCE(r.missing_conversation_turn_id,0)::BIGINT AS missing_conversation_turn_id,
       COALESCE(r.missing_tool_exchange_contract,0)::BIGINT AS missing_tool_exchange_contract,
       COALESCE(r.missing_project_context_assertion_id,0)::BIGINT AS missing_project_context_assertion_id,
       COALESCE(r.missing_content_identity_digest,0)::BIGINT AS missing_content_identity_digest
  FROM scoped_sources s
  LEFT JOIN record_readiness r ON r.tenant_id=s.tenant_id AND r.source_id=s.source_id
 ORDER BY s.tenant_id,s.source_id
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readiness_inventory_covers_all_required_runtime_surfaces() {
        let inventory = runtime_readiness_inventory().unwrap();
        let names = inventory
            .components
            .iter()
            .map(|component| component.component.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            names,
            [
                "rust_runtime",
                "local_agent_host",
                "macos",
                "windows",
                "internal_callers"
            ]
            .into_iter()
            .collect()
        );
        assert!(inventory.components.iter().all(|component| {
            !component.evidence.is_empty()
                && !(component.source_unit_ready
                    && component.runtime_assertion_ready
                    && component.model_capability_ready)
        }));
    }

    #[test]
    fn audit_queries_remain_read_only() {
        for sql in [FINDINGS_SQL, BASELINE_SQL, SOURCE_READINESS_SQL] {
            let upper = sql.to_ascii_uppercase();
            for forbidden in [" INSERT ", " UPDATE ", " DELETE ", " ALTER ", " DROP "] {
                assert!(!upper.contains(forbidden), "found {forbidden} in audit SQL");
            }
        }
    }
}
