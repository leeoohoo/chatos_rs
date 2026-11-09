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
    pub alerts: Vec<ReliabilityAlert>,
    pub baseline: ReliabilityAuditBaseline,
    pub source_contract_readiness: Vec<SourceContractReadiness>,
    pub runtime_readiness: RuntimeReadinessInventory,
    pub unavailable_baselines: Vec<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReliabilityAlert {
    pub code: String,
    pub severity: &'static str,
    pub count: usize,
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
    let alerts = build_alerts(&findings);
    let baseline = load_baseline(db, &scope).await?;
    let source_contract_readiness = load_source_contract_readiness(db, &scope).await?;
    Ok(ReliabilityAuditReport {
        schema_version: "memory_reliability_audit_v1",
        generated_at: chrono::Utc::now().to_rfc3339(),
        scope,
        finding_count: findings.len(),
        findings,
        alerts,
        baseline,
        source_contract_readiness,
        runtime_readiness: runtime_readiness_inventory()?,
        unavailable_baselines: vec![
            "pre-M0 compose token history is not persisted; live totals and max are exposed by /metrics",
            "generation replay identity does not exist before M1b",
        ],
    })
}

fn build_alerts(findings: &[ReliabilityAuditFinding]) -> Vec<ReliabilityAlert> {
    let mut counts = std::collections::BTreeMap::<String, usize>::new();
    for finding in findings {
        *counts.entry(finding.finding_kind.clone()).or_default() += 1;
    }
    counts
        .into_iter()
        .map(|(code, count)| ReliabilityAlert {
            severity: match code.as_str() {
                "broken_active_coverage"
                | "summarized_without_active_coverage"
                | "broken_active_derivation_edge"
                | "cross_project_rollup"
                | "duplicate_ancestor_descendant_derivation" => "critical",
                _ => "warning",
            },
            code,
            count,
        })
        .collect()
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
           COALESCE(r.project_partition_key,'__none__') AS partition_key
      FROM engine_records r
     WHERE ($1::TEXT IS NULL OR r.tenant_id=$1)
       AND ($2::TEXT IS NULL OR r.source_id=$2)
), summary_rows AS (
    SELECT s.*,
           NULLIF(BTRIM(s.data->>'rollup_summary_id'),'') AS rollup_summary_id,
           COALESCE(s.project_partition_key,'__none__') AS partition_key
      FROM engine_summaries s
     WHERE ($1::TEXT IS NULL OR s.tenant_id=$1)
       AND ($2::TEXT IS NULL OR s.source_id=$2)
), findings AS (
    SELECT 'summarized_without_active_coverage'::TEXT AS finding_kind,
           r.tenant_id,r.source_id,r.thread_id,r.project_partition_key,
           'record'::TEXT AS object_kind,r.id AS object_id,
           ARRAY_REMOVE(ARRAY[r.summary_id],NULL) AS related_object_ids,
           jsonb_build_object('summary_status',r.summary_status,'summary_id',r.summary_id) AS detail
      FROM engine_records r
     WHERE r.recall_eligible AND r.summary_status='summarized'
       AND ($1::TEXT IS NULL OR r.tenant_id=$1) AND ($2::TEXT IS NULL OR r.source_id=$2)
       AND NOT EXISTS (SELECT 1 FROM engine_summary_record_coverage coverage
         WHERE coverage.tenant_id=r.tenant_id AND coverage.source_id=r.source_id
           AND coverage.record_id=r.id AND coverage.status='active')
    UNION ALL
    SELECT 'broken_active_coverage',coverage.tenant_id,coverage.source_id,coverage.thread_id,
           coverage.project_partition_key,'coverage',coverage.id,
           ARRAY[coverage.summary_id,coverage.record_id],
           jsonb_build_object('summary_id',coverage.summary_id,'record_id',coverage.record_id)
      FROM engine_summary_record_coverage coverage
 LEFT JOIN engine_summaries output ON output.tenant_id=coverage.tenant_id
       AND output.source_id=coverage.source_id AND output.id=coverage.summary_id
 LEFT JOIN engine_records input ON input.tenant_id=coverage.tenant_id
       AND input.source_id=coverage.source_id AND input.id=coverage.record_id
     WHERE coverage.status='active'
       AND ($1::TEXT IS NULL OR coverage.tenant_id=$1)
       AND ($2::TEXT IS NULL OR coverage.source_id=$2)
       AND (output.id IS NULL OR NOT output.recall_eligible OR output.status<>'done'
         OR input.id IS NULL OR NOT input.recall_eligible OR input.summary_id IS DISTINCT FROM coverage.summary_id
         OR output.thread_id IS DISTINCT FROM coverage.thread_id
         OR input.thread_id IS DISTINCT FROM coverage.thread_id
         OR output.project_partition_key IS DISTINCT FROM coverage.project_partition_key
         OR input.project_partition_key IS DISTINCT FROM coverage.project_partition_key)
    UNION ALL
    SELECT 'broken_active_derivation_edge',edge.tenant_id,edge.source_id,NULL,
           edge.input_project_partition_key,'derivation_edge',edge.id,
           ARRAY[edge.input_id,edge.output_id],
           jsonb_build_object('input_kind',edge.input_kind,'output_kind',edge.output_kind,
             'derivation_kind',edge.derivation_kind)
      FROM engine_memory_derivation_edges edge
     WHERE edge.status='active'
       AND ($1::TEXT IS NULL OR edge.tenant_id=$1) AND ($2::TEXT IS NULL OR edge.source_id=$2)
       AND (CASE edge.input_kind
              WHEN 'thread_summary' THEN NOT EXISTS (SELECT 1 FROM engine_summaries s
                WHERE s.tenant_id=edge.tenant_id AND s.source_id=edge.source_id AND s.id=edge.input_id
                  AND s.recall_eligible AND s.status='done')
              ELSE NOT EXISTS (SELECT 1 FROM engine_subject_memories m
                WHERE m.tenant_id=edge.tenant_id AND m.source_id=edge.source_id AND m.id=edge.input_id
                  AND m.recall_eligible AND m.status='active') END
         OR CASE edge.output_kind
              WHEN 'thread_summary' THEN NOT EXISTS (SELECT 1 FROM engine_summaries s
                WHERE s.tenant_id=edge.tenant_id AND s.source_id=edge.source_id AND s.id=edge.output_id
                  AND s.recall_eligible AND s.status='done')
              ELSE NOT EXISTS (SELECT 1 FROM engine_subject_memories m
                WHERE m.tenant_id=edge.tenant_id AND m.source_id=edge.source_id AND m.id=edge.output_id
                  AND m.recall_eligible AND m.status='active') END)
    UNION ALL
    SELECT 'derived_event_dead_letter',event.tenant_id,event.source_id,NULL,
           COALESCE(event.project_partition_key,'__none__'),'derived_event',event.event_id,
           ARRAY[event.aggregate_id],jsonb_build_object('event_kind',event.event_kind,
             'attempt_count',event.attempt_count,'last_error_code',event.last_error_code)
      FROM engine_derived_event_outbox event WHERE event.status='dead_letter'
       AND ($1::TEXT IS NULL OR event.tenant_id=$1) AND ($2::TEXT IS NULL OR event.source_id=$2)
    UNION ALL
    SELECT 'partition_rebuild_failed',backlog.tenant_id,backlog.source_id,backlog.thread_id,
           backlog.project_partition_key,'thread_partition',
           backlog.thread_id || ':' || backlog.rebuild_kind,ARRAY[]::TEXT[],
           jsonb_build_object('rebuild_kind',backlog.rebuild_kind,'attempt_count',backlog.attempt_count,
             'last_error',backlog.last_error)
      FROM engine_partition_rebuild_backlog backlog WHERE backlog.status='failed'
       AND ($1::TEXT IS NULL OR backlog.tenant_id=$1) AND ($2::TEXT IS NULL OR backlog.source_id=$2)
    UNION ALL
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
SELECT finding_kind,tenant_id,source_id,thread_id,
       COALESCE(project_partition_key,'__none__') AS project_partition_key,object_kind,object_id,
       related_object_ids,detail
  FROM findings
 ORDER BY tenant_id,source_id,project_partition_key,finding_kind,object_id
"#;

const BASELINE_SQL: &str = r#"
SELECT
  (SELECT COUNT(DISTINCT (p.tenant_id,p.source_id,p.thread_id)) FROM engine_thread_summary_partitions p WHERE ($1::TEXT IS NULL OR p.tenant_id=$1) AND ($2::TEXT IS NULL OR p.source_id=$2) AND p.pending_record_count>0)::BIGINT AS pending_threads,
  (SELECT COALESCE(SUM(p.pending_record_count),0) FROM engine_thread_summary_partitions p WHERE ($1::TEXT IS NULL OR p.tenant_id=$1) AND ($2::TEXT IS NULL OR p.source_id=$2))::BIGINT AS pending_records,
  (SELECT COALESCE(SUM(p.pending_summary_tokens),0) FROM engine_thread_summary_partitions p WHERE ($1::TEXT IS NULL OR p.tenant_id=$1) AND ($2::TEXT IS NULL OR p.source_id=$2))::BIGINT AS pending_tokens,
  (SELECT COUNT(*) FROM engine_thread_summary_partitions p WHERE ($1::TEXT IS NULL OR p.tenant_id=$1) AND ($2::TEXT IS NULL OR p.source_id=$2) AND p.summary_dispatch_pending)::BIGINT AS summary_dispatch_pending,
  (SELECT COUNT(*) FROM engine_thread_summary_partitions p WHERE ($1::TEXT IS NULL OR p.tenant_id=$1) AND ($2::TEXT IS NULL OR p.source_id=$2) AND p.summary_dispatch_dead_letter_version IS NOT NULL)::BIGINT AS summary_dispatch_dead_lettered,
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
                && component.state == "ready"
                && component.missing.is_empty()
                && component.source_unit_ready
                && component.runtime_assertion_ready
                && component.model_capability_ready
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
