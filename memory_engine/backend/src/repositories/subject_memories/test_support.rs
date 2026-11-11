// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::db::Db;
use crate::models::{EngineSubjectMemory, UpsertSubjectMemoryRequest};
use crate::repositories::postgres::{json, timestamp};

use super::common::build_subject_memory;

pub(crate) async fn insert_test_subject_memory(
    db: &Db,
    subject_id: &str,
    memory_key: &str,
    request: UpsertSubjectMemoryRequest,
) -> Result<EngineSubjectMemory, String> {
    let memory = build_subject_memory(None, subject_id, memory_key, request, None, None);
    let relation_subject_id = memory
        .metadata
        .as_ref()
        .and_then(|value| value.get("relation_subject_id"))
        .and_then(serde_json::Value::as_str);
    sqlx::query(
        "INSERT INTO engine_subject_memories( \
         id,tenant_id,source_id,subject_id,memory_key,memory_type,level,source_digest, \
         relation_subject_id,status,rollup_status,created_at,updated_at,data) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)",
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
