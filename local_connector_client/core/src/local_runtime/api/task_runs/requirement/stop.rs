// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, State};
use axum::Json;
use chatos_project_execution::{
    collect_requirement_execution_scope, requirement_execution_recovery_state,
    ExecutionPlanIdentity, ExecutionPlane, STATUS_STOPPED,
};
use serde_json::{json, Value};
use std::collections::BTreeSet;

use crate::local_runtime::project_management::{
    UpdateLocalRequirementInput, UpdateLocalWorkItemInput,
};
use crate::local_runtime::task_board::LocalTaskBoardTaskRecord;
use crate::local_runtime::task_runner::LocalTaskRunRecord;
use crate::LocalRuntime;

use super::super::super::context::owner_context;
use super::super::super::error::LocalRuntimeApiError;
use super::confirm::load_expected_project_task_ids;
use super::execute::{dependency_map, execution_requirements};
use super::rerun::cleanup_replaced_local_execution_batch;
use super::StopRequirementExecutionPayload;

pub(in crate::local_runtime::api::task_runs) async fn stop_requirement(
    Path((project_id, requirement_id)): Path<(String, String)>,
    State(runtime): State<LocalRuntime>,
    Json(payload): Json<StopRequirementExecutionPayload>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
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
    let precise_plan = precise_plan_identity(
        payload.execution_group_id.as_deref(),
        payload.conversation_id.as_deref(),
    )?;
    if let Some((execution_group_id, conversation_id)) = precise_plan.as_ref() {
        runtime
            .turn_control
            .cancel(conversation_id.as_str(), Some(execution_group_id.as_str()));
        let _ = database
            .request_turn_cancel(
                owner.owner_user_id.as_str(),
                conversation_id.as_str(),
                Some(execution_group_id.as_str()),
            )
            .await?;
        if !payload.discard_tasks {
            load_expected_project_task_ids(
                database,
                owner.owner_user_id.as_str(),
                execution_group_id.as_str(),
                conversation_id.as_str(),
                project_id.as_str(),
                requirement_id.as_str(),
            )
            .await?;
        }
    }

    let snapshot = database
        .local_project_plan(owner.owner_user_id.as_str(), project_id.as_str(), false)
        .await?;
    let requirements = execution_requirements(&snapshot);
    let requirement_scope = collect_requirement_execution_scope(
        requirements.as_slice(),
        requirement_id.as_str(),
        &dependency_map(&snapshot.dependency_graph, "requirement"),
        false,
    );
    let runs = if let Some((execution_group_id, conversation_id)) = precise_plan.as_ref() {
        database
            .list_local_execution_group_task_runs(
                owner.owner_user_id.as_str(),
                project_id.as_str(),
                conversation_id.as_str(),
                execution_group_id.as_str(),
            )
            .await?
    } else {
        let mut scoped_runs = Vec::new();
        for scoped_requirement_id in &requirement_scope {
            scoped_runs.extend(
                database
                    .list_local_requirement_task_runs(
                        owner.owner_user_id.as_str(),
                        project_id.as_str(),
                        scoped_requirement_id.as_str(),
                    )
                    .await?,
            );
        }
        scoped_runs
    };

    let mut canceled = Vec::new();
    let mut cancelled_planned_task_ids = Vec::new();
    let mut reset_work_item_ids = BTreeSet::new();
    let mut reset_requirement_ids = if precise_plan.is_some() {
        BTreeSet::from([requirement_id.clone()])
    } else {
        requirement_scope.clone()
    };
    cancel_active_runs(
        database,
        owner.owner_user_id.as_str(),
        runs.as_slice(),
        &mut canceled,
        &mut reset_work_item_ids,
        &mut reset_requirement_ids,
    )
    .await?;

    let planned_tasks = database
        .list_local_project_execution_tasks(owner.owner_user_id.as_str(), project_id.as_str())
        .await?;
    let belongs_to_current_plan = |task: &LocalTaskBoardTaskRecord| {
        let belongs_to_plan =
            if let Some((execution_group_id, conversation_id)) = precise_plan.as_ref() {
                task.execution_group_id.as_deref() == Some(execution_group_id.as_str())
                    && task.conversation_id == conversation_id.as_str()
            } else {
                task.requirement_id
                    .as_deref()
                    .is_some_and(|id| requirement_scope.contains(id))
            };
        belongs_to_plan
    };
    let planned_task_count = planned_tasks
        .iter()
        .filter(|task| belongs_to_current_plan(task))
        .count();
    let planned_has_started_runs = planned_tasks
        .iter()
        .filter(|task| belongs_to_current_plan(task))
        .any(|task| task.last_run_id.is_some());
    for task in planned_tasks.into_iter().filter(|task| {
        belongs_to_current_plan(task) && matches!(task.status.as_str(), "todo" | "doing")
    }) {
        if let Some(project_work_item_id) = task.project_work_item_id.as_deref() {
            reset_work_item_ids.insert(project_work_item_id.to_string());
        }
        if let Some(task_requirement_id) = task.requirement_id.as_deref() {
            reset_requirement_ids.insert(task_requirement_id.to_string());
        }
        let _ = database
            .set_local_conversation_task_status(
                owner.owner_user_id.as_str(),
                task.conversation_id.as_str(),
                task.id.as_str(),
                "cancelled",
                None,
                Some("Requirement execution stopped by user before confirmation"),
            )
            .await?;
        cancelled_planned_task_ids.push(task.id);
    }
    for work_item_id in &reset_work_item_ids {
        database
            .update_local_work_item(
                owner.owner_user_id.as_str(),
                work_item_id.as_str(),
                UpdateLocalWorkItemInput {
                    status: Some("ready".to_string()),
                    ..Default::default()
                },
            )
            .await?;
    }
    let active_requirement_ids = requirements
        .iter()
        .filter(|requirement| matches!(requirement.status.as_str(), "reviewing" | "in_progress"))
        .map(|requirement| requirement.id.as_str())
        .collect::<BTreeSet<_>>();
    reset_requirement_ids
        .retain(|requirement_id| active_requirement_ids.contains(requirement_id.as_str()));
    for reset_requirement_id in &reset_requirement_ids {
        database
            .update_local_requirement(
                owner.owner_user_id.as_str(),
                reset_requirement_id.as_str(),
                UpdateLocalRequirementInput {
                    status: Some("approved".to_string()),
                    ..Default::default()
                },
            )
            .await?;
    }
    let cleanup = if payload.discard_tasks {
        let Some((execution_group_id, conversation_id)) = precise_plan.as_ref() else {
            return Err(LocalRuntimeApiError::bad_request(
                "local_execution_discard_identity_required",
                "Deleting planned tasks requires an exact execution batch",
            ));
        };
        let identity =
            ExecutionPlanIdentity::required(execution_group_id.as_str(), conversation_id.as_str())
                .map_err(|message| {
                    LocalRuntimeApiError::bad_request(
                        "local_execution_plan_identity_incomplete",
                        message,
                    )
                })?;
        Some(
            cleanup_replaced_local_execution_batch(
                &runtime,
                owner.owner_user_id.as_str(),
                &project,
                &identity,
            )
            .await?,
        )
    } else {
        None
    };
    if let Some((execution_group_id, _)) = precise_plan.as_ref() {
        database
            .set_turn_task_runner_status(
                owner.owner_user_id.as_str(),
                execution_group_id.as_str(),
                STATUS_STOPPED,
                STATUS_STOPPED,
            )
            .await?;
    }
    let recovery = requirement_execution_recovery_state(
        STATUS_STOPPED,
        planned_task_count,
        planned_has_started_runs,
        precise_plan.is_some(),
        payload.discard_tasks,
    );

    Ok(Json(json!({
        "success": true,
        "status": STATUS_STOPPED,
        "execution_plane": ExecutionPlane::LocalConnector.as_str(),
        "project_id": project_id,
        "requirement_id": requirement_id,
        "conversation_id": precise_plan.as_ref().map(|(_, value)| value),
        "execution_group_id": precise_plan.as_ref().map(|(value, _)| value),
        "cancelled_tasks": canceled,
        "cancelled_planned_task_ids": cancelled_planned_task_ids,
        "skipped_tasks": [],
        "reset_work_item_ids": reset_work_item_ids,
        "reset_requirement_ids": reset_requirement_ids,
        "discarded_tasks": payload.discard_tasks,
        "task_count": planned_task_count,
        "has_started_runs": planned_has_started_runs,
        "recovery_action": recovery.action,
        "recovery_reason": recovery.reason,
        "replace_previous_batch": recovery.replace_previous_batch,
        "cleanup": cleanup,
    })))
}

