// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, State};
use axum::Json;
use serde_json::{json, Value};

use crate::local_runtime::project_management::{
    UpdateLocalRequirementInput, UpdateLocalWorkItemInput,
};
use crate::LocalRuntime;

use super::super::super::context::owner_context;
use super::super::super::error::LocalRuntimeApiError;
use super::ExecuteRequirementPayload;

pub(in crate::local_runtime::api::task_runs) async fn stop_requirement(
    Path((project_id, requirement_id)): Path<(String, String)>,
    State(runtime): State<LocalRuntime>,
    Json(_payload): Json<ExecuteRequirementPayload>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let database = runtime.local_database()?;
    let runs = database
        .list_local_requirement_task_runs(
            owner.owner_user_id.as_str(),
            project_id.as_str(),
            requirement_id.as_str(),
        )
        .await?;
    let mut canceled = Vec::new();
    for run in runs
        .iter()
        .filter(|run| matches!(run.status.as_str(), "queued" | "running"))
    {
        if let Some(updated) = database
            .request_local_task_run_cancel(owner.owner_user_id.as_str(), run.id.as_str())
            .await?
        {
            database
                .update_local_work_item(
                    owner.owner_user_id.as_str(),
                    run.task_id.as_str(),
                    UpdateLocalWorkItemInput {
                        status: Some("todo".to_string()),
                        ..Default::default()
                    },
                )
                .await?;
            canceled.push(updated);
        }
    }
    database
        .update_local_requirement(
            owner.owner_user_id.as_str(),
            requirement_id.as_str(),
            UpdateLocalRequirementInput {
                status: Some("approved".to_string()),
                ..Default::default()
            },
        )
        .await?;
    Ok(Json(json!({
        "success": true,
        "project_id": project_id,
        "requirement_id": requirement_id,
        "cancelled_tasks": canceled,
        "skipped_tasks": [],
        "reset_work_item_ids": canceled.iter().map(|run| &run.task_id).collect::<Vec<_>>(),
    })))
}
