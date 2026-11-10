// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;
use sqlx::types::Json;
use sqlx::{FromRow, Postgres, Transaction};
use uuid::Uuid;

use crate::db::Db;

#[derive(Debug, Clone, FromRow, PartialEq)]
pub struct GenerationCommit {
    pub tenant_id: String,
    pub source_id: String,
    pub generation_id: String,
    pub job_run_id: String,
    pub generation_step_key: String,
    pub generation_kind: String,
    pub input_digest: String,
    pub input_manifest: Json<Value>,
    pub config_snapshot: Json<Value>,
    pub project_partition_key: Option<String>,
    pub status: String,
    pub attempt_count: i64,
    pub lease_owner: Option<String>,
    pub lease_token: Option<String>,
    pub output_kind: Option<String>,
    pub output_id: Option<String>,
}

pub struct PrepareGeneration<'a> {
    pub tenant_id: &'a str,
    pub source_id: &'a str,
    pub job_run_id: &'a str,
    pub generation_step_key: &'a str,
    pub generation_kind: &'a str,
    pub input_digest: &'a str,
    pub input_manifest: &'a Value,
    pub config_snapshot: &'a Value,
    pub project_partition_key: Option<&'a str>,
}

const COLUMNS: &str = "tenant_id,source_id,generation_id,job_run_id,generation_step_key,generation_kind,input_digest,input_manifest,config_snapshot,project_partition_key,status,attempt_count,lease_owner,lease_token,output_kind,output_id";

pub async fn prepare_generation(
    db: &Db,
    input: PrepareGeneration<'_>,
) -> Result<GenerationCommit, String> {
    validate_prepare_input(&input)?;
    let generation_id = format!("mgen_{}", Uuid::new_v4());
    let mut tx = db.begin().await.map_err(|error| error.to_string())?;
    sqlx::query(
        "INSERT INTO engine_generation_commits(tenant_id,source_id,generation_id,job_run_id, \
         generation_step_key,generation_kind,input_digest,input_manifest,config_snapshot, \
         project_partition_key,status) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,'prepared') \
         ON CONFLICT(tenant_id,source_id,job_run_id,generation_step_key) DO NOTHING",
    )
    .bind(input.tenant_id)
    .bind(input.source_id)
    .bind(&generation_id)
    .bind(input.job_run_id)
    .bind(input.generation_step_key)
    .bind(input.generation_kind)
    .bind(input.input_digest)
    .bind(Json(input.input_manifest))
    .bind(Json(input.config_snapshot))
    .bind(input.project_partition_key)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    let generation = fetch_by_job_step(
        &mut tx,
        input.tenant_id,
        input.source_id,
        input.job_run_id,
        input.generation_step_key,
    )
    .await?
    .ok_or_else(|| "generation prepare did not create or find its job step".to_string())?;
    if generation.generation_kind != input.generation_kind
        || generation.input_digest != input.input_digest
        || generation.input_manifest.0 != *input.input_manifest
        || generation.config_snapshot.0 != *input.config_snapshot
        || generation.project_partition_key.as_deref() != input.project_partition_key
    {
        return Err("generation job step replay changed frozen input or configuration".to_string());
    }
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(generation)
}

pub async fn claim_generation(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    generation_id: &str,
    lease_owner: &str,
    lease_seconds: i64,
) -> Result<Option<GenerationCommit>, String> {
    let lease_owner = lease_owner.trim();
    if lease_owner.is_empty() {
        return Err("generation lease_owner is required".to_string());
    }
    let lease_token = format!("mgenlease_{}", Uuid::new_v4());
    sqlx::query_as::<_, GenerationCommit>(sqlx::AssertSqlSafe(format!(
        "UPDATE engine_generation_commits SET status='running',attempt_count=attempt_count+1, \
         lease_owner=$4,lease_token=$5,lease_acquired_at=now(), \
         lease_expires_at=now()+make_interval(secs=>$6),last_error_code=NULL,last_error_detail=NULL \
         WHERE tenant_id=$1 AND source_id=$2 AND generation_id=$3 \
           AND (status='prepared' OR (status='running' \
                AND (lease_expires_at<=now() OR lease_owner=$4))) \
         RETURNING {COLUMNS}"
    )))
    .bind(tenant_id)
    .bind(source_id)
    .bind(generation_id)
    .bind(lease_owner)
    .bind(lease_token)
    .bind(lease_seconds.clamp(1, 86_400) as f64)
    .fetch_optional(db)
    .await
    .map_err(|error| error.to_string())
}

pub async fn mark_attempt_failed(
    db: &Db,
    generation: &GenerationCommit,
    error_code: &str,
) -> Result<bool, String> {
    let lease_token = required_lease_token(generation)?;
    let error_code = error_code.trim();
    if error_code.is_empty() {
        return Err("generation error_code is required".to_string());
    }
    let result = sqlx::query(
        "UPDATE engine_generation_commits SET status='prepared',lease_owner=NULL,lease_token=NULL, \
         lease_acquired_at=NULL,lease_expires_at=NULL,last_error_code=$5,last_error_detail=NULL,last_failed_at=now() \
         WHERE tenant_id=$1 AND source_id=$2 AND generation_id=$3 AND status='running' AND lease_token=$4",
    )
    .bind(&generation.tenant_id)
    .bind(&generation.source_id)
    .bind(&generation.generation_id)
    .bind(lease_token)
    .bind(error_code)
    .execute(db)
    .await
    .map_err(|error| error.to_string())?;
    Ok(result.rows_affected() == 1)
}

