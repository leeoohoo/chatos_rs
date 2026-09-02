// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};

use axum::http::StatusCode;
use chatos_project_execution::{
    append_planning_feedback, build_requirement_execution_planner_prompt,
    build_requirement_execution_user_message, executing_requirement_ids,
    format_planning_feedback_history, read_planning_feedback_history,
    select_unblocked_pending_work_items, sort_work_items_for_planning, ExecutionPlanIdentity,
    NEXT_ACTION_PREVIEW_AND_CONFIRM, RECOVERY_ACTION_NONE, STATUS_PLANNING_STARTED,
};
use serde_json::{json, Value};

use crate::api::chat_stream_common::ChatStreamRequest;
use crate::config::Config;
use crate::core::auth::AuthUser;
use crate::core::validation::normalize_non_empty;
use crate::modules::conversation_runtime::chat_usecase::{run_chat_usecase, RunChatUsecaseInput};
use crate::modules::conversation_runtime::guidance;
use crate::services::{access_token_scope, project_management_api_client, user_service_api_client};
use crate::utils::abort_registry;

use super::super::requirement_execution::{
    add_requirement_work_item_dependencies, collect_requirement_execution_scope,
    create_execution_message, ensure_requirement_execution_not_active,
    load_execution_links_for_work_items, load_requirement_execution_request_context,
    parse_requirements, parse_work_items, project_plan_array, project_plan_value,
    requirement_dependency_map, resolve_or_create_execution_session, select_contact_runtime,
    sync_requirement_execution_state, topological_work_item_order,
    validate_requirement_prerequisites, work_item_dependency_map, HandlerError,
};
use super::plan_query::{
    find_latest_cloud_execution_source_message, is_cloud_execution_planner_status_pending,
};
use super::rerun_support::ensure_old_cloud_execution_batch_ready_for_replacement;
use super::{
    expected_execution_project_task_ids, load_cloud_execution_source_message,
    repair_stale_cloud_execution_planner_message, ExecuteRequirementRequest,
};

pub(super) async fn execute_requirement_inner(
    auth: AuthUser,
    project_id: String,
    requirement_id: String,
    req: ExecuteRequirementRequest,
) -> Result<Value, HandlerError> {
    let explicitly_requested_model_config_id = normalize_non_empty(req.model_config_id.clone());
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
    let configured_model_config_id = if explicitly_requested_model_config_id.is_none() {
        load_project_management_agent_model_config_id(
            cfg,
            access_token.as_str(),
            auth.user_id.as_str(),
        )
        .await?
    } else {
        None
    };
    let requested_model_config_id = select_requirement_planner_model_config_id(
        explicitly_requested_model_config_id,
        configured_model_config_id,
    );

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
    let pending_work_items = chatos_project_execution::select_pending_work_items(
        all_work_items.as_slice(),
        &requirement_scope,
    );
    add_requirement_work_item_dependencies(
        &mut dependency_map,
        &pending_work_items,
        &requirement_dependency_map,
        &requirement_scope,
    );
    let mut selected_work_items = select_unblocked_pending_work_items(
        all_work_items.as_slice(),
        &requirement_scope,
        &dependency_map,
    )
    .map_err(HandlerError::bad_request)?;
    let creation_order = topological_work_item_order(&selected_work_items, &dependency_map)?;
    sort_work_items_for_planning(selected_work_items.as_mut_slice());
    if selected_work_items.is_empty() {
        return Err(HandlerError::bad_request(
            "该需求执行范围内没有需要执行的未完成项目任务",
        ));
    }
    ensure_latest_cloud_execution_planner_not_active(
        &auth,
        project.id.as_str(),
        requirement_id.as_str(),
        cfg.project_service_base_url.as_str(),
        access_token.as_str(),
        all_work_items.as_slice(),
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
        user_role: Some(auth.role.clone()),
        attachments: None,
        reasoning_enabled: None,
        plan_mode: false,
        turn_id: Some(execution_group_id.clone()),
        contact_agent_id: Some(contact_runtime.contact.agent_id.clone()),
        project_id: Some(project.id.clone()),
        project_root: Some(project.root_path.clone()),
        workspace_root: Some(project.root_path.clone()),
        remote_connection_id: None,
        task_plugin_preferences: Vec::new(),
        unsupported_plugin_agent_selection: None,
        user_message_id: Some(execution_group_id.clone()),
        project_requirement_execution_planner: true,
        project_requirement_execution_task_ids: selected_work_items
            .iter()
            .map(|item| item.id.clone())
            .collect(),
    };
    let persisted_user_message_metadata = message.metadata.clone();
    prepare_requirement_planner_turn(session.id.as_str(), execution_group_id.as_str());
    let planner_input = RunChatUsecaseInput {
        sender: None,
        req: chat_req,
        persisted_user_message_content: Some(user_visible_content),
        persisted_user_message_metadata,
        cloud_agent_owner_context: Some(json!({
            "kind": "requirement_planner",
            "project_id": project.id,
            "requirement_id": requirement_id,
            "session_id": session.id,
            "execution_group_id": execution_group_id,
            "executing_requirement_ids": executing_requirement_ids,
            "link_scope_work_items": selected_work_items,
            "selected_work_items": selected_work_items,
            "replacement_identity": replacement_identity,
            "replacement_work_items": replacement_work_items,
        })),
    };
    access_token_scope::spawn_with_access_token(Some(access_token), async move {
        run_chat_usecase(planner_input).await;
    });

    Ok(json!({
        "success": true,
        "status": STATUS_PLANNING_STARTED,
        "next_action": NEXT_ACTION_PREVIEW_AND_CONFIRM,
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
        "recovery_action": RECOVERY_ACTION_NONE,
        "recovery_reason": "not_recoverable_in_current_state",
        "replace_previous_batch": true,
        "conversation_id": session.id,
        "message_id": execution_group_id.clone(),
        "message": message,
        "execution_group_id": execution_group_id,
        "planner_agent_key": chatos_plugin_management_sdk::SystemAgentKey::ProjectRequirementExecutionPlannerAgent.as_str(),
        "plan_mode_enabled": false,
    }))
}

