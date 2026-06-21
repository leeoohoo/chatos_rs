use crate::db::Db;
use crate::models::{EngineSubjectMemory, UpsertSubjectMemoryRequest};

use super::common::{subject_memory_collection, upsert_subject_memory_document};

pub async fn upsert_subject_memory(
    db: &Db,
    subject_id: &str,
    memory_key: &str,
    req: UpsertSubjectMemoryRequest,
) -> Result<EngineSubjectMemory, String> {
    let (filter, update) = upsert_subject_memory_document(subject_id, memory_key, &req, None, None);

    subject_memory_collection(db)
        .update_one(filter.clone(), update)
        .upsert(true)
        .await
        .map_err(|err| err.to_string())?;

    subject_memory_collection(db)
        .find_one(filter)
        .await
        .map_err(|err| err.to_string())?
        .ok_or_else(|| "upserted subject memory not found".to_string())
}

pub async fn upsert_generated_subject_memory(
    db: &Db,
    subject_id: &str,
    memory_key: &str,
    req: UpsertSubjectMemoryRequest,
    source_digest: Option<String>,
    rollup_status: &str,
) -> Result<EngineSubjectMemory, String> {
    let (filter, update) = upsert_subject_memory_document(
        subject_id,
        memory_key,
        &req,
        Some(&source_digest),
        Some(rollup_status),
    );

    subject_memory_collection(db)
        .update_one(filter.clone(), update)
        .upsert(true)
        .await
        .map_err(|err| err.to_string())?;

    subject_memory_collection(db)
        .find_one(filter)
        .await
        .map_err(|err| err.to_string())?
        .ok_or_else(|| "upserted generated subject memory not found".to_string())
}
