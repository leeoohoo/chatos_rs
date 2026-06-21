use mongodb::bson::doc;

use crate::db::Db;
use crate::models::EngineSummary;

use super::super::common::{build_thread_summary_filter, collect_summaries, summary_collection};

pub async fn list_latest_thread_summaries(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    limit: i64,
) -> Result<Vec<EngineSummary>, String> {
    list_latest_thread_summaries_by_type(
        db,
        tenant_id,
        source_id,
        thread_id,
        "thread_incremental",
        limit,
    )
    .await
}

pub async fn list_latest_thread_summaries_at_level(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    summary_type: &str,
    level: i64,
    limit: i64,
) -> Result<Vec<EngineSummary>, String> {
    let cursor = summary_collection(db)
        .find(doc! {
            "tenant_id": tenant_id,
            "source_id": source_id,
            "thread_id": thread_id,
            "summary_type": summary_type,
            "status": "done",
            "level": level.max(0),
        })
        .sort(doc! {"created_at": -1})
        .limit(limit.max(1))
        .await
        .map_err(|err| err.to_string())?;

    collect_summaries(cursor).await
}

pub async fn list_latest_thread_summaries_by_type(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    summary_type: &str,
    limit: i64,
) -> Result<Vec<EngineSummary>, String> {
    let cursor = summary_collection(db)
        .find(doc! {
            "tenant_id": tenant_id,
            "source_id": source_id,
            "thread_id": thread_id,
            "summary_type": summary_type,
            "status": "done"
        })
        .sort(doc! {"level": -1, "created_at": -1})
        .limit(limit)
        .await
        .map_err(|err| err.to_string())?;

    collect_summaries(cursor).await
}

pub async fn list_thread_summaries(
    db: &Db,
    thread_id: &str,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    summary_type: Option<&str>,
    status: Option<&str>,
    level: Option<i64>,
    limit: i64,
    offset: i64,
) -> Result<Vec<EngineSummary>, String> {
    let cursor = summary_collection(db)
        .find(build_thread_summary_filter(
            thread_id,
            tenant_id,
            source_id,
            summary_type,
            status,
            level,
        ))
        .sort(doc! {"level": -1, "created_at": 1})
        .skip(offset.max(0) as u64)
        .limit(limit.max(1).min(500))
        .await
        .map_err(|err| err.to_string())?;

    collect_summaries(cursor).await
}
