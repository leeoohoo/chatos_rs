// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use sqlx::types::Json;

use crate::db::Db;
use crate::models::{EngineSubjectMemory, UpsertSubjectMemoryRequest};
use crate::repositories::postgres::{decode, json, timestamp};

use super::common::build_subject_memory;

pub async fn upsert_subject_memory(
    db: &Db,
    subject_id: &str,
    memory_key: &str,
    req: UpsertSubjectMemoryRequest,
) -> Result<EngineSubjectMemory, String> {
    upsert(db, subject_id, memory_key, req, None, None).await
}

pub async fn upsert_generated_subject_memory(
    db: &Db,
    subject_id: &str,
    memory_key: &str,
    req: UpsertSubjectMemoryRequest,
    source_digest: Option<String>,
    rollup_status: &str,
) -> Result<EngineSubjectMemory, String> {
    upsert(
        db,
        subject_id,
        memory_key,
        req,
        Some(source_digest),
        Some(rollup_status),
    )
    .await
}

async fn upsert(
    db: &Db,
    subject_id: &str,
    memory_key: &str,
    req: UpsertSubjectMemoryRequest,
    source_digest: Option<Option<String>>,
    rollup_status: Option<&str>,
) -> Result<EngineSubjectMemory, String> {
    let existing = sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "SELECT data FROM engine_subject_memories WHERE tenant_id=$1 AND source_id=$2 \
         AND subject_id=$3 AND memory_key=$4",
    )
    .bind(&req.tenant_id)
    .bind(&req.source_id)
    .bind(subject_id)
    .bind(memory_key)
    .fetch_optional(db)
    .await
    .map_err(|error| error.to_string())?
    .map(decode)
    .transpose()?;
    let memory = build_subject_memory(
        existing,
        subject_id,
        memory_key,
        req,
        source_digest,
        rollup_status,
    );
    let relation_subject_id = memory
        .metadata
        .as_ref()
        .and_then(|value| value.get("relation_subject_id"))
        .and_then(serde_json::Value::as_str);
    sqlx::query(
        "INSERT INTO engine_subject_memories \
         (id,tenant_id,source_id,subject_id,memory_key,memory_type,level,source_digest, \
          relation_subject_id,status,rollup_status,created_at,updated_at,data) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14) \
         ON CONFLICT(tenant_id,source_id,subject_id,memory_key) DO UPDATE SET \
         memory_type=EXCLUDED.memory_type,level=EXCLUDED.level,source_digest=EXCLUDED.source_digest, \
         relation_subject_id=EXCLUDED.relation_subject_id,status=EXCLUDED.status, \
         rollup_status=EXCLUDED.rollup_status,updated_at=EXCLUDED.updated_at,data=EXCLUDED.data",
    )
    .bind(&memory.id)
    .bind(&memory.tenant_id)
    .bind(&memory.source_id)
    .bind(&memory.subject_id)
    .bind(&memory.memory_key)
    .bind(&memory.memory_type)
    .bind(memory.level)
    .bind(&memory.source_digest)
    .bind(relation_subject_id)
    .bind(&memory.status)
    .bind(&memory.rollup_status)
    .bind(timestamp(&memory.created_at)?)
    .bind(timestamp(&memory.updated_at)?)
    .bind(json(&memory)?)
    .execute(db)
    .await
    .map_err(|error| error.to_string())?;
    Ok(memory)
}
