// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, State};
use axum::Json;
use chatos_project_execution::{
    ExecutionPlanIdentity, ExecutionPlane, STATUS_EXECUTION_STARTED, STATUS_PAUSED,
};
use serde_json::{json, Value};

use crate::LocalRuntime;

use super::super::super::context::owner_context;
use super::super::super::error::LocalRuntimeApiError;
use super::confirm::load_expected_project_task_ids;
use super::MutateRequirementExecutionDispatchPayload;

pub(in crate::local_runtime::api::task_runs) async fn pause_requirement_execution(
    path: Path<(String, String)>,
    state: State<LocalRuntime>,
    payload: Json<MutateRequirementExecutionDispatchPayload>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    mutate_requirement_execution_dispatch(path, state, payload, true).await
}

pub(in crate::local_runtime::api::task_runs) async fn resume_requirement_execution(
    path: Path<(String, String)>,
    state: State<LocalRuntime>,
    payload: Json<MutateRequirementExecutionDispatchPayload>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    mutate_requirement_execution_dispatch(path, state, payload, false).await
}

async fn mutate_requirement_execution_dispatch(
    Path((project_id, requirement_id)): Path<(String, String)>,
    State(runtime): State<LocalRuntime>,
    Json(payload): Json<MutateRequirementExecutionDispatchPayload>,
    paused: bool,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let identity = ExecutionPlanIdentity::required(
        payload.execution_group_id.as_str(),
        payload.conversation_id.as_str(),
    )
    .map_err(|message| {
        LocalRuntimeApiError::bad_request("local_execution_plan_identity_required", message)
    })?;
    let owner = owner_context(&runtime).await?;
    let database = runtime.local_database()?;
    let project = database
        .get_project(project_id.as_str(), owner.owner_user_id.as_str())
        .await?
        .ok_or_else(|| {
            LocalRuntimeApiError::not_found(
                "local_project_not_found",
                "Local project was not found",
            )
        })?;
    if project.execution_plane != "local_connector" {
        return Err(LocalRuntimeApiError::conflict(
            "local_execution_plane_mismatch",
            "Local requirement execution is only available for local_connector projects",
        ));
    }
    load_expected_project_task_ids(
        database,
        owner.owner_user_id.as_str(),
        identity.execution_group_id.as_str(),
        identity.conversation_id.as_str(),
        project_id.as_str(),
        requirement_id.as_str(),
    )
    .await?;
    let runs = database
        .list_local_execution_group_task_runs(
            owner.owner_user_id.as_str(),
            project_id.as_str(),
            identity.conversation_id.as_str(),
            identity.execution_group_id.as_str(),
        )
        .await?;
    if runs.is_empty() {
        return Err(LocalRuntimeApiError::conflict(
            "local_execution_not_started",
            "The local execution has not started yet",
        ));
    }
    database
        .set_local_execution_group_dispatch_paused(
            owner.owner_user_id.as_str(),
            project_id.as_str(),
            identity.conversation_id.as_str(),
            identity.execution_group_id.as_str(),
            paused,
        )
        .await?;
    database
        .set_turn_task_runner_execution_paused(
            owner.owner_user_id.as_str(),
            identity.execution_group_id.as_str(),
            paused,
        )
        .await?;
    let refreshed = database
        .list_local_execution_group_task_runs(
            owner.owner_user_id.as_str(),
            project_id.as_str(),
            identity.conversation_id.as_str(),
            identity.execution_group_id.as_str(),
        )
        .await?;
    let running_count = refreshed
        .iter()
        .filter(|run| run.status == "running")
        .count();
    let queued_count = refreshed
        .iter()
        .filter(|run| run.status == "queued")
        .count();
    Ok(Json(json!({
        "success": true,
        "status": if paused { STATUS_PAUSED } else { STATUS_EXECUTION_STARTED },
        "execution_paused": paused,
        "pause_scope": "future_dispatch",
        "active_runs_continue": true,
        "execution_plane": ExecutionPlane::LocalConnector.as_str(),
        "project_id": project_id,
        "requirement_id": requirement_id,
        "conversation_id": identity.conversation_id,
        "execution_group_id": identity.execution_group_id,
        "running_count": running_count,
        "queued_count": queued_count,
        "started_runs": [],
    })))
}
