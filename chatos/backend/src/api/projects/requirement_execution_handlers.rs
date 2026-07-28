// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::{
    extract::{Path, Query},
    http::StatusCode,
    Json,
};
use chatos_project_execution::{
    append_planning_feedback, build_requirement_execution_planner_prompt,
    build_requirement_execution_user_message, executing_requirement_ids,
    format_planning_feedback_history, missing_project_task_ids, read_planning_feedback_history,
    select_pending_work_items, sort_work_items_for_planning, validate_exact_project_task_scope,
    ExecutionPlanIdentity, ExecutionPlane, NEXT_ACTION_PREVIEW_AND_CONFIRM,
    STATUS_AWAITING_CONFIRMATION, STATUS_EXECUTION_STARTED, STATUS_PAUSED, STATUS_PLANNING,
    STATUS_PLANNING_STARTED, STATUS_STOPPED, STATUS_STOPPING,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use tracing::warn;

use crate::api::chat_stream_common::ChatStreamRequest;
use crate::core::auth::AuthUser;
use crate::core::messages::{
    ensure_message_metadata_object, set_task_runner_async_execution_paused_for_session,
    set_task_runner_async_overall_status_for_session, MessageOut,
};
use crate::core::validation::normalize_non_empty;
use crate::modules::conversation_runtime::chat_usecase::{run_chat_usecase, RunChatUsecaseInput};
use crate::modules::conversation_runtime::messages as conversation_messages;
use crate::services::{
    access_token_scope, chatos_sessions, project_management_api_client, task_runner_api_client,
};
use crate::utils::abort_registry;

use super::requirement_execution::{
    add_requirement_work_item_dependencies, collect_requirement_execution_scope,
    create_execution_message, create_execution_planner_failure_message,
    ensure_requirement_execution_not_active, load_execution_links_for_work_items,
    load_requirement_execution_request_context, mark_execution_messages_for_stop,
    parse_requirements, parse_work_items, project_plan_array, project_plan_value,
    requirement_dependency_map, resolve_or_create_execution_session, select_contact_runtime,
    set_execution_turn_hidden, sync_execution_link_status, sync_execution_message_task_tracking,
    sync_requirement_execution_state, task_runner_callback_event_for_status,
    task_runner_status_is_active, task_runner_status_is_success, topological_work_item_order,
    validate_requirement_prerequisites, value_string, work_item_dependency_map, ExecutionLink,
    HandlerError, WorkItemPlanItem,
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

async fn get_requirement_execution_plan_inner(
    auth: AuthUser,
    project_id: String,
    requirement_id: String,
    query: RequirementExecutionPlanQuery,
) -> Result<Value, HandlerError> {
    let precise_identity = match (query.conversation_id, query.execution_group_id) {
        (Some(conversation_id), Some(execution_group_id)) => Some(
            ExecutionPlanIdentity::required(execution_group_id.as_str(), conversation_id.as_str())
                .map_err(HandlerError::bad_request)?,
        ),
        (None, None) => None,
        _ => {
            return Err(HandlerError::bad_request(
                "读取执行计划时必须同时提供 conversation_id 和 execution_group_id",
            ))
        }
    };
    let context = load_requirement_execution_request_context(&auth, project_id.as_str()).await?;
    let requirement_items = parse_requirements(project_plan_array(
        &context.plan,
        "requirements",
        "requirements",
    ));
    if !requirement_items
        .iter()
        .any(|item| item.id == requirement_id)
    {
        return Err(HandlerError::not_found("需求不存在"));
    }
    let all_work_items =
        parse_work_items(project_plan_array(&context.plan, "work_items", "workItems"));
    let dependency_graph = project_plan_value(&context.plan, "dependency_graph", "dependencyGraph");
    let dependency_map = requirement_dependency_map(&dependency_graph);
    let scope = collect_requirement_execution_scope(
        requirement_items.as_slice(),
        requirement_id.as_str(),
        &dependency_map,
        true,
    );
    let scoped_work_items = all_work_items
        .into_iter()
        .filter(|item| scope.contains(item.requirement_id.as_str()))
        .collect::<Vec<_>>();
    let links = load_execution_links_for_work_items(
        context.cfg.project_service_base_url.as_str(),
        context.access_token.as_str(),
        scoped_work_items.as_slice(),
    )
    .await?;

    if let Some(identity) = precise_identity {
        let message = match load_cloud_execution_source_message(
            &auth,
            identity.conversation_id.as_str(),
            identity.execution_group_id.as_str(),
            context.project.id.as_str(),
            requirement_id.as_str(),
        )
        .await
        {
            Ok(message) => message,
            Err(error) if error.status == StatusCode::NOT_FOUND => {
                return Ok(json!({
                    "found": false,
                    "execution_plane": ExecutionPlane::Cloud.as_str(),
                    "project_id": context.project.id,
                    "requirement_id": requirement_id,
                    "conversation_id": identity.conversation_id,
                    "execution_group_id": identity.execution_group_id,
                }));
            }
            Err(error) => return Err(error),
        };
        let current_links = links
            .iter()
            .filter(|link| {
                link.source_session_id.as_deref() == Some(identity.conversation_id.as_str())
                    && link.source_user_message_id.as_deref()
                        == Some(identity.execution_group_id.as_str())
            })
            .collect::<Vec<_>>();
        let message =
            repair_stale_cloud_execution_planner_message(message, current_links.is_empty()).await?;
        return Ok(build_cloud_execution_plan_response(
            context.project.id.as_str(),
            requirement_id.as_str(),
            message,
            current_links.as_slice(),
        ));
    }

    let Some(message) = find_latest_cloud_execution_source_message(
        &auth,
        context.project.id.as_str(),
        requirement_id.as_str(),
    )
    .await?
    else {
        return Ok(json!({
            "found": false,
            "execution_plane": ExecutionPlane::Cloud.as_str(),
            "project_id": context.project.id,
            "requirement_id": requirement_id,
        }));
    };
    let current_links = links
        .iter()
        .filter(|link| {
            link.source_session_id.as_deref() == Some(message.session_id.as_str())
                && link.source_user_message_id.as_deref() == Some(message.id.as_str())
        })
        .collect::<Vec<_>>();
    let message =
        repair_stale_cloud_execution_planner_message(message, current_links.is_empty()).await?;
    Ok(build_cloud_execution_plan_response(
        context.project.id.as_str(),
        requirement_id.as_str(),
        message,
        current_links.as_slice(),
    ))
}

fn cloud_execution_message_matches_scope(
    message: &crate::models::message::Message,
    project_id: &str,
    requirement_id: &str,
) -> bool {
    if !message.role.trim().eq_ignore_ascii_case("user") {
        return false;
    }
    let execution = message
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("project_requirement_execution"));
    execution
        .and_then(|value| value.get("project_id"))
        .and_then(Value::as_str)
        == Some(project_id)
        && execution
            .and_then(|value| value.get("requirement_id"))
            .and_then(Value::as_str)
            == Some(requirement_id)
}

fn cloud_execution_message_is_newer(
    candidate: &crate::models::message::Message,
    current: &crate::models::message::Message,
) -> bool {
    (candidate.created_at.as_str(), candidate.id.as_str())
        > (current.created_at.as_str(), current.id.as_str())
}

async fn find_latest_cloud_execution_source_message(
    auth: &AuthUser,
    project_id: &str,
    requirement_id: &str,
) -> Result<Option<crate::models::message::Message>, HandlerError> {
    const PAGE_SIZE: i64 = 200;
    let mut latest = None;
    let mut session_offset = 0;
    loop {
        let sessions = chatos_sessions::list_sessions(
            Some(auth.user_id.as_str()),
            Some(project_id),
            Some(PAGE_SIZE),
            session_offset,
            false,
            false,
        )
        .await
        .map_err(|error| HandlerError::internal("读取需求执行会话列表失败", error))?;
        let session_count = sessions.len();
        for session in sessions {
            let mut message_offset = 0;
            loop {
                let messages = chatos_sessions::list_messages_including_hidden(
                    session.id.as_str(),
                    Some(PAGE_SIZE),
                    message_offset,
                    false,
                )
                .await
                .map_err(|error| HandlerError::internal("读取需求执行规划消息失败", error))?;
                let message_count = messages.len();
                for message in messages {
                    if !cloud_execution_message_matches_scope(&message, project_id, requirement_id)
                    {
                        continue;
                    }
                    let replace = latest
                        .as_ref()
                        .is_none_or(|current| cloud_execution_message_is_newer(&message, current));
                    if replace {
                        latest = Some(message);
                    }
                }
                message_offset += message_count as i64;
                if message_count < PAGE_SIZE as usize {
                    break;
                }
            }
        }
        session_offset += session_count as i64;
        if session_count < PAGE_SIZE as usize {
            break;
        }
    }
    Ok(latest)
}

async fn load_cloud_execution_source_message(
    auth: &AuthUser,
    conversation_id: &str,
    execution_group_id: &str,
    project_id: &str,
    requirement_id: &str,
) -> Result<crate::models::message::Message, HandlerError> {
    let session = chatos_sessions::get_session_by_id(conversation_id)
        .await
        .map_err(|err| HandlerError::internal("读取需求执行会话失败", err))?
        .ok_or_else(|| HandlerError::not_found("需求执行会话不存在"))?;
    if session.user_id.as_deref() != Some(auth.user_id.as_str()) {
        return Err(HandlerError::not_found("需求执行会话不存在"));
    }
    let message = conversation_messages::get_message_by_id_in_session_including_hidden(
        &session,
        execution_group_id,
    )
    .await
    .map_err(|err| HandlerError::internal("读取需求执行规划消息失败", err))?
    .ok_or_else(|| HandlerError::not_found("需求执行规划消息不存在"))?;
    expected_execution_project_task_ids(message.metadata.as_ref(), project_id, requirement_id)?;
    Ok(message)
}

fn execution_message_status(message: &crate::models::message::Message) -> String {
    let task_runner = message
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("task_runner_async"));
    value_string(task_runner.unwrap_or(&Value::Null), "overall_status")
        .or_else(|| value_string(task_runner.unwrap_or(&Value::Null), "confirmation_status"))
        .unwrap_or_else(|| STATUS_PLANNING_STARTED.to_string())
        .to_ascii_lowercase()
}

