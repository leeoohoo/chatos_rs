// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};

use axum::http::StatusCode;
use chatos_project_execution::{
    append_planning_feedback, build_requirement_execution_planner_prompt,
    build_requirement_execution_user_message, executing_requirement_ids,
    format_planning_feedback_history, read_planning_feedback_history,
    select_unblocked_pending_work_items, sort_work_items_for_planning, ExecutionPlanIdentity,
    ExecutionPlane, NEXT_ACTION_PREVIEW_AND_CONFIRM, RECOVERY_ACTION_NONE, STATUS_PLANNING_STARTED,
};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};

use crate::api::chat_stream_common::ChatStreamRequest;
use crate::core::auth::AuthUser;
use crate::core::validation::normalize_non_empty;
use crate::modules::conversation_runtime::chat_usecase::{run_chat_usecase, RunChatUsecaseInput};
use crate::modules::conversation_runtime::guidance;
use crate::services::project_management_api_client;
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
    if project_requires_cloud_runtime_initialization(project.source_type.as_deref()) {
        ensure_project_runtime_environment_initialization(
            cfg.project_service_base_url.as_str(),
            access_token.as_str(),
            project.id.as_str(),
            &root_requirement,
            &selected_work_items,
            &requirement_documents,
        )
        .await?;
    }
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
        selected_plugin_ids: Vec::new(),
        plugin_command_invocations: Vec::new(),
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
    run_chat_usecase(RunChatUsecaseInput {
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
    })
    .await;

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

#[derive(Debug, PartialEq, Eq)]
enum RuntimeEnvironmentInitializationAction {
    None,
    Analyze,
    GenerateImage(String),
}

fn project_requires_cloud_runtime_initialization(source_type: Option<&str>) -> bool {
    source_type
        .map(str::trim)
        .is_some_and(|source_type| source_type.eq_ignore_ascii_case("cloud"))
}

const RUNTIME_ANALYSIS_REQUIREMENT_MAX_CHARS: usize = 4_000;
const RUNTIME_ANALYSIS_STALE_AFTER_SECONDS: i64 = 15 * 60;

fn runtime_environment_initialization_action(
    current: &Value,
    analysis_requirement: Option<&str>,
) -> RuntimeEnvironmentInitializationAction {
    runtime_environment_initialization_action_at(current, analysis_requirement, Utc::now())
}

fn runtime_environment_initialization_action_at(
    current: &Value,
    analysis_requirement: Option<&str>,
    now: DateTime<Utc>,
) -> RuntimeEnvironmentInitializationAction {
    let status = current
        .get("environment")
        .and_then(|environment| environment.get("status"))
        .and_then(Value::as_str)
        .map(str::trim)
        .map(str::to_ascii_lowercase);
    match status.as_deref() {
        Some("ready") if runtime_environment_matches_requirement(current, analysis_requirement) => {
            RuntimeEnvironmentInitializationAction::None
        }
        Some("ready") => RuntimeEnvironmentInitializationAction::Analyze,
        Some("analyzing") if !runtime_environment_analysis_is_stale(current, now) => {
            RuntimeEnvironmentInitializationAction::None
        }
        Some("analyzing") => RuntimeEnvironmentInitializationAction::Analyze,
        Some("pending_image_build") => current
            .get("images")
            .and_then(Value::as_array)
            .and_then(|images| {
                images.iter().find_map(|image| {
                    let is_workspace = image
                        .get("service_role")
                        .and_then(Value::as_str)
                        .is_some_and(|value| value.eq_ignore_ascii_case("workspace"));
                    let is_planned = image
                        .get("status")
                        .and_then(Value::as_str)
                        .is_some_and(|value| value.eq_ignore_ascii_case("planned"));
                    (is_workspace && is_planned)
                        .then(|| image.get("id").and_then(Value::as_str))
                        .flatten()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(ToOwned::to_owned)
                })
            })
            .map(RuntimeEnvironmentInitializationAction::GenerateImage)
            .unwrap_or(RuntimeEnvironmentInitializationAction::None),
        _ => RuntimeEnvironmentInitializationAction::Analyze,
    }
}

fn runtime_environment_analysis_is_stale(current: &Value, now: DateTime<Utc>) -> bool {
    let updated_at = current
        .get("environment")
        .and_then(|environment| environment.get("detected_stack"))
        .and_then(|stack| stack.get("analysis_progress"))
        .and_then(|progress| progress.get("updated_at"))
        .and_then(Value::as_str)
        .or_else(|| {
            current
                .get("environment")
                .and_then(|environment| environment.get("updated_at"))
                .and_then(Value::as_str)
        });
    let Some(updated_at) = updated_at else {
        return true;
    };
    let Ok(updated_at) = DateTime::parse_from_rfc3339(updated_at) else {
        return true;
    };
    now.signed_duration_since(updated_at.with_timezone(&Utc))
        .num_seconds()
        >= RUNTIME_ANALYSIS_STALE_AFTER_SECONDS
}

