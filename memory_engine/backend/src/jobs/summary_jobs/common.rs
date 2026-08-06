// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::db::Db;
use crate::models::{CreateEngineJobRunRequest, FinishEngineJobRunRequest};
use crate::repositories::control_plane;

pub(crate) const SCHEDULER_TRIGGER: &str = "scheduler";

pub(crate) async fn create_scheduler_job_run(
    db: &Db,
    job_type: &str,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    metadata: Option<serde_json::Value>,
) -> Result<crate::models::EngineJobRun, String> {
    control_plane::create_job_run(
        db,
        CreateEngineJobRunRequest {
            job_type: job_type.to_string(),
            trigger_type: SCHEDULER_TRIGGER.to_string(),
            tenant_id: tenant_id.map(ToOwned::to_owned),
            source_id: source_id.map(ToOwned::to_owned),
            thread_id: None,
            subject_id: None,
            thread_label: None,
            metadata,
        },
    )
    .await
}

pub(crate) async fn finish_job_run(db: &Db, job_run_id: &str, req: FinishEngineJobRunRequest) {
    let _ = control_plane::finish_job_run(db, job_run_id, req).await;
}
