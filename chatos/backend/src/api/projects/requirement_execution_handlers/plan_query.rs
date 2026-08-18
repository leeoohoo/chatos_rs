// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::StatusCode;
use chatos_project_execution::{
    read_planning_feedback_history, requirement_execution_recovery_state,
    requirement_execution_status_is_stopped_terminal, ExecutionPlanIdentity, ExecutionPlane,
    STATUS_PAUSED, STATUS_PLANNING, STATUS_PLANNING_STARTED, STATUS_STOPPED, STATUS_STOPPING,
};
use serde_json::{json, Value};

use crate::core::auth::AuthUser;
use crate::core::messages::{ensure_message_metadata_object, MessageOut};
use crate::modules::conversation_runtime::messages as conversation_messages;
use crate::services::chatos_sessions;

use super::super::requirement_execution::{
    collect_requirement_execution_scope, load_execution_links_for_work_items,
    load_requirement_execution_request_context, parse_requirements, parse_work_items,
    project_plan_array, project_plan_value, requirement_dependency_map, value_string,
    ExecutionLink, HandlerError,
};
use super::{expected_execution_project_task_ids, RequirementExecutionPlanQuery};

pub(super) async fn get_requirement_execution_plan_inner(
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
                    "recovery_action": "none",
                    "recovery_reason": "source_missing",
                    "replace_previous_batch": false,
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
            "recovery_action": "none",
            "recovery_reason": "source_missing",
            "replace_previous_batch": false,
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

pub(super) fn cloud_execution_message_matches_scope(
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

pub(super) fn cloud_execution_message_is_newer(
    candidate: &crate::models::message::Message,
    current: &crate::models::message::Message,
) -> bool {
    (candidate.created_at.as_str(), candidate.id.as_str())
        > (current.created_at.as_str(), current.id.as_str())
}

pub(super) async fn find_latest_cloud_execution_source_message(
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

pub(super) async fn load_cloud_execution_source_message(
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

pub(crate) fn execution_message_status(message: &crate::models::message::Message) -> String {
    let task_runner = message
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("task_runner_async"));
    let overall_status = value_string(task_runner.unwrap_or(&Value::Null), "overall_status")
        .map(|value| value.trim().to_ascii_lowercase());
    let confirmation_status =
        value_string(task_runner.unwrap_or(&Value::Null), "confirmation_status")
            .map(|value| value.trim().to_ascii_lowercase());
    let status = if overall_status
        .as_deref()
        .is_some_and(execution_status_is_stop_locked)
    {
        overall_status.unwrap_or_default()
    } else if confirmation_status
        .as_deref()
        .is_some_and(execution_status_is_stop_locked)
    {
        confirmation_status.unwrap_or_default()
    } else if overall_status
        .as_deref()
        .is_some_and(execution_status_is_failure_terminal)
        || confirmation_status
            .as_deref()
            .is_some_and(execution_status_is_failure_terminal)
    {
        "failed".to_string()
    } else {
        overall_status
            .or(confirmation_status)
            .unwrap_or_else(|| STATUS_PLANNING_STARTED.to_string())
    };
    if !execution_status_is_stop_locked(status.as_str())
        && task_runner_metadata_has_stop_marker(task_runner)
    {
        STATUS_STOPPED.to_string()
    } else {
        status
    }
}

fn execution_status_is_failure_terminal(status: &str) -> bool {
    matches!(status.trim(), "failed" | "error" | "blocked")
}

#[cfg(test)]
pub(super) fn execution_message_is_stopped_terminal(
    message: &crate::models::message::Message,
) -> bool {
    execution_status_is_stopped_terminal(execution_message_status(message).as_str())
}

pub(super) fn execution_status_is_stopped_terminal(status: &str) -> bool {
    requirement_execution_status_is_stopped_terminal(status)
}

fn execution_status_is_stop_locked(status: &str) -> bool {
    matches!(
        status.trim().to_ascii_lowercase().as_str(),
        STATUS_STOPPING | STATUS_STOPPED | "cancelled" | "canceled"
    )
}

fn task_runner_metadata_has_stop_marker(task_runner: Option<&Value>) -> bool {
    let Some(task_runner) = task_runner else {
        return false;
    };
    task_runner
        .get("stopped_at")
        .and_then(Value::as_str)
        .is_some_and(|value| !value.trim().is_empty())
        || task_runner
            .get("stopped_task_ids")
            .and_then(Value::as_array)
            .is_some_and(|values| !values.is_empty())
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
    let task_count = links.len();
    let has_started_runs = links.iter().any(|link| link.task_runner_run_id.is_some());
    let recovery = requirement_execution_recovery_state(
        status.as_str(),
        task_count,
        has_started_runs,
        true,
        false,
    );
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
        "task_count": task_count,
        "has_started_runs": has_started_runs,
        "recovery_action": recovery.action,
        "recovery_reason": recovery.reason,
        "replace_previous_batch": recovery.replace_previous_batch,
        "failure_kind": failure_kind,
        "failure_reason": failure_reason,
        "failed_at": failed_at,
        "created_at": message.created_at,
        "message": MessageOut::from(message),
    })
}

pub(super) const STALE_PLANNER_NO_TASK_TIMEOUT_SECONDS: i64 = 10 * 60;
const STALE_PLANNER_FAILURE_KIND: &str = "stale_planning_agent";
const STALE_PLANNER_FAILURE_REASON: &str = "规划 Agent 已中断：后端重启或运行进程丢失后，执行计划长时间没有创建任何 Task Runner 任务。请重新生成执行计划。";

pub(super) fn is_cloud_execution_planner_status_pending(status: &str) -> bool {
    matches!(
        status.trim().to_ascii_lowercase().as_str(),
        STATUS_PLANNING | STATUS_PLANNING_STARTED | "pending" | "processing"
    )
}

pub(crate) fn cloud_execution_planner_message_is_stale(
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

pub(crate) async fn repair_stale_cloud_execution_planner_message(
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