fn select_requirement_planner_model_config_id(
    explicitly_requested: Option<String>,
    configured_default: Option<String>,
) -> Option<String> {
    explicitly_requested.or(configured_default)
}

async fn load_project_management_agent_model_config_id(
    cfg: &Config,
    access_token: &str,
    user_id: &str,
) -> Result<Option<String>, HandlerError> {
    let Some(base_url) = cfg
        .user_service_base_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    let settings = user_service_api_client::get_model_settings(
        base_url,
        access_token,
        Some(user_id),
        cfg.user_service_request_timeout_ms,
    )
    .await
    .map_err(|err| HandlerError::bad_gateway("读取项目管理 Agent 模型设置失败", err))?;
    Ok(normalize_non_empty(
        settings.project_management_agent_model_config_id,
    ))
}

async fn ensure_latest_cloud_execution_planner_not_active(
    auth: &AuthUser,
    project_id: &str,
    requirement_id: &str,
    project_service_base_url: &str,
    access_token: &str,
    all_work_items: &[super::super::requirement_execution::WorkItemPlanItem],
) -> Result<(), HandlerError> {
    let Some(message) =
        find_latest_cloud_execution_source_message(auth, project_id, requirement_id).await?
    else {
        return Ok(());
    };
    let project_task_ids =
        expected_execution_project_task_ids(message.metadata.as_ref(), project_id, requirement_id)?;
    let planner_work_items = all_work_items
        .iter()
        .filter(|item| project_task_ids.contains(item.id.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    let current_links = load_execution_links_for_work_items(
        project_service_base_url,
        access_token,
        planner_work_items.as_slice(),
    )
    .await?
    .into_iter()
    .filter(|link| {
        link.source_session_id.as_deref() == Some(message.session_id.as_str())
            && link.source_user_message_id.as_deref() == Some(message.id.as_str())
    })
    .collect::<Vec<_>>();
    let message =
        repair_stale_cloud_execution_planner_message(message, current_links.is_empty()).await?;
    if is_cloud_execution_planner_status_pending(
        super::plan_query::execution_message_status(&message).as_str(),
    ) {
        return Err(HandlerError::bad_request(
            "该需求已有正在生成的执行计划，请等待当前规划完成或停止后重试",
        ));
    }
    Ok(())
}

pub(super) fn prepare_requirement_planner_turn(session_id: &str, execution_group_id: &str) {
    abort_registry::reset_turn(session_id, Some(execution_group_id));
    guidance::register_active_turn(session_id, execution_group_id);
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

#[cfg(test)]
mod tests {
    use super::select_requirement_planner_model_config_id;

    #[test]
    fn explicit_requirement_model_overrides_configured_project_management_default() {
        assert_eq!(
            select_requirement_planner_model_config_id(
                Some("explicit-model".to_string()),
                Some("configured-model".to_string()),
            )
            .as_deref(),
            Some("explicit-model"),
        );
    }

    #[test]
    fn configured_project_management_model_is_used_when_request_omits_model() {
        assert_eq!(
            select_requirement_planner_model_config_id(None, Some("configured-model".to_string()),)
                .as_deref(),
            Some("configured-model"),
        );
    }
}
