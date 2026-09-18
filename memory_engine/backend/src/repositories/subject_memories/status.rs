// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use sqlx::types::Json;

use crate::db::Db;
use crate::models::{now_rfc3339, EngineSubjectMemory};
use crate::repositories::postgres::{decode, json, timestamp};

pub async fn mark_subject_memories_rolled_up(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    subject_id: &str,
    memory_ids: &[String],
    rollup_memory_key: &str,
) -> Result<usize, String> {
    if memory_ids.is_empty() {
        return Ok(0);
    }
    let mut tx = db.begin().await.map_err(|error| error.to_string())?;
    let rows = sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "SELECT data FROM engine_subject_memories WHERE tenant_id=$1 AND source_id=$2 \
         AND subject_id=$3 AND id = ANY($4) AND rollup_status='pending' AND status='active' \
         FOR UPDATE",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(subject_id)
    .bind(memory_ids)
    .fetch_all(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    let now = now_rfc3339();
    let mut marked = 0usize;
    for row in rows {
        let mut memory: EngineSubjectMemory = decode(row)?;
        memory.rollup_status = "done".to_string();
        memory.rollup_memory_key = Some(rollup_memory_key.to_string());
        memory.rolled_up_at = Some(now.clone());
        memory.updated_at = now.clone();
        marked += sqlx::query(
            "UPDATE engine_subject_memories SET rollup_status='done',updated_at=$2,data=$3 \
             WHERE id=$1 AND rollup_status='pending'",
        )
        .bind(&memory.id)
        .bind(timestamp(&now)?)
        .bind(json(&memory)?)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?
        .rows_affected() as usize;
    }
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(marked)
}
