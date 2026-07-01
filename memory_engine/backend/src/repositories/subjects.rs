// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use mongodb::bson::doc;
use uuid::Uuid;

use crate::db::Db;
use crate::models::{now_rfc3339, EngineSubject, UpsertSubjectRequest};

pub async fn upsert_subject(
    db: &Db,
    subject_id: &str,
    req: UpsertSubjectRequest,
) -> Result<EngineSubject, String> {
    let now = now_rfc3339();
    let id = format!("sub_{}", Uuid::new_v4());
    let status = req.status.unwrap_or_else(|| "active".to_string());

    db.collection::<EngineSubject>("engine_subjects")
        .update_one(
            doc! {
                "tenant_id": &req.tenant_id,
                "source_id": &req.source_id,
                "subject_id": subject_id
            },
            doc! {
                "$set": {
                    "tenant_id": &req.tenant_id,
                    "source_id": &req.source_id,
                    "subject_id": subject_id,
                    "subject_type": &req.subject_type,
                    "display_name": mongodb::bson::to_bson(&req.display_name).unwrap_or(mongodb::bson::Bson::Null),
                    "attributes": mongodb::bson::to_bson(&req.attributes).unwrap_or(mongodb::bson::Bson::Null),
                    "status": &status,
                    "updated_at": &now,
                },
                "$setOnInsert": {
                    "id": id,
                    "created_at": &now,
                }
            },
        )
        .upsert(true)
        .await
        .map_err(|err| err.to_string())?;

    db.collection::<EngineSubject>("engine_subjects")
        .find_one(doc! {
            "tenant_id": &req.tenant_id,
            "source_id": &req.source_id,
            "subject_id": subject_id
        })
        .await
        .map_err(|err| err.to_string())?
        .ok_or_else(|| "upserted subject not found".to_string())
}
