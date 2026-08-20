// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};

use chatos_project_execution::{
    validate_exact_project_task_scope, ExecutionPlanIdentity, STATUS_EXECUTION_STARTED,
    STATUS_PAUSED,
};
use serde_json::{json, Value};

use crate::core::auth::AuthUser;
use crate::core::messages::set_task_runner_async_execution_paused_for_session;
use crate::services::task_runner_api_client;

use super::super::requirement_execution::{
    load_execution_links_for_work_items, load_requirement_execution_request_context,
    parse_work_items, project_plan_array, select_contact_runtime, set_execution_turn_hidden,
    sync_execution_link_status, sync_execution_message_task_tracking,
    sync_requirement_execution_state, task_runner_callback_event_for_status,
    task_runner_status_is_active, value_string, HandlerError,
};
use super::{
    expand_project_task_scope_to_actual_graph, load_expected_execution_project_task_ids,
    ConfirmRequirementExecutionRequest, MutateRequirementExecutionDispatchRequest,
};

pub(super) async fn confirm_requirement_execution_inner(
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
        "project_id": context.project.id,
        "requirement_id": requirement_id,
        "conversation_id": conversation_id,
        "execution_group_id": execution_group_id,
        "started_runs": confirmation.get("started_runs").cloned().unwrap_or_else(|| json!([])),
        "root_task_ids": confirmation.get("root_task_ids").cloned().unwrap_or_else(|| json!([])),
    }))
}

pub(super) async fn mutate_requirement_execution_dispatch_inner(
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
        "project_id": context.project.id,
        "requirement_id": requirement_id,
        "conversation_id": identity.conversation_id,
        "execution_group_id": identity.execution_group_id,
        "running_count": result.get("running_count").cloned().unwrap_or_else(|| json!(0)),
        "queued_count": result.get("queued_count").cloned().unwrap_or_else(|| json!(0)),
        "started_runs": result.get("started_runs").cloned().unwrap_or_else(|| json!([])),
    }))
}
