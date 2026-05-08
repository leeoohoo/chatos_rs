use mongodb::bson::doc;
use uuid::Uuid;

use crate::db::Db;
use crate::models::{now_rfc3339, EngineSource, UpsertSourceRequest};

pub async fn upsert_source(
    db: &Db,
    source_id: &str,
    req: UpsertSourceRequest,
) -> Result<EngineSource, String> {
    let now = now_rfc3339();
    let id = format!("src_{}", Uuid::new_v4());
    let status = req.status.unwrap_or_else(|| "active".to_string());

    db.collection::<EngineSource>("engine_sources")
        .update_one(
            doc! {"tenant_id": &req.tenant_id, "source_id": source_id},
            doc! {
                "$set": {
                    "tenant_id": &req.tenant_id,
                    "source_id": source_id,
                    "source_type": &req.source_type,
                    "name": &req.name,
                    "config": mongodb::bson::to_bson(&req.config).unwrap_or(mongodb::bson::Bson::Null),
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

    db.collection::<EngineSource>("engine_sources")
        .find_one(doc! {"tenant_id": &req.tenant_id, "source_id": source_id})
        .await
        .map_err(|err| err.to_string())?
        .ok_or_else(|| "upserted source not found".to_string())
}