pub async fn commit_generation(
    tx: &mut Transaction<'_, Postgres>,
    generation: &GenerationCommit,
    output_kind: &str,
    output_id: &str,
) -> Result<bool, String> {
    if !matches!(output_kind, "thread_summary" | "subject_memory") {
        return Err("generation output_kind is invalid".to_string());
    }
    if output_id.trim().is_empty() {
        return Err("generation output_id is required".to_string());
    }
    let lease_token = required_lease_token(generation)?;
    let result = sqlx::query(
        "UPDATE engine_generation_commits SET status='committed',output_kind=$5,output_id=$6, \
         committed_at=now(),lease_owner=NULL,lease_token=NULL,lease_acquired_at=NULL,lease_expires_at=NULL, \
         last_error_code=NULL,last_error_detail=NULL \
         WHERE tenant_id=$1 AND source_id=$2 AND generation_id=$3 AND status='running' AND lease_token=$4",
    )
    .bind(&generation.tenant_id)
    .bind(&generation.source_id)
    .bind(&generation.generation_id)
    .bind(lease_token)
    .bind(output_kind)
    .bind(output_id)
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    Ok(result.rows_affected() == 1)
}

pub async fn get_generation(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    generation_id: &str,
) -> Result<Option<GenerationCommit>, String> {
    sqlx::query_as::<_, GenerationCommit>(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM engine_generation_commits \
         WHERE tenant_id=$1 AND source_id=$2 AND generation_id=$3"
    )))
    .bind(tenant_id)
    .bind(source_id)
    .bind(generation_id)
    .fetch_optional(db)
    .await
    .map_err(|error| error.to_string())
}

pub async fn get_generation_by_job_step(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    job_run_id: &str,
    generation_step_key: &str,
) -> Result<Option<GenerationCommit>, String> {
    sqlx::query_as::<_, GenerationCommit>(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM engine_generation_commits WHERE tenant_id=$1 AND source_id=$2 \
         AND job_run_id=$3 AND generation_step_key=$4"
    )))
    .bind(tenant_id)
    .bind(source_id)
    .bind(job_run_id)
    .bind(generation_step_key)
    .fetch_optional(db)
    .await
    .map_err(|error| error.to_string())
}

async fn fetch_by_job_step(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: &str,
    source_id: &str,
    job_run_id: &str,
    generation_step_key: &str,
) -> Result<Option<GenerationCommit>, String> {
    sqlx::query_as::<_, GenerationCommit>(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM engine_generation_commits WHERE tenant_id=$1 AND source_id=$2 \
         AND job_run_id=$3 AND generation_step_key=$4 FOR UPDATE"
    )))
    .bind(tenant_id)
    .bind(source_id)
    .bind(job_run_id)
    .bind(generation_step_key)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| error.to_string())
}

fn validate_prepare_input(input: &PrepareGeneration<'_>) -> Result<(), String> {
    for (name, value) in [
        ("tenant_id", input.tenant_id),
        ("source_id", input.source_id),
        ("job_run_id", input.job_run_id),
        ("generation_step_key", input.generation_step_key),
    ] {
        if value.trim().is_empty() {
            return Err(format!("generation {name} is required"));
        }
    }
    if !matches!(
        input.generation_kind,
        "thread_summary"
            | "thread_rollup"
            | "project_memory"
            | "agent_memory"
            | "subject_memory_rollup"
    ) {
        return Err("generation_kind is invalid".to_string());
    }
    validate_digest(input.input_digest)?;
    if input.input_manifest.get("schema_version").is_none()
        || input.input_manifest.get("canonicalizer_revision").is_none()
        || input
            .input_manifest
            .get("digest_algorithm")
            .and_then(Value::as_str)
            != Some("sha256")
        || !input
            .input_manifest
            .get("inputs")
            .is_some_and(Value::is_array)
    {
        return Err("generation input_manifest contract is incomplete".to_string());
    }
    for key in [
        "generation_profile",
        "prompt_checksum",
        "pipeline_revision",
        "model_provider",
        "model_id",
        "tokenizer_id",
        "policy_checksum",
    ] {
        if !input
            .config_snapshot
            .get(key)
            .and_then(Value::as_str)
            .is_some_and(|value| !value.trim().is_empty())
        {
            return Err(format!("generation config_snapshot.{key} is required"));
        }
    }
    if let Some(partition) = input.project_partition_key {
        if partition != "__none__" && !partition.starts_with("project:") {
            return Err("generation project_partition_key is invalid".to_string());
        }
    }
    Ok(())
}

fn validate_digest(value: &str) -> Result<(), String> {
    let Some(hex) = value.strip_prefix("sha256:") else {
        return Err("generation input_digest must use sha256 prefix".to_string());
    };
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|value| value.is_ascii_digit() || (b'a'..=b'f').contains(&value))
    {
        return Err("generation input_digest must be 64 lowercase hex characters".to_string());
    }
    Ok(())
}

fn required_lease_token(generation: &GenerationCommit) -> Result<&str, String> {
    generation
        .lease_token
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "generation lease_token is required for CAS update".to_string())
}
