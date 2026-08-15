// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use chatos_project_execution::{
    read_planning_feedback_history, ExecutionPlanIdentity, ExecutionPlane,
    STATUS_AWAITING_CONFIRMATION, STATUS_EXECUTION_STARTED,
};
use serde_json::{json, Value};
use tracing::warn;

use super::super::requirement_execution::{
    create_execution_message, load_execution_links_for_work_items,
    load_requirement_execution_request_context, parse_requirements, parse_work_items,
    project_plan_array, select_contact_runtime, set_execution_turn_hidden,
    sync_execution_link_status, sync_execution_message_task_tracking,
    sync_requirement_execution_state, task_runner_callback_event_for_status, value_string,
    ExecutionLink, HandlerError,
};
use super::plan_query::load_cloud_execution_source_message;
use super::rerun_support::{
    discard_cloned_project_execution, ensure_old_cloud_execution_batch_ready_for_replacement,
    expand_project_task_scope_to_actual_graph, expected_execution_project_task_ids,
    parse_rerun_clone_mappings, started_runs_by_task_id, validate_rerun_cloned_project_task_scope,
};
use super::{retire_cloud_execution_batch, RerunRequirementExecutionRequest};
use crate::core::auth::AuthUser;
use crate::core::messages::{set_task_runner_async_overall_status_for_session, MessageOut};
use crate::services::{chatos_sessions, project_management_api_client, task_runner_api_client};

pub(super) async fn rerun_requirement_execution_inner(
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
    let mut old_links = load_execution_links_for_work_items(
        context.cfg.project_service_base_url.as_str(),
        context.access_token.as_str(),
        selected_work_items.as_slice(),
    )
    .await?
    .into_iter()
    .filter(|link| {
        link.source_session_id.as_deref() == Some(identity.conversation_id.as_str())
            && link.source_user_message_id.as_deref() == Some(identity.execution_group_id.as_str())
    })
    .collect::<Vec<_>>();
    ensure_old_cloud_execution_batch_ready_for_replacement(
        &old_message,
        contact_runtime.task_runner_base_url.as_str(),
        contact_runtime.task_runner_agent_token.as_str(),
        context.access_token.as_str(),
        context.project_sync_secret.as_str(),
        identity.conversation_id.as_str(),
        identity.execution_group_id.as_str(),
        root_requirement.title.as_str(),
        "只有已取消或已停止的执行批次才能重新执行",
        old_links.as_mut_slice(),
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
    let parsed_clone = match parse_rerun_clone_mappings(&clone_result) {
        Ok(parsed) => parsed,
        Err(detail) => {
            discard_cloned_project_execution(
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
    let cloned_dag_node_count = parsed_clone.dag_node_count;
    let mapped_project_task_ids = parsed_clone.mapped_project_task_ids;
    let parsed_mappings = parsed_clone.task_mappings;
    let expanded_project_task_ids = expand_project_task_scope_to_actual_graph(
        &expected_project_task_ids,
        &mapped_project_task_ids,
    );
    let mut reload_old_links_for_expanded_scope = false;
    if expanded_project_task_ids != expected_project_task_ids {
        let expanded_work_items = all_work_items
            .iter()
            .filter(|item| expanded_project_task_ids.contains(item.id.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        if expanded_work_items.len() != expanded_project_task_ids.len() {
            discard_cloned_project_execution(
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
        reload_old_links_for_expanded_scope = true;
    }
    if let Err(detail) = validate_rerun_cloned_project_task_scope(
        &expected_project_task_ids,
        &mapped_project_task_ids,
        cloned_dag_node_count,
    ) {
        discard_cloned_project_execution(
            contact_runtime.task_runner_base_url.as_str(),
            context.project.id.as_str(),
            requirement_id.as_str(),
            session.id.as_str(),
            new_execution_group_id.as_str(),
        )
        .await;
        return Err(HandlerError::bad_gateway("复制执行图不完整", detail));
    }
    if reload_old_links_for_expanded_scope {
        old_links = match load_execution_links_for_work_items(
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
                discard_cloned_project_execution(
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
    }
    let mut new_links = Vec::new();
    for (project_task_id, task_runner_task_id) in parsed_mappings {
        let sync_result = project_management_api_client::sync_work_item_task_runner_status(
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
                supersedes_task_runner_task_ids: Vec::new(),
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
        let started_runs_by_task_id = started_runs_by_task_id(&started_runs);
        for link in &mut new_links {
            let Some((run_id, run_status)) =
                started_runs_by_task_id.get(link.task_runner_task_id.as_str())
            else {
                continue;
            };
            link.task_runner_run_id = Some(run_id.clone());
            link.task_runner_status = Some(run_status.clone());
            if let Err(error) = sync_execution_link_status(
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
