// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use sqlx::types::Json;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::config::AppConfig;
use crate::db::Db;
use crate::models::{
    CreateEngineJobRunRequest, EngineRecord, FinishEngineJobRunRequest,
    RunPendingSummaryRebuildResponse,
};
use crate::repositories::{control_plane, generation_commits, generation_output_commits, records};

use super::builders::{
    build_summary_text, THREAD_SUMMARY_GENERATION_PROFILE, THREAD_SUMMARY_PIPELINE_REVISION,
};
use super::settings::load_summary_job_settings;

const RESUMMARIZE_STEP: &str = "thread_resummarize_final";
const RESUMMARIZE_LEASE_SECONDS: i64 = 900;

pub(crate) struct PreparedCoveredRangeResummarization {
    pub(crate) summary: crate::models::EngineSummary,
    pub(crate) previous_summary_id: String,
    pub(crate) project_partition_key: String,
    record_ids: Vec<String>,
    covered_records: Vec<EngineRecord>,
    pub(crate) job_run_id: String,
    settings: super::SummaryJobSettings,
}

pub async fn run_pending_summary_rebuild(
    config: &AppConfig,
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    project_partition_key: &str,
) -> Result<RunPendingSummaryRebuildResponse, String> {
    let active = super::active_summary::run_thread_active_summary(
        config,
        db,
        tenant_id,
        source_id,
        thread_id,
        project_partition_key,
        Some("coverage_repair"),
    )
    .await?;
    Ok(RunPendingSummaryRebuildResponse {
        thread_id: active.thread_id,
        accepted: active.accepted,
        running: active.running,
        job_run_id: active.job_run_id,
        generated: active.generated,
        summary_id: active.summary_id,
        source_record_count: active.source_record_count,
    })
}