async fn ensure_project_runtime_environment_initialization(
    project_service_base_url: &str,
    access_token: &str,
    project_id: &str,
    root_requirement: &chatos_project_execution::RequirementPlanItem,
    selected_work_items: &[chatos_project_execution::WorkItemPlanItem],
    requirement_documents: &BTreeMap<String, Value>,
) -> Result<(), HandlerError> {
    let (analysis_requirement, selected_dependencies) = execution_runtime_analysis_request(
        root_requirement,
        selected_work_items,
        requirement_documents,
    );
    let current = project_management_api_client::get_project_service_runtime_environment(
        project_service_base_url,
        access_token,
        project_id,
    )
    .await
    .map_err(|err| HandlerError::bad_gateway("读取项目运行环境失败", err))?;
    match runtime_environment_initialization_action(&current, Some(analysis_requirement.as_str())) {
        RuntimeEnvironmentInitializationAction::None => Ok(()),
        RuntimeEnvironmentInitializationAction::GenerateImage(image_record_id) => {
            project_management_api_client::generate_project_service_runtime_environment_image(
                project_service_base_url,
                access_token,
                project_id,
                image_record_id.as_str(),
            )
            .await
            .map(|_| ())
            .map_err(|err| HandlerError::bad_gateway("启动项目执行环境镜像生成失败", err))
        }
        RuntimeEnvironmentInitializationAction::Analyze => {
            project_management_api_client::analyze_project_service_runtime_environment(
                project_service_base_url,
                access_token,
                project_id,
                &project_management_api_client::AnalyzeProjectRuntimeEnvironmentRequest {
                    analysis_requirement: Some(analysis_requirement),
                    selected_dependencies,
                },
            )
            .await
            .map(|_| ())
            .map_err(|err| HandlerError::bad_gateway("启动项目运行环境初始化失败", err))
        }
    }
}

fn runtime_environment_matches_requirement(current: &Value, expected: Option<&str>) -> bool {
    let expected = expected.map(str::trim).filter(|value| !value.is_empty());
    let Some(expected) = expected else {
        return true;
    };
    current
        .get("environment")
        .and_then(|environment| environment.get("detected_stack"))
        .and_then(|stack| stack.get("analysis_requirement"))
        .and_then(Value::as_str)
        .map(str::trim)
        == Some(expected)
}

fn execution_runtime_analysis_request(
    root_requirement: &chatos_project_execution::RequirementPlanItem,
    selected_work_items: &[chatos_project_execution::WorkItemPlanItem],
    requirement_documents: &BTreeMap<String, Value>,
) -> (String, Vec<String>) {
    let mut sections = vec![format!("执行需求：{}", root_requirement.title.trim())];
    let mut dependencies = BTreeMap::<String, String>::new();

    for item in selected_work_items {
        let mut task = format!("实施任务：{}", item.title.trim());
        if let Some(description) = item
            .description
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            task.push('\n');
            task.push_str(description);
        }
        sections.push(task);
        for tag in &item.tags {
            let tag = tag.trim();
            if !tag.is_empty() && tag.chars().count() <= 80 {
                dependencies
                    .entry(tag.to_ascii_lowercase())
                    .or_insert_with(|| tag.to_string());
            }
        }
    }

    for documents in requirement_documents.values() {
        let Some(documents) = documents.as_array() else {
            continue;
        };
        for document in documents {
            let title = document
                .get("title")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty());
            let content = document
                .get("content")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty());
            if title.is_none() && content.is_none() {
                continue;
            }
            let mut section = format!("技术文档：{}", title.unwrap_or("未命名"));
            if let Some(content) = content {
                section.push('\n');
                section.push_str(content);
            }
            sections.push(section);
        }
    }

    let requirement = truncate_chars(
        sections.join("\n\n").as_str(),
        RUNTIME_ANALYSIS_REQUIREMENT_MAX_CHARS,
    );
    (requirement, dependencies.into_values().take(64).collect())
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let keep = max_chars.saturating_sub(3);
    let mut truncated = value.chars().take(keep).collect::<String>();
    truncated.push_str("...");
    truncated
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
mod runtime_environment_tests {
    use std::collections::BTreeMap;

    use chatos_project_execution::{RequirementPlanItem, WorkItemPlanItem};
    use serde_json::json;

    use super::{
        execution_runtime_analysis_request, project_requires_cloud_runtime_initialization,
        runtime_environment_initialization_action, runtime_environment_initialization_action_at,
        RuntimeEnvironmentInitializationAction, RUNTIME_ANALYSIS_REQUIREMENT_MAX_CHARS,
    };
    use chrono::{TimeZone, Utc};

