use mongodb::bson::{Bson, doc};

use crate::db::Db;
use crate::models::{EngineSummary, UpsertThreadSummaryRequest, now_rfc3339};

use super::common::{new_summary, summary_collection};

pub async fn delete_thread_summary(
    db: &Db,
    thread_id: &str,
    summary_id: &str,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
) -> Result<usize, String> {
    let normalized_tenant_id = tenant_id.map(str::trim).filter(|value| !value.is_empty());
    let normalized_source_id = source_id.map(str::trim).filter(|value| !value.is_empty());
    let reset_count =
        if let (Some(tenant_id), Some(source_id)) = (normalized_tenant_id, normalized_source_id) {
            crate::repositories::records::reset_records_summary_by_summary_id(
                db, tenant_id, source_id, thread_id, summary_id,
            )
            .await?
        } else {
            0
        };
    let mut filter = doc! {"thread_id": thread_id, "id": summary_id};
    if let Some(value) = normalized_tenant_id {
        filter.insert("tenant_id", value);
    }
    if let Some(value) = normalized_source_id {
        filter.insert("source_id", value);
    }
    let result = summary_collection(db)
        .delete_one(filter)
        .await
        .map_err(|err| err.to_string())?;

    if result.deleted_count > 0 || reset_count > 0 {
        Ok(reset_count)
    } else {
        Ok(0)
    }
}

pub async fn create_thread_summary(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    subject_id: &str,
    summary_text: &str,
    source_record_start_id: Option<String>,
    source_record_end_id: Option<String>,
    source_record_count: usize,
) -> Result<EngineSummary, String> {
    create_thread_summary_with_type(
        db,
        tenant_id,
        source_id,
        thread_id,
        subject_id,
        "thread_incremental",
        None,
        summary_text,
        source_record_start_id,
        source_record_end_id,
        source_record_count,
        Some(serde_json::json!({
            "generator": "memory_engine_summary_v1"
        })),
    )
    .await
}

pub async fn upsert_thread_summary(
    db: &Db,
    thread_id: &str,
    summary_id: &str,
    req: UpsertThreadSummaryRequest,
) -> Result<EngineSummary, String> {
    let now = now_rfc3339();
    let created_at = req.created_at.clone().unwrap_or_else(|| now.clone());
    let updated_at = req.updated_at.clone().unwrap_or_else(|| now.clone());
    let status = req.status.clone().unwrap_or_else(|| "done".to_string());
    let rollup_status = req
        .rollup_status
        .clone()
        .unwrap_or_else(|| "pending".to_string());

    let filter = doc! {
        "thread_id": thread_id,
        "id": summary_id,
    };

    summary_collection(db)
        .update_one(
            filter.clone(),
            doc! {
                "$set": {
                    "tenant_id": &req.tenant_id,
                    "source_id": &req.source_id,
                    "thread_id": thread_id,
                    "subject_id": &req.subject_id,
                    "summary_type": &req.summary_type,
                    "level": req.level.unwrap_or(0).max(0),
                    "source_digest": mongodb::bson::to_bson(&req.source_digest).unwrap_or(Bson::Null),
                    "summary_text": &req.summary_text,
                    "source_record_start_id": mongodb::bson::to_bson(&req.source_record_start_id).unwrap_or(Bson::Null),
                    "source_record_end_id": mongodb::bson::to_bson(&req.source_record_end_id).unwrap_or(Bson::Null),
                    "source_record_count": req.source_record_count.unwrap_or(0).max(0),
                    "status": &status,
                    "rollup_status": &rollup_status,
                    "rollup_summary_id": mongodb::bson::to_bson(&req.rollup_summary_id).unwrap_or(Bson::Null),
                    "rolled_up_at": mongodb::bson::to_bson(&req.rolled_up_at).unwrap_or(Bson::Null),
                    "subject_memory_summarized": req.subject_memory_summarized.unwrap_or(0).max(0),
                    "subject_memory_summarized_at": mongodb::bson::to_bson(&req.subject_memory_summarized_at).unwrap_or(Bson::Null),
                    "metadata": mongodb::bson::to_bson(&req.metadata).unwrap_or(Bson::Null),
                    "updated_at": &updated_at,
                },
                "$setOnInsert": {
                    "id": summary_id,
                    "created_at": &created_at,
                }
            },
        )
        .upsert(true)
        .await
        .map_err(|err| err.to_string())?;

    summary_collection(db)
        .find_one(filter)
        .await
        .map_err(|err| err.to_string())?
        .ok_or_else(|| "upserted summary not found".to_string())
}

pub async fn create_thread_summary_with_type(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    subject_id: &str,
    summary_type: &str,
    source_digest: Option<String>,
    summary_text: &str,
    source_record_start_id: Option<String>,
    source_record_end_id: Option<String>,
    source_record_count: usize,
    metadata: Option<serde_json::Value>,
) -> Result<EngineSummary, String> {
    let summary = new_summary(
        tenant_id,
        source_id,
        thread_id,
        subject_id,
        summary_type,
        0,
        source_digest,
        summary_text,
        source_record_start_id,
        source_record_end_id,
        source_record_count,
        metadata,
    );

    summary_collection(db)
        .insert_one(summary.clone())
        .await
        .map_err(|err| err.to_string())?;

    Ok(summary)
}

pub async fn create_rollup_summary(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    subject_id: &str,
    level: i64,
    source_digest: Option<String>,
    summary_text: &str,
    source_record_start_id: Option<String>,
    source_record_end_id: Option<String>,
    source_record_count: usize,
    metadata: Option<serde_json::Value>,
) -> Result<EngineSummary, String> {
    let summary = new_summary(
        tenant_id,
        source_id,
        thread_id,
        subject_id,
        "thread_incremental",
        level,
        source_digest,
        summary_text,
        source_record_start_id,
        source_record_end_id,
        source_record_count,
        metadata,
    );

    summary_collection(db)
        .insert_one(summary.clone())
        .await
        .map_err(|err| err.to_string())?;

    Ok(summary)
}