pub(crate) async fn prepare_covered_range_resummarize(
    config: &AppConfig,
    _db: &Db,
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: &str,
    source_id: &str,
    summary_id: &str,
    reason: &str,
) -> Result<PreparedCoveredRangeResummarization, String> {
    let reason = reason.trim();
    if reason.len() < 8 {
        return Err("resummarize reason must contain at least 8 characters".to_string());
    }
    let (summary, record_ids, covered_records, project_partition_key) =
        freeze_covered_range(tx, tenant_id, source_id, summary_id).await?;
    let lease_token = format!("resumlease_{}", Uuid::new_v4());
    let claimed = sqlx::query(
        "INSERT INTO engine_covered_range_resummarization_claims(tenant_id,source_id,summary_id, \
         status,lease_token,lease_expires_at,reason) \
         VALUES($1,$2,$3,'running',$4,now()+make_interval(secs=>$5),$6) \
         ON CONFLICT(tenant_id,source_id,summary_id) DO UPDATE SET status='running',job_run_id=NULL, \
           lease_token=EXCLUDED.lease_token,lease_expires_at=EXCLUDED.lease_expires_at, \
           reason=EXCLUDED.reason,last_error=NULL,completed_at=NULL,updated_at=now() \
         WHERE engine_covered_range_resummarization_claims.status='failed' \
            OR (engine_covered_range_resummarization_claims.status='running' \
                AND engine_covered_range_resummarization_claims.lease_expires_at<=now() \
                AND NOT EXISTS (SELECT 1 FROM engine_job_runs job \
                  WHERE job.id=engine_covered_range_resummarization_claims.job_run_id \
                    AND job.status='running'))",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(summary_id)
    .bind(&lease_token)
    .bind(RESUMMARIZE_LEASE_SECONDS as f64)
    .bind(reason)
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    if claimed.rows_affected() != 1 {
        return Err("covered range already has an active resummarization".to_string());
    }

    let settings = load_summary_job_settings(config, tenant_id).await?;
    let job = control_plane::create_job_run_in_tx(
        tx,
        CreateEngineJobRunRequest {
            job_type: "summary".to_string(),
            trigger_type: "covered_range_resummarize".to_string(),
            tenant_id: Some(tenant_id.to_string()),
            source_id: Some(source_id.to_string()),
            thread_id: Some(summary.thread_id.clone()),
            subject_id: Some(summary.subject_id.clone()),
            thread_label: None,
            metadata: Some(serde_json::json!({
                "previous_summary_id": summary_id,
                "project_partition_key": project_partition_key,
                "frozen_record_ids": record_ids,
                "source_batch_capacity_tokens": settings.source_batch_token_limit,
                "model_input_limit_tokens": settings.token_limit,
                "reason": reason,
            })),
        },
    )
    .await?;
    let attached = sqlx::query(
        "UPDATE engine_covered_range_resummarization_claims SET job_run_id=$5,updated_at=now() \
         WHERE tenant_id=$1 AND source_id=$2 AND summary_id=$3 AND lease_token=$4 \
           AND status='running'",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(summary_id)
    .bind(&lease_token)
    .bind(&job.id)
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    if attached.rows_affected() != 1 {
        return Err("covered-range resummarization claim was lost before job start".to_string());
    }

    Ok(PreparedCoveredRangeResummarization {
        summary,
        previous_summary_id: summary_id.to_string(),
        project_partition_key,
        record_ids,
        covered_records,
        job_run_id: job.id,
        settings,
    })
}

pub(crate) async fn execute_prepared_covered_range_resummarize(
    config: &AppConfig,
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    prepared: PreparedCoveredRangeResummarization,
) -> Result<RunPendingSummaryRebuildResponse, String> {
    execute_resummarization(
        config,
        db,
        tenant_id,
        source_id,
        prepared.summary,
        prepared.previous_summary_id.as_str(),
        prepared.project_partition_key.as_str(),
        prepared.record_ids,
        prepared.covered_records,
        prepared.job_run_id.as_str(),
        prepared.settings,
    )
    .await
}

pub(crate) async fn resume_covered_range_resummarize(
    config: &AppConfig,
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    job_run_id: &str,
) -> Result<RunPendingSummaryRebuildResponse, String> {
    let job = control_plane::get_job_run_by_id(db, job_run_id)
        .await?
        .ok_or_else(|| "resummarization job not found".to_string())?;
    if job.status == "done" {
        return Ok(RunPendingSummaryRebuildResponse {
            thread_id: thread_id.to_string(),
            accepted: true,
            running: false,
            job_run_id: Some(job_run_id.to_string()),
            generated: true,
            summary_id: job
                .metadata
                .as_ref()
                .and_then(|value| value.get("generated_summary_id"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
            source_record_count: job.input_count.max(0) as usize,
        });
    }
    if job.tenant_id.as_deref() != Some(tenant_id)
        || job.source_id.as_deref() != Some(source_id)
        || job.thread_id.as_deref() != Some(thread_id)
    {
        return Err("resummarization job scope is invalid".to_string());
    }
    if job.status != "running" {
        return Ok(RunPendingSummaryRebuildResponse {
            thread_id: thread_id.to_string(),
            accepted: true,
            running: false,
            job_run_id: Some(job_run_id.to_string()),
            generated: false,
            summary_id: None,
            source_record_count: job.input_count.max(0) as usize,
        });
    }
    let metadata = job
        .metadata
        .as_ref()
        .ok_or_else(|| "resummarization job metadata is missing".to_string())?;
    let previous_summary_id = metadata
        .get("previous_summary_id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "resummarization previous summary is missing".to_string())?;
    let owns_claim = refresh_covered_range_resummarize_claim(
        db,
        tenant_id,
        source_id,
        previous_summary_id,
        job_run_id,
        RESUMMARIZE_LEASE_SECONDS,
    )
    .await?;
    if !owns_claim {
        fail_covered_range_resummarize(
            db,
            tenant_id,
            source_id,
            thread_id,
            job_run_id,
            "covered-range claim was superseded before terminal callback".to_string(),
        )
        .await?;
        return Ok(RunPendingSummaryRebuildResponse {
            thread_id: thread_id.to_string(),
            accepted: true,
            running: false,
            job_run_id: Some(job_run_id.to_string()),
            generated: false,
            summary_id: None,
            source_record_count: job.input_count.max(0) as usize,
        });
    }
    let partition = metadata
        .get("project_partition_key")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "resummarization partition is missing".to_string())?;
    let record_ids = metadata
        .get("frozen_record_ids")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "resummarization frozen records are missing".to_string())?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| "invalid frozen record id".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let settings = super::SummaryJobSettings {
        source_batch_token_limit: metadata
            .get("source_batch_capacity_tokens")
            .and_then(serde_json::Value::as_i64)
            .ok_or_else(|| "resummarization max source batch tokens are missing".to_string())?,
        token_limit: metadata
            .get("model_input_limit_tokens")
            .and_then(serde_json::Value::as_i64)
            .ok_or_else(|| "resummarization max model input tokens are missing".to_string())?,
        cloud_owner_entity_id: None,
        cloud_resume_kind: None,
    };
    let summary = crate::repositories::summaries::get_summary_by_id(
        db,
        tenant_id,
        source_id,
        previous_summary_id,
    )
    .await?
    .ok_or_else(|| "resummarization source summary is no longer active".to_string())?;
    let covered_records =
        records::list_records_by_ids(db, tenant_id, source_id, thread_id, &record_ids).await?;
    let source_record_count = record_ids.len();
    let result = execute_resummarization(
        config,
        db,
        tenant_id,
        source_id,
        summary,
        previous_summary_id,
        partition,
        record_ids,
        covered_records,
        job_run_id,
        settings,
    )
    .await;
    match result {
        Ok(response) => Ok(response),
        Err(error) => {
            let current = control_plane::get_job_run_by_id(db, job_run_id)
                .await?
                .ok_or_else(|| {
                    format!("resummarization job not found after callback: {job_run_id}")
                })?;
            if current.status != "running" {
                return Ok(acknowledged_resummarization_response(
                    thread_id,
                    job_run_id,
                    source_record_count,
                ));
            }
            let still_owns_claim = refresh_covered_range_resummarize_claim(
                db,
                tenant_id,
                source_id,
                previous_summary_id,
                job_run_id,
                RESUMMARIZE_LEASE_SECONDS,
            )
            .await?;
            if !still_owns_claim {
                fail_covered_range_resummarize(
                    db,
                    tenant_id,
                    source_id,
                    thread_id,
                    job_run_id,
                    "covered-range claim was superseded during terminal callback".to_string(),
                )
                .await?;
                return Ok(acknowledged_resummarization_response(
                    thread_id,
                    job_run_id,
                    source_record_count,
                ));
            }
            Err(error)
        }
    }
}

fn acknowledged_resummarization_response(
    thread_id: &str,
    job_run_id: &str,
    source_record_count: usize,
) -> RunPendingSummaryRebuildResponse {
    RunPendingSummaryRebuildResponse {
        thread_id: thread_id.to_string(),
        accepted: true,
        running: false,
        job_run_id: Some(job_run_id.to_string()),
        generated: false,
        summary_id: None,
        source_record_count,
    }
}

#[allow(clippy::too_many_arguments)]
async fn execute_resummarization(
    config: &AppConfig,
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    summary: crate::models::EngineSummary,
    previous_summary_id: &str,
    project_partition_key: &str,
    record_ids: Vec<String>,
    covered_records: Vec<EngineRecord>,
    job_run_id: &str,
    mut settings: super::SummaryJobSettings,
) -> Result<RunPendingSummaryRebuildResponse, String> {
    let descriptor = resummarization_descriptor(
        &covered_records,
        &settings,
        project_partition_key,
        previous_summary_id,
    )?;
    settings.cloud_owner_entity_id = Some(job_run_id.to_string());
    settings.cloud_resume_kind =
        Some(crate::services::memory_cloud_agent::THREAD_RESUMMARIZE_JOB_RESUME_KIND.to_string());
    let build = build_summary_text(
        config,
        db,
        tenant_id,
        None,
        summary.subject_id.as_str(),
        &covered_records,
        &settings,
        descriptor,
    )
    .await;
    let build = match build {
        Err(error) if error == crate::services::memory_cloud_agent::MEMORY_CLOUD_AGENT_DEFERRED => {
            return Ok(RunPendingSummaryRebuildResponse {
                thread_id: summary.thread_id,
                accepted: true,
                running: true,
                job_run_id: Some(job_run_id.to_string()),
                generated: false,
                summary_id: None,
                source_record_count: record_ids.len(),
            });
        }
        Err(error) => {
            fail_covered_range_resummarize(
                db,
                tenant_id,
                source_id,
                summary.thread_id.as_str(),
                job_run_id,
                error.clone(),
            )
            .await?;
            return Err(error);
        }
        Ok(build) => build,
    };
    let prepared = generation_commits::get_generation_by_job_step(
        db,
        tenant_id,
        source_id,
        job_run_id,
        RESUMMARIZE_STEP,
    )
    .await?
    .ok_or_else(|| "resummarization generation was not frozen".to_string())?;
    let generation = if prepared.status == "committed" {
        prepared
    } else {
        generation_commits::claim_generation(
            db,
            tenant_id,
            source_id,
            prepared.generation_id.as_str(),
            format!("resummarization-finalizer:{job_run_id}").as_str(),
            RESUMMARIZE_LEASE_SECONDS,
        )
        .await?
        .ok_or_else(|| "resummarization generation finalization is occupied".to_string())?
    };
    let rendered = crate::models::structured_payload::render_payload(&build.structured_payload)?;
    let replacement = generation_output_commits::commit_thread_summary_resummarization_final(
        db,
        &generation,
        generation_output_commits::ThreadSummaryGenerationOutput {
            thread_id: summary.thread_id.as_str(),
            subject_id: summary.subject_id.as_str(),
            summary_type: "thread_incremental",
            level: 0,
            source_digest: None,
            summary_text: rendered.as_str(),
            source_record_start_id: record_ids.first().cloned(),
            source_record_end_id: record_ids.last().cloned(),
            source_record_count: record_ids.len(),
            metadata: Some(serde_json::json!({
                "project_subject_id": project_partition_key,
                "generation_profile": THREAD_SUMMARY_GENERATION_PROFILE,
                "supersedes_summary_id": previous_summary_id,
            })),
            generation_reason: "resummarize",
            generation_profile: THREAD_SUMMARY_GENERATION_PROFILE,
            project_partition_key,
            payload_schema: crate::models::structured_payload::MEMORY_PAYLOAD_SCHEMA,
            structured_payload: &build.structured_payload,
            pipeline_revision: THREAD_SUMMARY_PIPELINE_REVISION,
        },
        generation_output_commits::ThreadL0ResummarizationFinalization {
            previous_summary_id,
            record_ids: &record_ids,
            job_result: FinishEngineJobRunRequest {
                status: "done".to_string(),
                input_count: record_ids.len() as i64,
                output_count: 1,
                processed_count: record_ids.len() as i64,
                success_count: record_ids.len() as i64,
                error_count: 0,
                metadata: Some(serde_json::json!({
                    "previous_summary_id": previous_summary_id,
                    "project_partition_key": project_partition_key,
                    "frozen_record_ids": record_ids,
                    "source_batch_capacity_tokens": settings.source_batch_token_limit,
                    "model_input_limit_tokens": settings.token_limit,
                })),
                error_message: None,
            },
        },
    )
    .await?;
    Ok(RunPendingSummaryRebuildResponse {
        thread_id: summary.thread_id,
        accepted: true,
        running: false,
        job_run_id: Some(job_run_id.to_string()),
        generated: true,
        summary_id: Some(replacement.id),
        source_record_count: record_ids.len(),
    })
}

pub(crate) async fn fail_covered_range_resummarize(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    job_run_id: &str,
    error: String,
) -> Result<(), String> {
    let Some(job) = control_plane::get_job_run_by_id(db, job_run_id).await? else {
        return Ok(());
    };
    if job.status != "running" || job.thread_id.as_deref() != Some(thread_id) {
        return Ok(());
    }
    let previous_summary_id = job
        .metadata
        .as_ref()
        .and_then(|value| value.get("previous_summary_id"))
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "resummarization failure has no previous summary".to_string())?;
    fail_claim(
        db,
        tenant_id,
        source_id,
        previous_summary_id,
        Some(job_run_id),
        &error,
    )
    .await;
    control_plane::finish_job_run(
        db,
        job_run_id,
        FinishEngineJobRunRequest {
            status: "failed".to_string(),
            input_count: job.input_count,
            output_count: 0,
            processed_count: 0,
            success_count: 0,
            error_count: 1,
            metadata: job.metadata,
            error_message: Some(error),
        },
    )
    .await?;
    Ok(())
}

async fn freeze_covered_range(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: &str,
    source_id: &str,
    summary_id: &str,
) -> Result<
    (
        crate::models::EngineSummary,
        Vec<String>,
        Vec<EngineRecord>,
        String,
    ),
    String,
> {
    let summary: crate::models::EngineSummary = sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "SELECT data FROM engine_summaries WHERE tenant_id=$1 AND source_id=$2 AND id=$3 \
         AND recall_eligible FOR UPDATE",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(summary_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| error.to_string())?
    .map(crate::repositories::postgres::decode)
    .transpose()?
    .ok_or_else(|| "covered-range summary not found or not recall eligible".to_string())?;
    if summary.level != 0
        || summary.summary_type != "thread_incremental"
        || summary.status != "done"
    {
        return Err(
            "covered-range resummarization only accepts an active canonical L0 summary".to_string(),
        );
    }
    let rows = sqlx::query_as::<_, (String, String, Json<serde_json::Value>)>(
        "SELECT coverage.record_id,coverage.project_partition_key,record.data \
         FROM engine_summary_record_coverage coverage JOIN engine_records record \
           ON record.tenant_id=coverage.tenant_id AND record.source_id=coverage.source_id \
          AND record.id=coverage.record_id \
         WHERE coverage.tenant_id=$1 AND coverage.source_id=$2 AND coverage.summary_id=$3 \
           AND coverage.status='active' AND record.recall_eligible ORDER BY coverage.ordinal",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(summary_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    if rows.is_empty() {
        return Err("covered-range summary has no active coverage".to_string());
    }
    let project_partition_key = rows[0].1.clone();
    if rows.iter().any(|row| row.1 != project_partition_key) {
        return Err("covered range crosses project partitions".to_string());
    }
    let mut record_ids = Vec::with_capacity(rows.len());
    let mut covered_records = Vec::with_capacity(rows.len());
    for (record_id, _, data) in rows {
        record_ids.push(record_id);
        covered_records.push(crate::repositories::postgres::decode(data)?);
    }
    Ok((summary, record_ids, covered_records, project_partition_key))
}

fn resummarization_descriptor(
    records: &[EngineRecord],
    settings: &super::SummaryJobSettings,
    project_partition_key: &str,
    previous_summary_id: &str,
) -> Result<serde_json::Value, String> {
    let inputs = records
        .iter()
        .map(|record| {
            serde_json::json!({
                "id": record.id, "thread_id": record.thread_id, "role": record.role,
                "record_type": record.record_type, "content": record.content,
                "structured_payload": record.structured_payload, "metadata": record.metadata,
                "created_at": record.created_at,
            })
        })
        .collect::<Vec<_>>();
    let (manifest, digest) = crate::services::generation_identity::input_manifest(inputs)?;
    Ok(crate::services::generation_identity::descriptor(
        RESUMMARIZE_STEP,
        "thread_summary",
        THREAD_SUMMARY_GENERATION_PROFILE,
        THREAD_SUMMARY_PIPELINE_REVISION,
        Some(project_partition_key),
        manifest,
        digest,
        serde_json::json!({
            "source_batch_capacity_tokens": settings.source_batch_token_limit,
            "model_input_limit_tokens": settings.token_limit,
            "previous_summary_id": previous_summary_id,
            "generation_reason": "resummarize",
        }),
    ))
}

async fn fail_claim(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    summary_id: &str,
    job_run_id: Option<&str>,
    error: &str,
) {
    let _ = sqlx::query(
        "UPDATE engine_covered_range_resummarization_claims SET status='failed',last_error=$5, \
         updated_at=now() WHERE tenant_id=$1 AND source_id=$2 AND summary_id=$3 \
           AND ($4::text IS NULL OR job_run_id=$4) AND status='running'",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(summary_id)
    .bind(job_run_id)
    .bind(error)
    .execute(db)
    .await;
}

pub(crate) async fn refresh_covered_range_resummarize_claim(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    summary_id: &str,
    job_run_id: &str,
    lease_seconds: i64,
) -> Result<bool, String> {
    sqlx::query(
        "UPDATE engine_covered_range_resummarization_claims SET \
         lease_expires_at=now()+make_interval(secs=>$5),updated_at=now() \
         WHERE tenant_id=$1 AND source_id=$2 AND summary_id=$3 AND job_run_id=$4 \
           AND status='running'",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(summary_id)
    .bind(job_run_id)
    .bind(lease_seconds.max(1) as f64)
    .execute(db)
    .await
    .map(|result| result.rows_affected() == 1)
    .map_err(|error| error.to_string())
}