fn precise_plan_identity(
    execution_group_id: Option<&str>,
    conversation_id: Option<&str>,
) -> Result<Option<(String, String)>, LocalRuntimeApiError> {
    ExecutionPlanIdentity::optional(execution_group_id, conversation_id)
        .map(|identity| {
            identity.map(|identity| (identity.execution_group_id, identity.conversation_id))
        })
        .map_err(|message| {
            LocalRuntimeApiError::bad_request("local_execution_plan_identity_incomplete", message)
        })
}

async fn cancel_active_runs(
    database: &crate::local_runtime::LocalDatabase,
    owner_user_id: &str,
    runs: &[LocalTaskRunRecord],
    canceled: &mut Vec<LocalTaskRunRecord>,
    reset_work_item_ids: &mut BTreeSet<String>,
    reset_requirement_ids: &mut BTreeSet<String>,
) -> Result<(), LocalRuntimeApiError> {
    for run in runs
        .iter()
        .filter(|run| matches!(run.status.as_str(), "queued" | "running"))
    {
        let Some(updated) = database
            .request_local_task_run_cancel(owner_user_id, run.id.as_str())
            .await?
        else {
            continue;
        };
        if let Some(run_requirement_id) = run.requirement_id.as_deref() {
            reset_requirement_ids.insert(run_requirement_id.to_string());
        }
        if run.task_kind == "project_work_item" {
            reset_work_item_ids.insert(run.task_id.clone());
        } else if let Some(task) = database
            .get_local_task_board_task(owner_user_id, run.session_id.as_str(), run.task_id.as_str())
            .await?
        {
            if let Some(project_work_item_id) = task.project_work_item_id {
                reset_work_item_ids.insert(project_work_item_id);
            }
            let _ = database
                .set_local_conversation_task_status(
                    owner_user_id,
                    run.session_id.as_str(),
                    run.task_id.as_str(),
                    "cancelled",
                    None,
                    Some("Requirement execution stopped by user"),
                )
                .await?;
        }
        canceled.push(updated);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::precise_plan_identity;

    #[test]
    fn precise_stop_requires_both_plan_identifiers() {
        assert!(precise_plan_identity(None, None)
            .expect("legacy requirement stop remains supported")
            .is_none());
        assert_eq!(
            precise_plan_identity(Some(" group-1 "), Some(" session-1 "))
                .expect("complete plan identity")
                .expect("precise identity"),
            ("group-1".to_string(), "session-1".to_string())
        );
        assert!(precise_plan_identity(Some("group-1"), None).is_err());
        assert!(precise_plan_identity(None, Some("session-1")).is_err());
    }
}
