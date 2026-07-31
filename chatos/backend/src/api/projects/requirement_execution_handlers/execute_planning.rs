// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};

use axum::http::StatusCode;
use chatos_project_execution::{
    append_planning_feedback, build_requirement_execution_planner_prompt,
    build_requirement_execution_user_message, executing_requirement_ids,
    format_planning_feedback_history, read_planning_feedback_history, select_pending_work_items,
    sort_work_items_for_planning, ExecutionPlanIdentity, ExecutionPlane,
    NEXT_ACTION_PREVIEW_AND_CONFIRM, STATUS_PLANNING_STARTED,
};
use serde_json::{json, Value};
use tracing::warn;

use crate::api::chat_stream_common::ChatStreamRequest;
use crate::core::auth::AuthUser;
use crate::core::validation::normalize_non_empty;
use crate::modules::conversation_runtime::chat_usecase::{run_chat_usecase, RunChatUsecaseInput};
use crate::services::{access_token_scope, project_management_api_client};

use super::super::requirement_execution::{
    add_requirement_work_item_dependencies, collect_requirement_execution_scope,
    create_execution_message, ensure_requirement_execution_not_active,
    load_execution_links_for_work_items, load_requirement_execution_request_context,
    parse_requirements, parse_work_items, project_plan_array, project_plan_value,
    requirement_dependency_map, resolve_or_create_execution_session, select_contact_runtime,
    sync_requirement_execution_state, topological_work_item_order,
    validate_requirement_prerequisites, work_item_dependency_map, HandlerError,
};
use super::rerun_support::ensure_old_cloud_execution_batch_ready_for_replacement;
use super::{
    expected_execution_project_task_ids, load_cloud_execution_source_message,
    reconcile_requirement_planner_outcome, ExecuteRequirementRequest, RequirementPlannerRecovery,
};

pub(super) async fn execute_requirement_inner(
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
    let all_work_items = parse_work_items(project_plan_array(&plan, "work_items", "workItems"));
    let contact_runtime = select_contact_runtime(
        &auth,
        cfg,
        req.contact_id,
        project.id.as_str(),
        access_token.as_str(),
    )
    .await?;
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
                previous_planning_feedback = read_planning_feedback_history(
                    replaced_message
                        .metadata
                        .as_ref()
                        .and_then(|metadata| metadata.get("project_requirement_execution")),
                );
                let project_task_ids = expected_execution_project_task_ids(
                    replaced_message.metadata.as_ref(),
                    project.id.as_str(),
                    requirement_id.as_str(),
                )?;
                let replacement_work_items_for_readiness = all_work_items
                    .iter()
                    .filter(|item| project_task_ids.contains(item.id.as_str()))
                    .cloned()
                    .collect::<Vec<_>>();
                if replacement_work_items_for_readiness.len() != project_task_ids.len() {
                    return Err(HandlerError::bad_request(
                        "原执行批次包含已经删除或不可见的项目任务，不能直接重新规划",
                    ));
                }
                let mut old_links = load_execution_links_for_work_items(
                    cfg.project_service_base_url.as_str(),
                    access_token.as_str(),
                    replacement_work_items_for_readiness.as_slice(),
                )
                .await?
                .into_iter()
                .filter(|link| {
                    link.source_session_id.as_deref() == Some(identity.conversation_id.as_str())
                        && link.source_user_message_id.as_deref()
                            == Some(identity.execution_group_id.as_str())
                })
                .collect::<Vec<_>>();
                ensure_old_cloud_execution_batch_ready_for_replacement(
                    &replaced_message,
                    contact_runtime.task_runner_base_url.as_str(),
                    contact_runtime.task_runner_agent_token.as_str(),
                    access_token.as_str(),
                    cfg.project_service_base_url.as_str(),
                    project_sync_secret.as_str(),
                    identity.conversation_id.as_str(),
                    identity.execution_group_id.as_str(),
                    root_requirement.title.as_str(),
                    "重新规划前必须先取消或停止旧执行批次",
                    old_links.as_mut_slice(),
                )
                .await?;
                Some(project_task_ids)
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