fn build_cloud_execution_plan_response(
    project_id: &str,
    requirement_id: &str,
    message: crate::models::message::Message,
    links: &[&ExecutionLink],
) -> Value {
    let execution = message
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("project_requirement_execution"));
    let task_runner = message
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("task_runner_async"));
    let execution_group_id = value_string(execution.unwrap_or(&Value::Null), "execution_group_id")
        .unwrap_or_else(|| message.id.clone());
    let status = execution_message_status(&message);
    let confirmation_status =
        value_string(task_runner.unwrap_or(&Value::Null), "confirmation_status")
            .unwrap_or_else(|| status.clone());
    let execution_paused = task_runner
        .and_then(|value| value.get("execution_paused"))
        .and_then(Value::as_bool)
        .unwrap_or_else(|| status == STATUS_PAUSED);
    let failure_kind = value_string(task_runner.unwrap_or(&Value::Null), "failure_kind")
        .or_else(|| value_string(execution.unwrap_or(&Value::Null), "failure_kind"));
    let failure_reason = value_string(task_runner.unwrap_or(&Value::Null), "failure_reason")
        .or_else(|| value_string(execution.unwrap_or(&Value::Null), "failure_reason"));
    let failed_at = value_string(task_runner.unwrap_or(&Value::Null), "failed_at")
        .or_else(|| value_string(execution.unwrap_or(&Value::Null), "failed_at"));
    json!({
        "found": true,
        "execution_plane": ExecutionPlane::Cloud.as_str(),
        "project_id": project_id,
        "requirement_id": requirement_id,
        "conversation_id": message.session_id,
        "execution_group_id": execution_group_id,
        "message_id": message.id,
        "contact_id": value_string(execution.unwrap_or(&Value::Null), "contact_id"),
        "model_config_id": value_string(message.metadata.as_ref().unwrap_or(&Value::Null), "model_config_id"),
        "include_prerequisite_dependents": execution
            .and_then(|value| value.get("include_prerequisite_dependents"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
        "planning_feedback": value_string(execution.unwrap_or(&Value::Null), "planning_feedback"),
        "planning_feedback_history": read_planning_feedback_history(execution),
        "status": status,
        "confirmation_status": confirmation_status,
        "execution_paused": execution_paused,
        "task_count": links.len(),
        "has_started_runs": links.iter().any(|link| link.task_runner_run_id.is_some()),
        "failure_kind": failure_kind,
        "failure_reason": failure_reason,
        "failed_at": failed_at,
        "created_at": message.created_at,
        "message": MessageOut::from(message),
    })
}

const STALE_PLANNER_NO_TASK_TIMEOUT_SECONDS: i64 = 10 * 60;
const STALE_PLANNER_FAILURE_KIND: &str = "stale_planning_agent";
const STALE_PLANNER_FAILURE_REASON: &str = "规划 Agent 已中断：后端重启或运行进程丢失后，执行计划长时间没有创建任何 Task Runner 任务。请重新生成执行计划。";

fn is_cloud_execution_planner_status_pending(status: &str) -> bool {
    matches!(
        status.trim().to_ascii_lowercase().as_str(),
        STATUS_PLANNING | STATUS_PLANNING_STARTED | "pending" | "processing"
    )
}

fn cloud_execution_planner_message_is_stale(
    message: &crate::models::message::Message,
    has_execution_links: bool,
    now: chrono::DateTime<chrono::Utc>,
) -> bool {
    if has_execution_links
        || !is_cloud_execution_planner_status_pending(&execution_message_status(message))
    {
        return false;
    }
    let Ok(created_at) = chrono::DateTime::parse_from_rfc3339(message.created_at.as_str()) else {
        return false;
    };
    now.signed_duration_since(created_at.with_timezone(&chrono::Utc))
        .num_seconds()
        >= STALE_PLANNER_NO_TASK_TIMEOUT_SECONDS
}

async fn repair_stale_cloud_execution_planner_message(
    mut message: crate::models::message::Message,
    no_execution_links: bool,
) -> Result<crate::models::message::Message, HandlerError> {
    if !cloud_execution_planner_message_is_stale(&message, !no_execution_links, chrono::Utc::now())
    {
        return Ok(message);
    }
    let failed_at = crate::core::time::now_rfc3339();
    let metadata = ensure_message_metadata_object(&mut message);
    let task_runner_async = metadata
        .entry("task_runner_async".to_string())
        .or_insert_with(|| json!({}));
    if !task_runner_async.is_object() {
        *task_runner_async = json!({});
    }
    if let Some(task_runner_async) = task_runner_async.as_object_mut() {
        task_runner_async.insert("overall_status".to_string(), json!("failed"));
        task_runner_async.insert("confirmation_status".to_string(), json!("failed"));
        task_runner_async.insert(
            "failure_kind".to_string(),
            json!(STALE_PLANNER_FAILURE_KIND),
        );
        task_runner_async.insert(
            "failure_reason".to_string(),
            json!(STALE_PLANNER_FAILURE_REASON),
        );
        task_runner_async.insert("failed_at".to_string(), json!(failed_at.clone()));
    }
    let execution = metadata
        .entry("project_requirement_execution".to_string())
        .or_insert_with(|| json!({}));
    if !execution.is_object() {
        *execution = json!({});
    }
    if let Some(execution) = execution.as_object_mut() {
        execution.insert("status".to_string(), json!("failed"));
        execution.insert(
            "failure_kind".to_string(),
            json!(STALE_PLANNER_FAILURE_KIND),
        );
        execution.insert(
            "failure_reason".to_string(),
            json!(STALE_PLANNER_FAILURE_REASON),
        );
        execution.insert("failed_at".to_string(), json!(failed_at));
    }
    let session = chatos_sessions::get_session_by_id(message.session_id.as_str())
        .await
        .map_err(|error| HandlerError::internal("读取需求执行会话失败", error))?
        .ok_or_else(|| HandlerError::not_found("需求执行会话不存在"))?;
    conversation_messages::upsert_message_in_session(&session, &message)
        .await
        .map_err(|error| HandlerError::internal("收口超时执行规划状态失败", error))
}

async fn execute_requirement_inner(
    auth: AuthUser,
    project_id: String,
    requirement_id: String,
    req: ExecuteRequirementRequest,
) -> Result<Value, HandlerError> {
    let requested_model_config_id = normalize_non_empty(req.model_config_id.clone());
    let planning_feedback = normalize_non_empty(req.planning_feedback.clone());
    let mut replacement_identity = ExecutionPlanIdentity::optional(
        req.replaces_execution_group_id.as_deref(),
        req.replaces_conversation_id.as_deref(),
    )
    .map_err(HandlerError::bad_request)?;
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
    let mut previous_planning_feedback = Vec::new();
    let replacement_project_task_ids = if let Some(identity) = replacement_identity.clone() {
        match load_cloud_execution_source_message(
            &auth,
            identity.conversation_id.as_str(),
            identity.execution_group_id.as_str(),
            project.id.as_str(),
            requirement_id.as_str(),
        )
        .await
        {
            Ok(replaced_message) => {
                if execution_message_status(&replaced_message) != STATUS_STOPPED {
                    return Err(HandlerError::bad_request("重新规划前必须先停止旧执行批次"));
                }
                previous_planning_feedback = read_planning_feedback_history(
                    replaced_message
                        .metadata
                        .as_ref()
                        .and_then(|metadata| metadata.get("project_requirement_execution")),
                );
                Some(expected_execution_project_task_ids(
                    replaced_message.metadata.as_ref(),
                    project.id.as_str(),
                    requirement_id.as_str(),
                )?)
            }
            Err(error) if error.status == StatusCode::NOT_FOUND => {
                replacement_identity = None;
                None
            }
            Err(error) => return Err(error),
        }
    } else {
        None
    };
    let planning_feedback_history = append_planning_feedback(
        previous_planning_feedback.as_slice(),
        planning_feedback.as_deref(),
    );
    let planning_feedback_context =
        format_planning_feedback_history(planning_feedback_history.as_slice());
    let all_work_items = parse_work_items(project_plan_array(&plan, "work_items", "workItems"));
    let replacement_work_items = replacement_project_task_ids
        .as_ref()
        .map(|project_task_ids| {
            all_work_items
                .iter()
                .filter(|item| project_task_ids.contains(item.id.as_str()))
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let dependency_graph = project_plan_value(&plan, "dependency_graph", "dependencyGraph");
    let requirement_dependency_map = requirement_dependency_map(&dependency_graph);
    let requirement_scope = collect_requirement_execution_scope(
        &requirement_items,
        requirement_id.as_str(),
        &requirement_dependency_map,
        req.include_prerequisite_dependents,
    );
    validate_requirement_prerequisites(
        &requirement_items,
        &requirement_scope,
        &requirement_dependency_map,
    )?;
    let mut dependency_map = work_item_dependency_map(&dependency_graph);
    let mut selected_work_items =
        select_pending_work_items(all_work_items.as_slice(), &requirement_scope);
    add_requirement_work_item_dependencies(
        &mut dependency_map,
        &selected_work_items,
        &requirement_dependency_map,
        &requirement_scope,
    );
    let creation_order = topological_work_item_order(&selected_work_items, &dependency_map)?;
    sort_work_items_for_planning(selected_work_items.as_mut_slice());
    if selected_work_items.is_empty() {
        return Err(HandlerError::bad_request(
            "该需求执行范围内没有需要执行的未完成项目任务",
        ));
    }
    let contact_runtime = select_contact_runtime(
        &auth,
        cfg,
        req.contact_id,
        project.id.as_str(),
        access_token.as_str(),
    )
    .await?;
    ensure_requirement_execution_not_active(
        &root_requirement,
        &selected_work_items,
        cfg.project_service_base_url.as_str(),
        project_sync_secret.as_str(),
        access_token.as_str(),
        &contact_runtime,
    )
    .await?;
    let requirement_documents = load_requirement_documents_for_scope(
        cfg.project_service_base_url.as_str(),
        access_token.as_str(),
        &requirement_scope,
    )
    .await?;
    let planner_prompt = build_requirement_execution_planner_prompt(
        ExecutionPlane::Cloud,
        project.id.as_str(),
        &root_requirement,
        &requirement_items,
        &requirement_scope,
        &all_work_items,
        &selected_work_items,
        &creation_order,
        &dependency_map,
        &requirement_documents,
        requested_model_config_id.as_deref(),
        planning_feedback_context.as_deref(),
    )
    .map_err(HandlerError::bad_request)?;
    let mut user_visible_content =
        build_requirement_execution_user_message(&root_requirement, &selected_work_items);
    if let Some(feedback) = planning_feedback_context.as_deref() {
        user_visible_content.push_str("\n\n执行计划调整要求（按提交顺序，全部保留）：\n");
        user_visible_content.push_str(feedback);
    }
    let session = resolve_or_create_execution_session(
        &auth,
        &project,
        &contact_runtime.contact,
        root_requirement.title.as_str(),
        requested_model_config_id.clone(),
    )
    .await?;
    let message = create_execution_message(
        &session,
        project.id.as_str(),
        &root_requirement,
        &contact_runtime.contact,
        &selected_work_items,
        user_visible_content.clone(),
        planning_feedback.as_deref(),
        planning_feedback_history.as_slice(),
        replacement_identity
            .as_ref()
            .map(|identity| identity.execution_group_id.as_str()),
        replacement_identity
            .as_ref()
            .map(|identity| identity.conversation_id.as_str()),
        req.include_prerequisite_dependents,
    )
    .await?;

    let executing_requirement_ids =
        executing_requirement_ids(root_requirement.id.as_str(), selected_work_items.as_slice());
    for executing_requirement_id in &executing_requirement_ids {
        sync_requirement_execution_state(
            cfg.project_service_base_url.as_str(),
            project_sync_secret.as_str(),
            executing_requirement_id.as_str(),
            Some("reviewing"),
            Vec::new(),
            None,
            false,
        )
        .await?;
    }
    let execution_group_id = message.id.clone();
    let chat_req = ChatStreamRequest {
        conversation_id: Some(session.id.clone()),
        content: Some(planner_prompt),
        model_config_id: requested_model_config_id
            .clone()
            .or_else(|| session.selected_model_id.clone()),
        ai_model_config: None,
        user_id: Some(auth.user_id.clone()),
        attachments: None,
        reasoning_enabled: None,
        plan_mode: false,
        turn_id: Some(execution_group_id.clone()),
        contact_agent_id: Some(contact_runtime.contact.agent_id.clone()),
        project_id: Some(project.id.clone()),
        project_root: Some(project.root_path.clone()),
        workspace_root: Some(project.root_path.clone()),
        remote_connection_id: None,
        plugin_device_id: None,
        plugin_workspace_id: None,
        selected_plugin_ids: Vec::new(),
        plugin_command_invocations: Vec::new(),
        plugin_agent_selection: None,
        user_message_id: Some(execution_group_id.clone()),
        project_requirement_execution_planner: true,
        project_requirement_execution_task_ids: selected_work_items
            .iter()
            .map(|item| item.id.clone())
            .collect(),
    };
    let persisted_user_message_metadata = message.metadata.clone();
    let recovery = RequirementPlannerRecovery {
        access_token: access_token.clone(),
        execution_group_id: execution_group_id.clone(),
        executing_requirement_ids,
        link_scope_work_items: all_work_items.clone(),
        project_id: project.id.clone(),
        project_service_base_url: cfg.project_service_base_url.clone(),
        project_sync_secret,
        replacement_identity,
        replacement_work_items,
        requirement_id: requirement_id.clone(),
        selected_work_items: selected_work_items.clone(),
        session_id: session.id.clone(),
        task_runner_base_url: contact_runtime.task_runner_base_url.clone(),
    };
    access_token_scope::spawn_with_current_access_token(async move {
        run_chat_usecase(RunChatUsecaseInput {
            sender: None,
            req: chat_req,
            persisted_user_message_content: Some(user_visible_content),
            persisted_user_message_metadata,
        })
        .await;
        if let Err(err) = reconcile_requirement_planner_outcome(recovery).await {
            warn!(
                error = err.error.as_str(),
                detail = err.detail.as_deref().unwrap_or_default(),
                "failed to reconcile requirement execution planner outcome"
            );
        }
    });

    Ok(json!({
        "success": true,
        "status": STATUS_PLANNING_STARTED,
        "next_action": NEXT_ACTION_PREVIEW_AND_CONFIRM,
        "execution_plane": ExecutionPlane::Cloud.as_str(),
        "project_id": project.id,
        "requirement_id": requirement_id,
        "contact_id": contact_runtime.contact.contact_id,
        "model_config_id": requested_model_config_id
            .or_else(|| session.selected_model_id.clone()),
        "include_prerequisite_dependents": req.include_prerequisite_dependents,
        "planning_feedback": planning_feedback,
        "planning_feedback_history": planning_feedback_history,
        "confirmation_status": STATUS_PLANNING_STARTED,
        "has_started_runs": false,
        "conversation_id": session.id,
        "message_id": execution_group_id.clone(),
        "message": message,
        "execution_group_id": execution_group_id,
        "planner_agent_key": "project_requirement_execution_planner_agent",
        "plan_mode_enabled": false,
    }))
}

async fn load_requirement_documents_for_scope(
    base_url: &str,
    access_token: &str,
    requirement_scope: &BTreeSet<String>,
) -> Result<BTreeMap<String, Value>, HandlerError> {
    let mut out = BTreeMap::new();
    for requirement_id in requirement_scope {
        let documents = project_management_api_client::list_project_service_requirement_documents(
            base_url,
            access_token,
            requirement_id.as_str(),
        )
        .await
        .map_err(|err| HandlerError::bad_gateway("读取需求技术文档失败", err))?;
        out.insert(requirement_id.clone(), documents);
    }
    Ok(out)
}

async fn confirm_requirement_execution_inner(
    auth: AuthUser,
    project_id: String,
    requirement_id: String,
    req: ConfirmRequirementExecutionRequest,
) -> Result<Value, HandlerError> {
    let identity = ExecutionPlanIdentity::required(
        req.execution_group_id.as_str(),
        req.conversation_id.as_str(),
    )
    .map_err(HandlerError::bad_request)?;
    let execution_group_id = identity.execution_group_id;
    let conversation_id = identity.conversation_id;
    let context = load_requirement_execution_request_context(&auth, project_id.as_str()).await?;
    let expected_project_task_ids = load_expected_execution_project_task_ids(
        &auth,
        conversation_id.as_str(),
        execution_group_id.as_str(),
        context.project.id.as_str(),
        requirement_id.as_str(),
    )
    .await?;
    let all_work_items =
        parse_work_items(project_plan_array(&context.plan, "work_items", "workItems"));
    let links = load_execution_links_for_work_items(
        context.cfg.project_service_base_url.as_str(),
        context.access_token.as_str(),
        all_work_items.as_slice(),
    )
    .await?;
    let mut current_links = links
        .into_iter()
        .filter(|link| {
            link.source_session_id.as_deref() == Some(conversation_id.as_str())
                && link.source_user_message_id.as_deref() == Some(execution_group_id.as_str())
        })
        .collect::<Vec<_>>();
    if current_links.is_empty() {
        return Err(HandlerError::bad_request(
            "执行任务图尚未生成完成，请稍后刷新流程图",
        ));
    }
    let linked_project_task_ids = current_links
        .iter()
        .map(|link| link.work_item_id.clone())
        .collect::<BTreeSet<_>>();
    let expected_project_task_ids = expand_project_task_scope_to_actual_graph(
        &expected_project_task_ids,
        &linked_project_task_ids,
    );
    if let Err(mismatch) =
        validate_exact_project_task_scope(&expected_project_task_ids, &linked_project_task_ids)
    {
        return Err(HandlerError::bad_request(format!(
            "执行任务图尚未完整生成；缺少项目任务=[{}]，越界项目任务=[{}]",
            mismatch.missing.join(","),
            mismatch.unexpected.join(",")
        )));
    }
    sync_execution_message_task_tracking(
        conversation_id.as_str(),
        execution_group_id.as_str(),
        current_links.as_slice(),
    )
    .await?;

    let contact_runtime = select_contact_runtime(
        &auth,
        context.cfg,
        req.contact_id,
        context.project.id.as_str(),
        context.access_token.as_str(),
    )
    .await?;
    let confirmation = task_runner_api_client::confirm_project_execution(
        contact_runtime.task_runner_base_url.as_str(),
        context.project.id.as_str(),
        requirement_id.as_str(),
        conversation_id.as_str(),
        execution_group_id.as_str(),
    )
    .await
    .map_err(|err| HandlerError::bad_gateway("确认 Task Runner 执行失败", err))?;
    let started_runs_by_task_id = confirmation
        .get("started_runs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|run| {
            Some((
                value_string(run, "task_id")?,
                (
                    value_string(run, "id")?,
                    value_string(run, "status").unwrap_or_else(|| "queued".to_string()),
                ),
            ))
        })
        .collect::<BTreeMap<_, _>>();
    for link in &mut current_links {
        let Some((run_id, run_status)) =
            started_runs_by_task_id.get(link.task_runner_task_id.as_str())
        else {
            continue;
        };
        link.task_runner_run_id = Some(run_id.clone());
        link.task_runner_status = Some(run_status.clone());
        let callback_event =
            task_runner_callback_event_for_status(run_status.as_str()).or_else(|| match run_status
                .trim()
                .to_ascii_lowercase()
                .as_str()
            {
                "queued" | "ready" | "pending" => Some("task.queued"),
                "running" | "processing" | "in_progress" => Some("task.running"),
                _ => None,
            });
        sync_execution_link_status(
            context.cfg.project_service_base_url.as_str(),
            context.project_sync_secret.as_str(),
            link,
            run_status.as_str(),
            callback_event,
        )
        .await?;
    }

    let linked_work_item_ids = current_links
        .iter()
        .map(|link| link.work_item_id.as_str())
        .collect::<BTreeSet<_>>();
    let active_work_item_ids = current_links
        .iter()
        .filter(|link| task_runner_status_is_active(link.task_runner_status.as_deref()))
        .map(|link| link.work_item_id.as_str())
        .collect::<BTreeSet<_>>();
    let executing_requirement_ids = all_work_items
        .iter()
        .filter(|item| {
            linked_work_item_ids.contains(item.id.as_str())
                && active_work_item_ids.contains(item.id.as_str())
        })
        .map(|item| item.requirement_id.clone())
        .collect::<BTreeSet<_>>();
    for executing_requirement_id in executing_requirement_ids {
        sync_requirement_execution_state(
            context.cfg.project_service_base_url.as_str(),
            context.project_sync_secret.as_str(),
            executing_requirement_id.as_str(),
            Some("in_progress"),
            Vec::new(),
            None,
            false,
        )
        .await?;
    }
    sync_execution_message_task_tracking(
        conversation_id.as_str(),
        execution_group_id.as_str(),
        current_links.as_slice(),
    )
    .await?;
    set_execution_turn_hidden(conversation_id.as_str(), execution_group_id.as_str(), false).await?;

    Ok(json!({
        "success": true,
        "status": value_string(&confirmation, "status").unwrap_or_else(|| STATUS_EXECUTION_STARTED.to_string()),
        "execution_plane": ExecutionPlane::Cloud.as_str(),
        "project_id": context.project.id,
        "requirement_id": requirement_id,
        "conversation_id": conversation_id,
        "execution_group_id": execution_group_id,
        "started_runs": confirmation.get("started_runs").cloned().unwrap_or_else(|| json!([])),
        "root_task_ids": confirmation.get("root_task_ids").cloned().unwrap_or_else(|| json!([])),
    }))
}

async fn mutate_requirement_execution_dispatch_inner(
    auth: AuthUser,
    project_id: String,
    requirement_id: String,
    req: MutateRequirementExecutionDispatchRequest,
    paused: bool,
) -> Result<Value, HandlerError> {
    let identity = ExecutionPlanIdentity::required(
        req.execution_group_id.as_str(),
        req.conversation_id.as_str(),
    )
    .map_err(HandlerError::bad_request)?;
    let context = load_requirement_execution_request_context(&auth, project_id.as_str()).await?;
    load_expected_execution_project_task_ids(
        &auth,
        identity.conversation_id.as_str(),
        identity.execution_group_id.as_str(),
        context.project.id.as_str(),
        requirement_id.as_str(),
    )
    .await?;
    let contact_runtime = select_contact_runtime(
        &auth,
        context.cfg,
        req.contact_id,
        context.project.id.as_str(),
        context.access_token.as_str(),
    )
    .await?;
    let result = if paused {
        task_runner_api_client::pause_project_execution(
            contact_runtime.task_runner_base_url.as_str(),
            context.project.id.as_str(),
            requirement_id.as_str(),
            identity.conversation_id.as_str(),
            identity.execution_group_id.as_str(),
        )
        .await
    } else {
        task_runner_api_client::resume_project_execution(
            contact_runtime.task_runner_base_url.as_str(),
            context.project.id.as_str(),
            requirement_id.as_str(),
            identity.conversation_id.as_str(),
            identity.execution_group_id.as_str(),
        )
        .await
    }
    .map_err(|err| {
        HandlerError::bad_gateway(
            if paused {
                "暂停 Task Runner 执行失败"
            } else {
                "继续 Task Runner 执行失败"
            },
            err,
        )
    })?;
    set_task_runner_async_execution_paused_for_session(
        identity.conversation_id.as_str(),
        identity.execution_group_id.as_str(),
        paused,
    )
    .await
    .map_err(|err| HandlerError::internal("更新需求执行暂停状态失败", err))?;
    Ok(json!({
        "success": true,
        "status": if paused { STATUS_PAUSED } else { STATUS_EXECUTION_STARTED },
        "execution_paused": paused,
        "execution_plane": ExecutionPlane::Cloud.as_str(),
        "project_id": context.project.id,
        "requirement_id": requirement_id,
        "conversation_id": identity.conversation_id,
        "execution_group_id": identity.execution_group_id,
        "running_count": result.get("running_count").cloned().unwrap_or_else(|| json!(0)),
        "queued_count": result.get("queued_count").cloned().unwrap_or_else(|| json!(0)),
        "started_runs": result.get("started_runs").cloned().unwrap_or_else(|| json!([])),
    }))
}

async fn load_expected_execution_project_task_ids(
    auth: &AuthUser,
    conversation_id: &str,
    execution_group_id: &str,
    project_id: &str,
    requirement_id: &str,
) -> Result<BTreeSet<String>, HandlerError> {
    let session = chatos_sessions::get_session_by_id(conversation_id)
        .await
        .map_err(|err| HandlerError::internal("读取需求执行会话失败", err))?
        .ok_or_else(|| HandlerError::not_found("需求执行会话不存在"))?;
    if session.user_id.as_deref() != Some(auth.user_id.as_str()) {
        return Err(HandlerError::not_found("需求执行会话不存在"));
    }
    let message = conversation_messages::get_message_by_id_in_session_including_hidden(
        &session,
        execution_group_id,
    )
    .await
    .map_err(|err| HandlerError::internal("读取需求执行规划消息失败", err))?
    .ok_or_else(|| HandlerError::not_found("需求执行规划消息不存在"))?;
    expected_execution_project_task_ids(message.metadata.as_ref(), project_id, requirement_id)
}

fn expected_execution_project_task_ids(
    metadata: Option<&Value>,
    project_id: &str,
    requirement_id: &str,
) -> Result<BTreeSet<String>, HandlerError> {
    let execution = metadata.and_then(|metadata| metadata.get("project_requirement_execution"));
    let metadata_project_id = execution
        .and_then(|value| value.get("project_id"))
        .and_then(Value::as_str);
    let metadata_requirement_id = execution
        .and_then(|value| value.get("requirement_id"))
        .and_then(Value::as_str);
    if metadata_project_id != Some(project_id) || metadata_requirement_id != Some(requirement_id) {
        return Err(HandlerError::bad_request("执行规划不属于当前项目或需求"));
    }
    let expected = execution
        .and_then(|value| value.get("project_task_ids"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect::<BTreeSet<_>>();
    if expected.is_empty() {
        return Err(HandlerError::bad_request(
            "执行规划缺少项目任务范围，不能确认执行",
        ));
    }
    Ok(expected)
}

fn expand_project_task_scope_to_actual_graph(
    expected: &BTreeSet<String>,
    actual: &BTreeSet<String>,
) -> BTreeSet<String> {
    if !actual.is_empty() && expected.is_subset(actual) {
        actual.clone()
    } else {
        expected.clone()
    }
}

#[derive(Debug)]
struct RequirementPlannerRecovery {
    access_token: String,
    execution_group_id: String,
    executing_requirement_ids: BTreeSet<String>,
    link_scope_work_items: Vec<WorkItemPlanItem>,
    project_id: String,
    project_service_base_url: String,
    project_sync_secret: String,
    replacement_identity: Option<ExecutionPlanIdentity>,
    replacement_work_items: Vec<WorkItemPlanItem>,
    requirement_id: String,
    selected_work_items: Vec<WorkItemPlanItem>,
    session_id: String,
    task_runner_base_url: String,
}

fn replacement_link_scope(
    selected_work_items: &[WorkItemPlanItem],
    replacement_work_items: &[WorkItemPlanItem],
) -> Vec<WorkItemPlanItem> {
    let mut scope = selected_work_items.to_vec();
    let mut ids = scope
        .iter()
        .map(|item| item.id.clone())
        .collect::<BTreeSet<_>>();
    for item in replacement_work_items {
        if ids.insert(item.id.clone()) {
            scope.push(item.clone());
        }
    }
    scope
}

async fn reconcile_requirement_planner_outcome(
    recovery: RequirementPlannerRecovery,
) -> Result<(), HandlerError> {
    if let Ok(Some(session)) =
        chatos_sessions::get_session_by_id(recovery.session_id.as_str()).await
    {
        if let Ok(Some(message)) =
            conversation_messages::get_message_by_id_in_session_including_hidden(
                &session,
                recovery.execution_group_id.as_str(),
            )
            .await
        {
            if execution_message_status(&message) == STATUS_STOPPED {
                return Ok(());
            }
        }
    }
    let link_scope_work_items = replacement_link_scope(
        recovery.link_scope_work_items.as_slice(),
        recovery.replacement_work_items.as_slice(),
    );
    let links = load_execution_links_for_work_items(
        recovery.project_service_base_url.as_str(),
        recovery.access_token.as_str(),
        link_scope_work_items.as_slice(),
    )
    .await?;
    let current_execution_links = links
        .iter()
        .filter(|link| {
            link.source_user_message_id.as_deref() == Some(recovery.execution_group_id.as_str())
        })
        .cloned()
        .collect::<Vec<_>>();
    let linked_project_task_ids = current_execution_links
        .iter()
        .map(|link| link.work_item_id.clone())
        .collect::<BTreeSet<_>>();
    let missing_project_task_ids = missing_project_task_ids(
        recovery.selected_work_items.as_slice(),
        &linked_project_task_ids,
    );
    if missing_project_task_ids.is_empty() {
        sync_execution_message_task_tracking(
            recovery.session_id.as_str(),
            recovery.execution_group_id.as_str(),
            current_execution_links.as_slice(),
        )
        .await?;
        if let Some(identity) = recovery.replacement_identity.as_ref() {
            let replaced_links = links
                .iter()
                .filter(|link| {
                    link.source_session_id.as_deref() == Some(identity.conversation_id.as_str())
                        && link.source_user_message_id.as_deref()
                            == Some(identity.execution_group_id.as_str())
                })
                .cloned()
                .collect::<Vec<_>>();
            if let Err(error) = retire_cloud_execution_batch(
                recovery.task_runner_base_url.as_str(),
                recovery.project_service_base_url.as_str(),
                recovery.access_token.as_str(),
                recovery.project_id.as_str(),
                recovery.requirement_id.as_str(),
                identity.conversation_id.as_str(),
                identity.execution_group_id.as_str(),
                replaced_links.as_slice(),
            )
            .await
            {
                let _ = retire_cloud_execution_batch(
                    recovery.task_runner_base_url.as_str(),
                    recovery.project_service_base_url.as_str(),
                    recovery.access_token.as_str(),
                    recovery.project_id.as_str(),
                    recovery.requirement_id.as_str(),
                    recovery.session_id.as_str(),
                    recovery.execution_group_id.as_str(),
                    current_execution_links.as_slice(),
                )
                .await;
                let _ = set_task_runner_async_overall_status_for_session(
                    recovery.session_id.as_str(),
                    recovery.execution_group_id.as_str(),
                    "failed",
                )
                .await;
                create_execution_planner_failure_message(
                    recovery.session_id.as_str(),
                    recovery.execution_group_id.as_str(),
                    format!(
                        "新的执行流程已经生成，但旧批次任务及临时资源清理失败，因此新流程没有切换为可执行状态。请检查 Task Runner、沙箱和 Git 分支清理状态后重试。详情：{}",
                        error.error
                    ),
                )
                .await?;
                return Err(error);
            }
        }
        return Ok(());
    }

    if !current_execution_links.is_empty() {
        sync_execution_message_task_tracking(
            recovery.session_id.as_str(),
            recovery.execution_group_id.as_str(),
            current_execution_links.as_slice(),
        )
        .await?;
    }

    let mut work_item_ids_by_requirement = BTreeMap::<String, Vec<String>>::new();
    for item in &recovery.selected_work_items {
        work_item_ids_by_requirement
            .entry(item.requirement_id.clone())
            .or_default()
            .push(item.id.clone());
    }
    for requirement_id in &recovery.executing_requirement_ids {
        let selected_ids = work_item_ids_by_requirement
            .remove(requirement_id.as_str())
            .unwrap_or_default();
        let missing_ids = selected_ids
            .iter()
            .filter(|work_item_id| !linked_project_task_ids.contains(work_item_id.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        sync_requirement_execution_state(
            recovery.project_service_base_url.as_str(),
            recovery.project_sync_secret.as_str(),
            requirement_id.as_str(),
            Some("approved"),
            missing_ids,
            Some("ready"),
            true,
        )
        .await?;
    }
    let _ = set_task_runner_async_overall_status_for_session(
        recovery.session_id.as_str(),
        recovery.execution_group_id.as_str(),
        "failed",
    )
    .await;
    create_execution_planner_failure_message(
        recovery.session_id.as_str(),
        recovery.execution_group_id.as_str(),
        build_planner_coverage_failure_message(
            recovery.selected_work_items.as_slice(),
            &linked_project_task_ids,
        ),
    )
    .await?;
    Ok(())
}

fn build_planner_coverage_failure_message(
    selected_work_items: &[WorkItemPlanItem],
    linked_project_task_ids: &BTreeSet<String>,
) -> String {
    let missing = selected_work_items
        .iter()
        .filter(|item| !linked_project_task_ids.contains(item.id.as_str()))
        .collect::<Vec<_>>();
    let visible_titles = missing
        .iter()
        .take(5)
        .map(|item| format!("「{}」", item.title))
        .collect::<Vec<_>>()
        .join("、");
    let suffix = if missing.len() > 5 {
        format!("等 {} 个项目任务", missing.len())
    } else {
        format!("{} 个项目任务", missing.len())
    };
    format!(
        "需求执行规划未完整覆盖当前执行范围：缺少 {visible_titles}{suffix}。系统已把未创建执行任务的项目任务恢复为就绪状态；已经生成的部分任务不会自动运行。请先停止当前计划后重试，避免遗漏任务。"
    )
}

async fn rerun_requirement_execution_inner(
    auth: AuthUser,
    project_id: String,
    requirement_id: String,
    req: RerunRequirementExecutionRequest,
) -> Result<Value, HandlerError> {
    let identity = ExecutionPlanIdentity::required(
        req.execution_group_id.as_str(),
        req.conversation_id.as_str(),
    )
    .map_err(HandlerError::bad_request)?;
    let context = load_requirement_execution_request_context(&auth, project_id.as_str()).await?;
    let requirement_items = parse_requirements(project_plan_array(
        &context.plan,
        "requirements",
        "requirements",
    ));
    let root_requirement = requirement_items
        .iter()
        .find(|item| item.id == requirement_id)
        .cloned()
        .ok_or_else(|| HandlerError::not_found("需求不存在"))?;
    let old_message = load_cloud_execution_source_message(
        &auth,
        identity.conversation_id.as_str(),
        identity.execution_group_id.as_str(),
        context.project.id.as_str(),
        requirement_id.as_str(),
    )
    .await?;
    if execution_message_status(&old_message) != STATUS_STOPPED {
        return Err(HandlerError::bad_request(
            "只有已经停止的执行批次才能重新执行",
        ));
    }
    let mut expected_project_task_ids = expected_execution_project_task_ids(
        old_message.metadata.as_ref(),
        context.project.id.as_str(),
        requirement_id.as_str(),
    )?;
    let all_work_items =
        parse_work_items(project_plan_array(&context.plan, "work_items", "workItems"));
    let mut selected_work_items = all_work_items
        .iter()
        .filter(|item| expected_project_task_ids.contains(item.id.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    if selected_work_items.len() != expected_project_task_ids.len() {
        return Err(HandlerError::bad_request(
            "原执行批次包含已经删除或不可见的项目任务，不能直接重新执行",
        ));
    }
    let contact_runtime = select_contact_runtime(
        &auth,
        context.cfg,
        req.contact_id,
        context.project.id.as_str(),
        context.access_token.as_str(),
    )
    .await?;
    let session = chatos_sessions::get_session_by_id(identity.conversation_id.as_str())
        .await
        .map_err(|error| HandlerError::internal("读取需求执行会话失败", error))?
        .ok_or_else(|| HandlerError::not_found("需求执行会话不存在"))?;
    if session.user_id.as_deref() != Some(auth.user_id.as_str()) {
        return Err(HandlerError::not_found("需求执行会话不存在"));
    }
    let old_execution = old_message
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("project_requirement_execution"));
    let content = value_string(
        old_execution.unwrap_or(&Value::Null),
        "user_visible_content",
    )
    .filter(|value| !value.trim().is_empty())
    .unwrap_or_else(|| old_message.content.clone());
    let planning_feedback =
        value_string(old_execution.unwrap_or(&Value::Null), "planning_feedback");
    let planning_feedback_history = read_planning_feedback_history(old_execution);
    let include_prerequisite_dependents = old_execution
        .and_then(|value| value.get("include_prerequisite_dependents"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let new_message = create_execution_message(
        &session,
        context.project.id.as_str(),
        &root_requirement,
        &contact_runtime.contact,
        selected_work_items.as_slice(),
        content,
        planning_feedback.as_deref(),
        planning_feedback_history.as_slice(),
        Some(identity.execution_group_id.as_str()),
        Some(identity.conversation_id.as_str()),
        include_prerequisite_dependents,
    )
    .await?;
    let new_execution_group_id = new_message.id.clone();
    let clone_result = task_runner_api_client::clone_project_execution(
        contact_runtime.task_runner_base_url.as_str(),
        context.project.id.as_str(),
        requirement_id.as_str(),
        identity.conversation_id.as_str(),
        identity.execution_group_id.as_str(),
        session.id.as_str(),
        new_execution_group_id.as_str(),
    )
    .await
    .map_err(|error| HandlerError::bad_gateway("复制 Task Runner 执行图失败", error))?;
    let mappings = clone_result
        .get("task_mappings")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let parsed_mappings = mappings
        .iter()
        .map(|mapping| {
            let project_task_id = value_string(mapping, "project_task_id")
                .ok_or_else(|| format!("复制执行图缺少项目任务映射: {mapping}"))?;
            let task_runner_task_id = value_string(mapping, "new_task_id")
                .ok_or_else(|| format!("复制执行图缺少新任务标识: {mapping}"))?;
            Ok((project_task_id, task_runner_task_id))
        })
        .collect::<Result<Vec<_>, String>>();
    let parsed_mappings = match parsed_mappings {
        Ok(parsed) => parsed,
        Err(detail) => {
            let _ = task_runner_api_client::retire_project_execution(
                contact_runtime.task_runner_base_url.as_str(),
                context.project.id.as_str(),
                requirement_id.as_str(),
                session.id.as_str(),
                new_execution_group_id.as_str(),
            )
            .await;
            return Err(HandlerError::bad_gateway("复制执行图无效", detail));
        }
    };
    let mapped_project_task_ids = parsed_mappings
        .iter()
        .map(|(project_task_id, _)| project_task_id.clone())
        .collect::<BTreeSet<_>>();
    let expanded_project_task_ids = expand_project_task_scope_to_actual_graph(
        &expected_project_task_ids,
        &mapped_project_task_ids,
    );
    if expanded_project_task_ids != expected_project_task_ids {
        let expanded_work_items = all_work_items
            .iter()
            .filter(|item| expanded_project_task_ids.contains(item.id.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        if expanded_work_items.len() != expanded_project_task_ids.len() {
            let _ = task_runner_api_client::retire_project_execution(
                contact_runtime.task_runner_base_url.as_str(),
                context.project.id.as_str(),
                requirement_id.as_str(),
                session.id.as_str(),
                new_execution_group_id.as_str(),
            )
            .await;
            return Err(HandlerError::bad_gateway(
                "复制执行图无效",
                "Task Runner 返回了当前项目计划中不存在或不可见的项目任务",
            ));
        }
        expected_project_task_ids = expanded_project_task_ids;
        selected_work_items = expanded_work_items;
    }
    if mappings.len() != expected_project_task_ids.len()
        || validate_exact_project_task_scope(&expected_project_task_ids, &mapped_project_task_ids)
            .is_err()
    {
        let _ = task_runner_api_client::retire_project_execution(
            contact_runtime.task_runner_base_url.as_str(),
            context.project.id.as_str(),
            requirement_id.as_str(),
            session.id.as_str(),
            new_execution_group_id.as_str(),
        )
        .await;
        return Err(HandlerError::bad_gateway(
            "复制执行图不完整",
            format!(
                "expected {} tasks, cloned {}",
                expected_project_task_ids.len(),
                mappings.len()
            ),
        ));
    }
    let old_links = match load_execution_links_for_work_items(
        context.cfg.project_service_base_url.as_str(),
        context.access_token.as_str(),
        selected_work_items.as_slice(),
    )
    .await
    {
        Ok(links) => links
            .into_iter()
            .filter(|link| {
                link.source_session_id.as_deref() == Some(identity.conversation_id.as_str())
                    && link.source_user_message_id.as_deref()
                        == Some(identity.execution_group_id.as_str())
            })
            .collect::<Vec<_>>(),
        Err(error) => {
            let _ = task_runner_api_client::retire_project_execution(
                contact_runtime.task_runner_base_url.as_str(),
                context.project.id.as_str(),
                requirement_id.as_str(),
                session.id.as_str(),
                new_execution_group_id.as_str(),
            )
            .await;
            return Err(error);
        }
    };
    let mut new_links = Vec::new();
    for (project_task_id, task_runner_task_id) in parsed_mappings {
        let sync_result = project_management_api_client::sync_work_item_task_runner_status(
            context.cfg.project_service_base_url.as_str(),
            context.project_sync_secret.as_str(),
            project_task_id.as_str(),
            &project_management_api_client::SyncTaskRunnerWorkItemStatusRequest {
                task_runner_task_id: task_runner_task_id.clone(),
                task_runner_run_id: None,
                task_runner_status: Some("ready".to_string()),
                execution_group_id: Some(new_execution_group_id.clone()),
                last_callback_event: Some("task.planned".to_string()),
                last_callback_at: Some(crate::core::time::now_rfc3339()),
                last_error_message: None,
                source_session_id: Some(session.id.clone()),
                source_user_message_id: Some(new_execution_group_id.clone()),
            },
        )
        .await;
        let sync_result = match sync_result {
            Ok(value) => value,
            Err(error) => {
                let _ = retire_cloud_execution_batch(
                    contact_runtime.task_runner_base_url.as_str(),
                    context.cfg.project_service_base_url.as_str(),
                    context.access_token.as_str(),
                    context.project.id.as_str(),
                    requirement_id.as_str(),
                    session.id.as_str(),
                    new_execution_group_id.as_str(),
                    new_links.as_slice(),
                )
                .await;
                let _ = set_task_runner_async_overall_status_for_session(
                    session.id.as_str(),
                    new_execution_group_id.as_str(),
                    "failed",
                )
                .await;
                return Err(HandlerError::bad_gateway("关联重新执行任务失败", error));
            }
        };
        new_links.push(ExecutionLink {
            link_id: sync_result
                .get("link")
                .and_then(|link| value_string(link, "id")),
            work_item_id: project_task_id,
            task_runner_task_id,
            task_runner_run_id: None,
            task_runner_status: Some("ready".to_string()),
            source_session_id: Some(session.id.clone()),
            source_user_message_id: Some(new_execution_group_id.clone()),
        });
    }
    if let Err(error) = sync_execution_message_task_tracking(
        session.id.as_str(),
        new_execution_group_id.as_str(),
        new_links.as_slice(),
    )
    .await
    {
        let _ = retire_cloud_execution_batch(
            contact_runtime.task_runner_base_url.as_str(),
            context.cfg.project_service_base_url.as_str(),
            context.access_token.as_str(),
            context.project.id.as_str(),
            requirement_id.as_str(),
            session.id.as_str(),
            new_execution_group_id.as_str(),
            new_links.as_slice(),
        )
        .await;
        let _ = set_task_runner_async_overall_status_for_session(
            session.id.as_str(),
            new_execution_group_id.as_str(),
            "failed",
        )
        .await;
        return Err(error);
    }
    let cleanup = match retire_cloud_execution_batch(
        contact_runtime.task_runner_base_url.as_str(),
        context.cfg.project_service_base_url.as_str(),
        context.access_token.as_str(),
        context.project.id.as_str(),
        requirement_id.as_str(),
        identity.conversation_id.as_str(),
        identity.execution_group_id.as_str(),
        old_links.as_slice(),
    )
    .await
    {
        Ok(cleanup) => cleanup,
        Err(error) => {
            let _ = retire_cloud_execution_batch(
                contact_runtime.task_runner_base_url.as_str(),
                context.cfg.project_service_base_url.as_str(),
                context.access_token.as_str(),
                context.project.id.as_str(),
                requirement_id.as_str(),
                session.id.as_str(),
                new_execution_group_id.as_str(),
                new_links.as_slice(),
            )
            .await;
            let _ = set_task_runner_async_overall_status_for_session(
                session.id.as_str(),
                new_execution_group_id.as_str(),
                "failed",
            )
            .await;
            return Err(error);
        }
    };

    let confirmation = task_runner_api_client::confirm_project_execution(
        contact_runtime.task_runner_base_url.as_str(),
        context.project.id.as_str(),
        requirement_id.as_str(),
        session.id.as_str(),
        new_execution_group_id.as_str(),
    )
    .await;
    let mut start_error = None;
    let mut started_runs = json!([]);
    let mut confirmation_status = STATUS_AWAITING_CONFIRMATION.to_string();
    let mut has_started_runs = false;
    let mut post_start_warnings = Vec::new();
    if let Ok(value) = confirmation.as_ref() {
        started_runs = value
            .get("started_runs")
            .cloned()
            .unwrap_or_else(|| json!([]));
        confirmation_status =
            value_string(value, "status").unwrap_or_else(|| STATUS_EXECUTION_STARTED.to_string());
        let started_runs_by_task_id = started_runs
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|run| {
                Some((
                    value_string(run, "task_id")?,
                    (
                        value_string(run, "id")?,
                        value_string(run, "status").unwrap_or_else(|| "queued".to_string()),
                    ),
                ))
            })
            .collect::<BTreeMap<_, _>>();
        for link in &mut new_links {
            let Some((run_id, run_status)) =
                started_runs_by_task_id.get(link.task_runner_task_id.as_str())
            else {
                continue;
            };
            link.task_runner_run_id = Some(run_id.clone());
            link.task_runner_status = Some(run_status.clone());
            if let Err(error) = sync_execution_link_status(
                context.cfg.project_service_base_url.as_str(),
                context.project_sync_secret.as_str(),
                link,
                run_status.as_str(),
                task_runner_callback_event_for_status(run_status.as_str()).or(Some("task.queued")),
            )
            .await
            {
                warn!(
                    task_runner_task_id = link.task_runner_task_id.as_str(),
                    error = error.error.as_str(),
                    "replacement execution started but link status synchronization failed"
                );
                post_start_warnings.push(error.error);
            }
        }
        has_started_runs = !started_runs_by_task_id.is_empty();
    } else if let Err(error) = confirmation {
        start_error = Some(error);
    }

    if has_started_runs {
        let work_item_ids_by_requirement = selected_work_items.iter().fold(
            BTreeMap::<String, Vec<String>>::new(),
            |mut grouped, item| {
                grouped
                    .entry(item.requirement_id.clone())
                    .or_default()
                    .push(item.id.clone());
                grouped
            },
        );
        for (executing_requirement_id, work_item_ids) in work_item_ids_by_requirement {
            if let Err(error) = sync_requirement_execution_state(
                context.cfg.project_service_base_url.as_str(),
                context.project_sync_secret.as_str(),
                executing_requirement_id.as_str(),
                Some("in_progress"),
                work_item_ids,
                Some("in_progress"),
                false,
            )
            .await
            {
                warn!(
                    requirement_id = executing_requirement_id.as_str(),
                    error = error.error.as_str(),
                    "replacement execution started but requirement status synchronization failed"
                );
                post_start_warnings.push(error.error);
            }
        }
    }
    if let Err(error) = sync_execution_message_task_tracking(
        session.id.as_str(),
        new_execution_group_id.as_str(),
        new_links.as_slice(),
    )
    .await
    {
        warn!(
            execution_group_id = new_execution_group_id.as_str(),
            error = error.error.as_str(),
            "replacement execution task tracking synchronization failed"
        );
        post_start_warnings.push(error.error);
    }
    if let Err(error) =
        set_execution_turn_hidden(session.id.as_str(), new_execution_group_id.as_str(), false).await
    {
        warn!(
            execution_group_id = new_execution_group_id.as_str(),
            error = error.error.as_str(),
            "replacement execution message reveal failed"
        );
        post_start_warnings.push(error.error);
    }

    Ok(json!({
        "success": true,
        "status": confirmation_status,
        "execution_plane": ExecutionPlane::Cloud.as_str(),
        "project_id": context.project.id,
        "requirement_id": requirement_id,
        "contact_id": contact_runtime.contact.contact_id,
        "conversation_id": session.id,
        "execution_group_id": new_execution_group_id,
        "message_id": new_message.id,
        "message": MessageOut::from(new_message),
        "started_runs": started_runs,
        "root_task_ids": clone_result.get("root_task_ids").cloned().unwrap_or_else(|| json!([])),
        "replaced_execution_group_id": identity.execution_group_id,
        "cleanup": cleanup,
        "has_started_runs": has_started_runs,
        "confirmation_status": confirmation_status,
        "start_error": start_error,
        "warnings": post_start_warnings,
    }))
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
    for link in links
        .iter_mut()
        .filter(|link| task_runner_status_is_active(link.task_runner_status.as_deref()))
    {
        let task = task_runner_api_client::get_task_runner_task(
            contact_runtime.task_runner_base_url.as_str(),
            contact_runtime.task_runner_agent_token.as_str(),
            link.task_runner_task_id.as_str(),
        )
        .await
        .map_err(|err| HandlerError::bad_gateway("校验 Task Runner 任务状态失败", err))?;
        link.task_runner_status = Some(task.status.clone());
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
        if !task_runner_status_is_active(link.task_runner_status.as_deref()) {
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
    mark_execution_messages_for_stop(&active_links, STATUS_STOPPED).await;
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