    #[test]
    fn execution_context_drives_runtime_analysis_request() {
        let requirement = RequirementPlanItem {
            id: "requirement-1".to_string(),
            title: "Build inventory system".to_string(),
            status: "approved".to_string(),
            parent_requirement_id: None,
        };
        let work_items = vec![WorkItemPlanItem {
            id: "work-item-1".to_string(),
            requirement_id: requirement.id.clone(),
            title: "Implement Rust API".to_string(),
            description: Some("Use Axum and PostgreSQL with a React frontend.".to_string()),
            status: "todo".to_string(),
            priority: 5,
            tags: vec![
                "Rust".to_string(),
                "PostgreSQL".to_string(),
                "rust".to_string(),
            ],
            is_planning_task: false,
        }];
        let documents = BTreeMap::from([(
            requirement.id.clone(),
            json!([{
                "title": "Runtime design",
                "content": "The workspace requires Cargo, Node.js and a PostgreSQL service.",
            }]),
        )]);

        let (analysis_requirement, dependencies) =
            execution_runtime_analysis_request(&requirement, &work_items, &documents);

        assert!(analysis_requirement.contains("Build inventory system"));
        assert!(analysis_requirement.contains("Use Axum and PostgreSQL"));
        assert!(analysis_requirement.contains("requires Cargo, Node.js"));
        assert!(analysis_requirement.chars().count() <= RUNTIME_ANALYSIS_REQUIREMENT_MAX_CHARS);
        assert_eq!(dependencies, vec!["PostgreSQL", "Rust"]);
    }

    #[test]
    fn ready_or_active_runtime_environment_waits_for_completion() {
        assert_eq!(
            runtime_environment_initialization_action(
                &json!({
                    "environment": { "status": "ready" },
                }),
                None
            ),
            RuntimeEnvironmentInitializationAction::None
        );
        assert_eq!(
            runtime_environment_initialization_action(
                &json!({
                    "environment": {
                        "status": "ANALYZING",
                        "updated_at": Utc::now().to_rfc3339(),
                    },
                }),
                Some("React and Rust")
            ),
            RuntimeEnvironmentInitializationAction::None
        );
        assert_eq!(
            runtime_environment_initialization_action(
                &json!({
                    "environment": { "status": "pending_image_build" },
                    "images": [{
                        "id": "workspace-image",
                        "service_role": "workspace",
                        "status": "building",
                    }],
                }),
                Some("React and Rust")
            ),
            RuntimeEnvironmentInitializationAction::None
        );
    }

    #[test]
    fn only_cloud_projects_require_server_managed_runtime_initialization() {
        assert!(project_requires_cloud_runtime_initialization(Some("cloud")));
        assert!(!project_requires_cloud_runtime_initialization(Some(
            "local"
        )));
        assert!(!project_requires_cloud_runtime_initialization(Some(
            "local_connector"
        )));
        assert!(!project_requires_cloud_runtime_initialization(None));
    }

    #[test]
    fn stale_or_unverifiable_analyzing_environment_is_restarted() {
        let now = Utc.with_ymd_and_hms(2026, 8, 17, 8, 0, 0).unwrap();
        for current in [
            json!({ "environment": { "status": "analyzing" } }),
            json!({
                "environment": {
                    "status": "analyzing",
                    "detected_stack": {
                        "analysis_progress": {
                            "updated_at": "2026-08-17T07:30:00Z"
                        }
                    }
                }
            }),
        ] {
            assert_eq!(
                runtime_environment_initialization_action_at(&current, Some("React and Rust"), now,),
                RuntimeEnvironmentInitializationAction::Analyze
            );
        }
    }

    #[test]
    fn ready_runtime_environment_is_reanalyzed_for_new_execution_requirement() {
        assert_eq!(
            runtime_environment_initialization_action(
                &json!({
                    "environment": {
                        "status": "ready",
                        "detected_stack": { "analysis_requirement": "Node only" },
                    },
                }),
                Some("React, Rust and PostgreSQL")
            ),
            RuntimeEnvironmentInitializationAction::Analyze
        );
        assert_eq!(
            runtime_environment_initialization_action(
                &json!({
                    "environment": {
                        "status": "ready",
                        "detected_stack": {
                            "analysis_requirement": "React, Rust and PostgreSQL",
                        },
                    },
                }),
                Some("React, Rust and PostgreSQL")
            ),
            RuntimeEnvironmentInitializationAction::None
        );
    }

    #[test]
    fn planned_workspace_image_starts_generation() {
        assert_eq!(
            runtime_environment_initialization_action(
                &json!({
                    "environment": { "status": "pending_image_build" },
                    "images": [{
                        "id": "workspace-image",
                        "service_role": "workspace",
                        "status": "planned",
                    }],
                }),
                Some("React and Rust")
            ),
            RuntimeEnvironmentInitializationAction::GenerateImage("workspace-image".to_string())
        );
    }

    #[test]
    fn missing_pending_or_failed_runtime_environment_starts_analysis() {
        for current in [
            json!({}),
            json!({ "environment": { "status": "pending" } }),
            json!({ "environment": { "status": "failed" } }),
            json!({ "environment": { "status": "not_runnable" } }),
        ] {
            assert_eq!(
                runtime_environment_initialization_action(&current, Some("React and Rust")),
                RuntimeEnvironmentInitializationAction::Analyze
            );
        }
    }
}
