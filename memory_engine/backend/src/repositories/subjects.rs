// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use sqlx::types::Json;
use uuid::Uuid;

use crate::db::Db;
use crate::models::{now_rfc3339, EngineSubject, UpsertSubjectRequest};
use crate::repositories::postgres::{decode, json, timestamp};

pub async fn upsert_subject(
    db: &Db,
    subject_id: &str,
    req: UpsertSubjectRequest,
) -> Result<EngineSubject, String> {
    let existing = sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "SELECT data FROM engine_subjects \
         WHERE tenant_id=$1 AND source_id=$2 AND subject_id=$3",
    )
    .bind(&req.tenant_id)
    .bind(&req.source_id)
    .bind(subject_id)
    .fetch_optional(db)
    .await
    .map_err(|error| error.to_string())?
    .map(decode::<EngineSubject>)
    .transpose()?;
    let now = now_rfc3339();
    let subject = EngineSubject {
        id: existing
            .as_ref()
            .map(|item| item.id.clone())
            .unwrap_or_else(|| format!("sub_{}", Uuid::new_v4())),
        tenant_id: req.tenant_id,
        source_id: req.source_id,
        subject_id: subject_id.to_string(),
        subject_type: req.subject_type,
        display_name: req.display_name,
        attributes: req.attributes,
        status: req.status.unwrap_or_else(|| "active".to_string()),
        created_at: existing
            .map(|item| item.created_at)
            .unwrap_or_else(|| now.clone()),
        updated_at: now,
    };
    sqlx::query(
        "INSERT INTO engine_subjects \
         (id,tenant_id,source_id,subject_id,subject_type,status,created_at,updated_at,data) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) \
         ON CONFLICT(tenant_id,source_id,subject_id) DO UPDATE SET \
         subject_type=EXCLUDED.subject_type,status=EXCLUDED.status,updated_at=EXCLUDED.updated_at, \
         data=EXCLUDED.data",
    )
    .bind(&subject.id)
    .bind(&subject.tenant_id)
    .bind(&subject.source_id)
    .bind(&subject.subject_id)
    .bind(&subject.subject_type)
    .bind(&subject.status)
    .bind(timestamp(&subject.created_at)?)
    .bind(timestamp(&subject.updated_at)?)
    .bind(json(&subject)?)
    .execute(db)
    .await
    .map_err(|error| error.to_string())?;
    Ok(subject)
}
