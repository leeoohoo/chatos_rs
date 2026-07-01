// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use mongodb::bson::{doc, Bson};
use tracing::{info, warn};
use uuid::Uuid;

use crate::db::Db;
use crate::models::{
    now_rfc3339, CreateEngineJobRunRequest, EngineJobRun, FinishEngineJobRunRequest,
};

use super::super::common::job_run_collection;

pub async fn create_job_run(
    db: &Db,
    req: CreateEngineJobRunRequest,
) -> Result<EngineJobRun, String> {
    let started_at = now_rfc3339();
    let job_run = EngineJobRun {
        id: Uuid::new_v4().to_string(),
        job_type: req.job_type,
        trigger_type: req.trigger_type,
        tenant_id: req.tenant_id,
        source_id: req.source_id,
        thread_id: req.thread_id,
        subject_id: req.subject_id,
        thread_label: req.thread_label,
        thread_display_name: None,
        status: "running".to_string(),
        input_count: 0,
        output_count: 0,
        processed_count: 0,
        success_count: 0,
        error_count: 0,
        metadata: req.metadata,
        error_message: None,
        started_at,
        finished_at: None,
    };

    job_run_collection(db)
        .insert_one(job_run.clone())
        .await
        .map_err(|err| err.to_string())?;
    info!(
        "[MEMORY-ENGINE-JOB] created job_run_id={} job_type={} trigger_type={} tenant_id={} source_id={} thread_id={} subject_id={} thread_label={}",
        job_run.id,
        job_run.job_type,
        job_run.trigger_type,
        job_run.tenant_id.as_deref().unwrap_or("-"),
        job_run.source_id.as_deref().unwrap_or("-"),
        job_run.thread_id.as_deref().unwrap_or("-"),
        job_run.subject_id.as_deref().unwrap_or("-"),
        job_run.thread_label.as_deref().unwrap_or("-")
    );
    Ok(job_run)
}

pub async fn finish_job_run(
    db: &Db,
    id: &str,
    req: FinishEngineJobRunRequest,
) -> Result<Option<EngineJobRun>, String> {
    let FinishEngineJobRunRequest {
        status,
        input_count,
        output_count,
        processed_count,
        success_count,
        error_count,
        metadata,
        error_message,
    } = req;
    let finished_at = now_rfc3339();

    let update_result = job_run_collection(db)
        .update_one(
            doc! {"id": id, "status": "running"},
            doc! {
                "$set": {
                    "status": &status,
                    "input_count": input_count,
                    "output_count": output_count,
                    "processed_count": processed_count,
                    "success_count": success_count,
                    "error_count": error_count,
                    "metadata": mongodb::bson::to_bson(&metadata).unwrap_or(Bson::Null),
                    "error_message": mongodb::bson::to_bson(&error_message).unwrap_or(Bson::Null),
                    "finished_at": finished_at,
                }
            },
        )
        .await
        .map_err(|err| err.to_string())?;

    let item = job_run_collection(db)
        .find_one(doc! {"id": id})
        .await
        .map_err(|err| err.to_string())?;

    if update_result.matched_count == 0 {
        warn!(
            "[MEMORY-ENGINE-JOB] finish-skipped job_run_id={} requested_status={} existing_status={} input_count={} output_count={} processed_count={} success_count={} error_count={} error_message={}",
            id,
            status,
            item.as_ref()
                .map(|job| job.status.as_str())
                .unwrap_or("missing"),
            input_count,
            output_count,
            processed_count,
            success_count,
            error_count,
            error_message.as_deref().unwrap_or("-")
        );
    } else {
        info!(
            "[MEMORY-ENGINE-JOB] finished job_run_id={} status={} input_count={} output_count={} processed_count={} success_count={} error_count={} error_message={}",
            id,
            status,
            input_count,
            output_count,
            processed_count,
            success_count,
            error_count,
            error_message.as_deref().unwrap_or("-")
        );
    }

    Ok(item)
}
