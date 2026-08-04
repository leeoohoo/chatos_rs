// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::{
    extract::{Path, Query},
    http::StatusCode,
    Json,
};
#[cfg(test)]
use chatos_project_execution::{
    build_requirement_execution_planner_prompt, build_requirement_execution_user_message,
};
use chatos_project_execution::{
    requirement_execution_recovery_state, ExecutionPlanIdentity, ExecutionPlane, STATUS_STOPPED,
    STATUS_STOPPING,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use tracing::warn;

use crate::core::auth::AuthUser;
use crate::core::messages::set_task_runner_async_overall_status_for_session;
use crate::services::{project_management_api_client, task_runner_api_client};
use crate::utils::abort_registry;

#[cfg(test)]
use super::requirement_execution::WorkItemPlanItem;
use super::requirement_execution::{
    apply_task_runner_task_snapshot, collect_requirement_execution_scope,
    load_execution_links_for_work_items, load_requirement_execution_request_context,
    mark_execution_messages_for_stop, parse_requirements, parse_work_items, project_plan_array,
    project_plan_value, requirement_dependency_map, select_contact_runtime,
    sync_execution_link_status, sync_requirement_execution_state,
    task_runner_callback_event_for_status, task_runner_status_is_active,
    task_runner_status_is_cancelled, task_runner_status_is_success, value_string, ExecutionLink,
    HandlerError,
};

mod execute_planning;
mod execution_dispatch;
mod plan_query;
mod rerun_execution;
mod rerun_support;

pub(in crate::api::projects) use plan_query::repair_stale_cloud_execution_planner_message;
pub(crate) use plan_query::{cloud_execution_planner_message_is_stale, execution_message_status};

use execute_planning::execute_requirement_inner;
#[cfg(test)]
use execute_planning::prepare_requirement_planner_turn;
use execution_dispatch::{
    confirm_requirement_execution_inner, mutate_requirement_execution_dispatch_inner,
};
#[cfg(test)]
use plan_query::{
    cloud_execution_message_is_newer, cloud_execution_message_matches_scope,
    execution_message_is_stopped_terminal, STALE_PLANNER_NO_TASK_TIMEOUT_SECONDS,
};
use plan_query::{get_requirement_execution_plan_inner, load_cloud_execution_source_message};
use rerun_execution::rerun_requirement_execution_inner;
#[cfg(test)]
use rerun_support::{
    build_planner_coverage_failure_message, replacement_link_scope,
    resolve_old_cloud_execution_batch_state, validate_rerun_cloned_project_task_scope,
    OldCloudExecutionBatchState,
};
use rerun_support::{
    expand_project_task_scope_to_actual_graph, expected_execution_project_task_ids,
    load_expected_execution_project_task_ids, reconcile_requirement_planner_outcome,
    RequirementPlannerRecovery,
};

#[derive(Debug, Default, Deserialize)]
pub(super) struct ExecuteRequirementRequest {
    contact_id: Option<String>,
    #[serde(alias = "modelConfigId")]
    model_config_id: Option<String>,
    #[serde(default, alias = "includePrerequisiteDependents")]
    include_prerequisite_dependents: bool,
    #[serde(alias = "planningFeedback")]
    planning_feedback: Option<String>,
    #[serde(alias = "replacesExecutionGroupId")]
    replaces_execution_group_id: Option<String>,
    #[serde(alias = "replacesConversationId")]
    replaces_conversation_id: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct ConfirmRequirementExecutionRequest {
    contact_id: Option<String>,
    #[serde(alias = "executionGroupId")]
    execution_group_id: String,
    #[serde(alias = "conversationId")]
    conversation_id: String,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct MutateRequirementExecutionDispatchRequest {
    contact_id: Option<String>,
    #[serde(alias = "executionGroupId")]
    execution_group_id: String,
    #[serde(alias = "conversationId")]
    conversation_id: String,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct StopRequirementExecutionRequest {
    contact_id: Option<String>,
    #[serde(alias = "executionGroupId")]
    execution_group_id: Option<String>,
    #[serde(alias = "conversationId")]
    conversation_id: Option<String>,
    #[serde(default, alias = "discardTasks")]
    discard_tasks: bool,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct RerunRequirementExecutionRequest {
    contact_id: Option<String>,
    #[serde(alias = "executionGroupId")]
    execution_group_id: String,
    #[serde(alias = "conversationId")]
    conversation_id: String,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct RequirementExecutionPlanQuery {
    #[serde(alias = "executionGroupId")]
    execution_group_id: Option<String>,
    #[serde(alias = "conversationId")]
    conversation_id: Option<String>,
}

pub(super) async fn execute_requirement(
    auth: AuthUser,
    Path((project_id, requirement_id)): Path<(String, String)>,
    Json(req): Json<ExecuteRequirementRequest>,
) -> (StatusCode, Json<Value>) {
    match execute_requirement_inner(auth, project_id, requirement_id, req).await {
        Ok(value) => (StatusCode::CREATED, Json(value)),
        Err(err) => {
            let mut body = json!({ "error": err.error });
            if let Some(detail) = err.detail {
                body["detail"] = Value::String(detail);
            }
            (err.status, Json(body))
        }
    }
}

pub(super) async fn stop_requirement_execution(
    auth: AuthUser,
    Path((project_id, requirement_id)): Path<(String, String)>,
    Json(req): Json<StopRequirementExecutionRequest>,
) -> (StatusCode, Json<Value>) {
    match stop_requirement_execution_inner(auth, project_id, requirement_id, req).await {
        Ok(value) => (StatusCode::OK, Json(value)),
        Err(err) => {
            let mut body = json!({ "error": err.error });
            if let Some(detail) = err.detail {
                body["detail"] = Value::String(detail);
            }
            (err.status, Json(body))
        }
    }
}

pub(super) async fn confirm_requirement_execution(
    auth: AuthUser,
    Path((project_id, requirement_id)): Path<(String, String)>,
    Json(req): Json<ConfirmRequirementExecutionRequest>,
) -> (StatusCode, Json<Value>) {
    match confirm_requirement_execution_inner(auth, project_id, requirement_id, req).await {
        Ok(value) => (StatusCode::OK, Json(value)),
        Err(err) => {
            let mut body = json!({ "error": err.error });
            if let Some(detail) = err.detail {
                body["detail"] = Value::String(detail);
            }
            (err.status, Json(body))
        }
    }
}

pub(super) async fn pause_requirement_execution(
    auth: AuthUser,
    Path((project_id, requirement_id)): Path<(String, String)>,
    Json(req): Json<MutateRequirementExecutionDispatchRequest>,
) -> (StatusCode, Json<Value>) {
    mutate_requirement_execution_dispatch(auth, project_id, requirement_id, req, true).await
}

pub(super) async fn resume_requirement_execution(
    auth: AuthUser,
    Path((project_id, requirement_id)): Path<(String, String)>,
    Json(req): Json<MutateRequirementExecutionDispatchRequest>,
) -> (StatusCode, Json<Value>) {
    mutate_requirement_execution_dispatch(auth, project_id, requirement_id, req, false).await
}

async fn mutate_requirement_execution_dispatch(
    auth: AuthUser,
    project_id: String,
    requirement_id: String,
    req: MutateRequirementExecutionDispatchRequest,
    paused: bool,
) -> (StatusCode, Json<Value>) {
    match mutate_requirement_execution_dispatch_inner(auth, project_id, requirement_id, req, paused)
        .await
    {
        Ok(value) => (StatusCode::OK, Json(value)),
        Err(err) => {
            let mut body = json!({ "error": err.error });
            if let Some(detail) = err.detail {
                body["detail"] = Value::String(detail);
            }
            (err.status, Json(body))
        }
    }
}

pub(super) async fn get_requirement_execution_plan(
    auth: AuthUser,
    Path((project_id, requirement_id)): Path<(String, String)>,
    Query(query): Query<RequirementExecutionPlanQuery>,
) -> (StatusCode, Json<Value>) {
    match get_requirement_execution_plan_inner(auth, project_id, requirement_id, query).await {
        Ok(value) => (StatusCode::OK, Json(value)),
        Err(err) => {
            let mut body = json!({ "error": err.error });
            if let Some(detail) = err.detail {
                body["detail"] = Value::String(detail);
            }
            (err.status, Json(body))
        }
    }
}

pub(super) async fn rerun_requirement_execution(
    auth: AuthUser,
    Path((project_id, requirement_id)): Path<(String, String)>,
    Json(req): Json<RerunRequirementExecutionRequest>,
) -> (StatusCode, Json<Value>) {
    match rerun_requirement_execution_inner(auth, project_id, requirement_id, req).await {
        Ok(value) => (StatusCode::CREATED, Json(value)),
        Err(err) => {
            let mut body = json!({ "error": err.error });
            if let Some(detail) = err.detail {
                body["detail"] = Value::String(detail);
            }
            (err.status, Json(body))
        }
    }
}

async fn retire_cloud_execution_batch(
    task_runner_base_url: &str,
    project_service_base_url: &str,
    access_token: &str,
    project_id: &str,
    requirement_id: &str,
    source_session_id: &str,
    source_user_message_id: &str,
    links: &[ExecutionLink],
) -> Result<Value, HandlerError> {
    let retired = task_runner_api_client::retire_project_execution(
        task_runner_base_url,
        project_id,
        requirement_id,
        source_session_id,
        source_user_message_id,
    )
    .await
    .map_err(|error| HandlerError::bad_gateway("回收旧 Task Runner 执行批次失败", error))?;
    let mut deleted_link_ids = Vec::new();
    let mut link_delete_errors = Vec::new();
    for link in links {
        let Some(link_id) = link.link_id.as_deref() else {
            continue;
        };
        match project_management_api_client::delete_work_item_task_runner_link(
            project_service_base_url,
            access_token,
            link.work_item_id.as_str(),
            link_id,
        )
        .await
        {
            Ok(()) => deleted_link_ids.push(link_id.to_string()),
            Err(error) => {
                warn!(
                    work_item_id = link.work_item_id.as_str(),
                    link_id,
                    error = error.as_str(),
                    "failed to delete retired project execution link"
                );
                link_delete_errors.push(json!({
                    "work_item_id": link.work_item_id,
                    "link_id": link_id,
                    "error": error,
                }));
            }
        }
    }
    Ok(json!({
        "task_runner": retired,
        "deleted_link_ids": deleted_link_ids,
        "link_delete_errors": link_delete_errors,
    }))
}

async fn stop_requirement_execution_inner(
    auth: AuthUser,
    project_id: String,
    requirement_id: String,
    req: StopRequirementExecutionRequest,
) -> Result<Value, HandlerError> {
    let StopRequirementExecutionRequest {
        contact_id,
        execution_group_id,
        conversation_id,
        discard_tasks,
    } = req;
    let precise_plan = precise_cloud_plan_identity(execution_group_id, conversation_id)?;
    if let Some((conversation_id, execution_group_id)) = precise_plan.as_ref() {
        abort_registry::abort_turn(conversation_id.as_str(), Some(execution_group_id.as_str()));
    }
    let context = load_requirement_execution_request_context(&auth, project_id.as_str()).await?;
    let cfg = context.cfg;
    let project = context.project;
    let access_token = context.access_token;
    let project_sync_secret = context.project_sync_secret;
    let plan = context.plan;
    let requirement_items =
        parse_requirements(project_plan_array(&plan, "requirements", "requirements"));
    let Some(root_requirement) = requirement_items
        .iter()
        .find(|item| item.id == requirement_id)
        .cloned()
    else {
        return Err(HandlerError::not_found("需求不存在"));
    };
    let dependency_graph = project_plan_value(&plan, "dependency_graph", "dependencyGraph");
    let requirement_dependency_map = requirement_dependency_map(&dependency_graph);
    let requirement_scope = collect_requirement_execution_scope(
        &requirement_items,
        requirement_id.as_str(),
        &requirement_dependency_map,
        false,
    );
    let all_work_items = parse_work_items(project_plan_array(&plan, "work_items", "workItems"));
    let selected_work_items = all_work_items
        .iter()
        .filter(|item| requirement_scope.contains(item.requirement_id.as_str()))
        .filter(|item| item.status != "archived")
        .cloned()
        .collect::<Vec<_>>();
    let expected_project_task_ids =
        if let Some((conversation_id, execution_group_id)) = precise_plan.as_ref() {
            match load_expected_execution_project_task_ids(
                &auth,
                conversation_id.as_str(),
                execution_group_id.as_str(),
                project.id.as_str(),
                requirement_id.as_str(),
            )
            .await
            {
                Ok(ids) => Some(ids),
                Err(error) if discard_tasks && error.status == StatusCode::NOT_FOUND => None,
                Err(error) => return Err(error),
            }
        } else {
            None
        };
    let work_items_for_stop =
        if let Some(expected_project_task_ids) = expected_project_task_ids.as_ref() {
            all_work_items
                .iter()
                .filter(|item| expected_project_task_ids.contains(item.id.as_str()))
                .cloned()
                .collect::<Vec<_>>()
        } else {
            selected_work_items.clone()
        };
    if work_items_for_stop.is_empty() && precise_plan.is_none() {
        return Err(HandlerError::bad_request("该需求下没有可停止的项目任务"));
    }

    let contact_runtime = select_contact_runtime(
        &auth,
        cfg,
        contact_id,
        project.id.as_str(),
        access_token.as_str(),
    )
    .await?;
    let mut links = load_execution_links_for_work_items(
        cfg.project_service_base_url.as_str(),
        access_token.as_str(),
        &work_items_for_stop,
    )
    .await?;
    if let Some((conversation_id, execution_group_id)) = precise_plan.as_ref() {
        links.retain(|link| {
            link.source_session_id.as_deref() == Some(conversation_id.as_str())
                && link.source_user_message_id.as_deref() == Some(execution_group_id.as_str())
        });
    }
    // Retries reuse the Task Runner task id. Always reconcile the authoritative
    // task snapshot so a cached failed link cannot hide a newly active retry.
    for link in links.iter_mut() {
        let task = task_runner_api_client::get_task_runner_task(
            contact_runtime.task_runner_base_url.as_str(),
            contact_runtime.task_runner_agent_token.as_str(),
            link.task_runner_task_id.as_str(),
        )
        .await
        .map_err(|err| HandlerError::bad_gateway("校验 Task Runner 任务状态失败", err))?;
        apply_task_runner_task_snapshot(link, &task);
        sync_execution_link_status(
            cfg.project_service_base_url.as_str(),
            project_sync_secret.as_str(),
            link,
            task.status.as_str(),
            task_runner_callback_event_for_status(task.status.as_str()),
        )
        .await?;
    }
    let active_links = links
        .iter()
        .filter(|link| task_runner_status_is_active(link.task_runner_status.as_deref()))
        .cloned()
        .collect::<Vec<_>>();
    mark_execution_messages_for_stop(&active_links, STATUS_STOPPING).await;
    if let Some((conversation_id, execution_group_id)) = precise_plan.as_ref() {
        let _ = set_task_runner_async_overall_status_for_session(
            conversation_id.as_str(),
            execution_group_id.as_str(),
            STATUS_STOPPING,
        )
        .await;
    }

    let mut cancelled_tasks = Vec::new();
    let mut skipped_tasks = Vec::new();
    let mut cancel_errors = Vec::new();
    for link in &links {
        if task_runner_status_is_success(link.task_runner_status.as_deref()) {
            skipped_tasks.push(json!({
                "project_task_id": link.work_item_id,
                "task_runner_task_id": link.task_runner_task_id,
                "reason": "succeeded",
            }));
            continue;
        }
        if !task_runner_status_is_active(link.task_runner_status.as_deref())
            && !task_runner_status_is_cancelled(link.task_runner_status.as_deref())
        {
            skipped_tasks.push(json!({
                "project_task_id": link.work_item_id,
                "task_runner_task_id": link.task_runner_task_id,
                "status": link.task_runner_status,
                "reason": "not_active",
            }));
            continue;
        }
        let cancel_result = task_runner_api_client::cancel_task_runner_task(
            contact_runtime.task_runner_base_url.as_str(),
            contact_runtime.task_runner_agent_token.as_str(),
            Some(access_token.as_str()),
            link.task_runner_task_id.as_str(),
            &task_runner_api_client::CancelTaskRunnerTaskRequest {
                reason: format!("用户停止需求执行：{}", root_requirement.title),
                replacement_task_ids: Vec::new(),
            },
        )
        .await;
        match cancel_result {
            Ok(value) => {
                let status = value_string(&value, "status")
                    .or_else(|| {
                        value
                            .get("task")
                            .and_then(|task| value_string(task, "status"))
                    })
                    .unwrap_or_else(|| "cancelled".to_string());
                if let Err(err) = sync_execution_link_status(
                    cfg.project_service_base_url.as_str(),
                    project_sync_secret.as_str(),
                    link,
                    status.as_str(),
                    task_runner_callback_event_for_status(status.as_str())
                        .or(Some("task.cancelled")),
                )
                .await
                {
                    cancel_errors.push(format!("{}: {}", link.task_runner_task_id, err.error));
                    continue;
                }
                cancelled_tasks.push(json!({
                    "project_task_id": link.work_item_id,
                    "task_runner_task_id": link.task_runner_task_id,
                    "task_runner_run_id": link.task_runner_run_id,
                    "task_runner_status": status,
                    "result": value,
                }));
            }
            Err(err) => cancel_errors.push(format!("{}: {}", link.task_runner_task_id, err)),
        }
    }
    if !cancel_errors.is_empty() {
        return Err(HandlerError::bad_gateway(
            "取消 Task Runner 执行任务失败",
            cancel_errors.join("；"),
        ));
    }

    let reset_work_item_ids = if precise_plan.is_some() {
        links
            .iter()
            .map(|link| link.work_item_id.clone())
            .collect::<BTreeSet<_>>()
    } else {
        work_items_for_stop
            .iter()
            .map(|item| item.id.clone())
            .collect::<BTreeSet<_>>()
    };
    let mut work_item_ids_by_requirement = BTreeMap::<String, Vec<String>>::new();
    for item in all_work_items
        .iter()
        .filter(|item| reset_work_item_ids.contains(item.id.as_str()))
    {
        work_item_ids_by_requirement
            .entry(item.requirement_id.clone())
            .or_default()
            .push(item.id.clone());
    }
    let work_item_ids = reset_work_item_ids.into_iter().collect::<Vec<_>>();
    let requirement_status_by_id = requirement_items
        .iter()
        .map(|item| (item.id.as_str(), item.status.as_str()))
        .collect::<BTreeMap<_, _>>();
    let mut reset_requirement_ids = BTreeSet::new();
    if matches!(
        root_requirement.status.as_str(),
        "reviewing" | "in_progress"
    ) {
        reset_requirement_ids.insert(root_requirement.id.clone());
    }
    for item in all_work_items
        .iter()
        .filter(|item| work_item_ids.contains(&item.id))
    {
        if requirement_status_by_id
            .get(item.requirement_id.as_str())
            .is_some_and(|status| matches!(*status, "reviewing" | "in_progress"))
        {
            reset_requirement_ids.insert(item.requirement_id.clone());
        }
    }
    for reset_requirement_id in &reset_requirement_ids {
        let requirement_work_item_ids = work_item_ids_by_requirement
            .remove(reset_requirement_id.as_str())
            .unwrap_or_default();
        sync_requirement_execution_state(
            cfg.project_service_base_url.as_str(),
            project_sync_secret.as_str(),
            reset_requirement_id.as_str(),
            Some("approved"),
            requirement_work_item_ids,
            Some("ready"),
            true,
        )
        .await?;
    }
    let cleanup = if discard_tasks {
        let Some((conversation_id, execution_group_id)) = precise_plan.as_ref() else {
            return Err(HandlerError::bad_request("删除规划任务时必须指定执行批次"));
        };
        Some(
            retire_cloud_execution_batch(
                contact_runtime.task_runner_base_url.as_str(),
                cfg.project_service_base_url.as_str(),
                access_token.as_str(),
                project.id.as_str(),
                requirement_id.as_str(),
                conversation_id.as_str(),
                execution_group_id.as_str(),
                links.as_slice(),
            )
            .await?,
        )
    } else {
        None
    };
    mark_execution_messages_for_stop(&links, STATUS_STOPPED).await;
    if let Some((conversation_id, execution_group_id)) = precise_plan.as_ref() {
        let status_result = set_task_runner_async_overall_status_for_session(
            conversation_id.as_str(),
            execution_group_id.as_str(),
            STATUS_STOPPED,
        )
        .await;
        if let Err(error) = status_result {
            if discard_tasks {
                warn!(
                    conversation_id = conversation_id.as_str(),
                    execution_group_id = execution_group_id.as_str(),
                    error = error.as_str(),
                    "discarded requirement execution batch without a persisted planning message"
                );
            } else {
                return Err(HandlerError::internal("更新执行计划停止状态失败", error));
            }
        }
    }

    let stopped_task_count = links.len();
    let stopped_has_started_runs = links.iter().any(|link| link.task_runner_run_id.is_some());
    let recovery = requirement_execution_recovery_state(
        STATUS_STOPPED,
        stopped_task_count,
        stopped_has_started_runs,
        precise_plan.is_some(),
        discard_tasks,
    );

    Ok(json!({
        "success": true,
        "status": STATUS_STOPPED,
        "execution_plane": ExecutionPlane::Cloud.as_str(),
        "project_id": project.id,
        "requirement_id": requirement_id,
        "contact_id": contact_runtime.contact.contact_id,
        "conversation_id": precise_plan.as_ref().map(|(value, _)| value),
        "execution_group_id": precise_plan.as_ref().map(|(_, value)| value),
        "cancelled_tasks": cancelled_tasks,
        "skipped_tasks": skipped_tasks,
        "reset_work_item_ids": work_item_ids,
        "reset_requirement_ids": reset_requirement_ids,
        "discarded_tasks": discard_tasks,
        "task_count": stopped_task_count,
        "has_started_runs": stopped_has_started_runs,
        "recovery_action": recovery.action,
        "recovery_reason": recovery.reason,
        "replace_previous_batch": recovery.replace_previous_batch,
        "cleanup": cleanup,
    }))
}

fn precise_cloud_plan_identity(
    execution_group_id: Option<String>,
    conversation_id: Option<String>,
) -> Result<Option<(String, String)>, HandlerError> {
    ExecutionPlanIdentity::optional(execution_group_id.as_deref(), conversation_id.as_deref())
        .map(|identity| {
            identity.map(|identity| (identity.conversation_id, identity.execution_group_id))
        })
        .map_err(HandlerError::bad_request)
}

#[cfg(test)]
include!("requirement_execution_handlers.test.rs");
