// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, State};
use axum::Json;
use chatos_project_execution::{
    add_requirement_work_item_dependencies, append_planning_feedback,
    build_requirement_execution_planner_prompt, build_requirement_execution_user_message,
    collect_requirement_execution_scope, executing_requirement_ids,
    format_planning_feedback_history, missing_project_task_ids, read_planning_feedback_history,
    select_pending_work_items, sort_work_items_for_planning, topological_work_item_order,
    validate_requirement_prerequisites, ExecutionPlanIdentity, ExecutionPlane, RequirementPlanItem,
    WorkItemPlanItem, NEXT_ACTION_PREVIEW_AND_CONFIRM, STATUS_PLANNING, STATUS_PLANNING_STARTED,
    STATUS_STOPPED, STATUS_STOPPING,
};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

use crate::local_runtime::chat::{execute_chat_turn, LocalChatSendRequest};
use crate::local_runtime::project_management::{
    LocalDependencyGraph, LocalProjectPlanSnapshot, UpdateLocalRequirementInput,
    UpdateLocalWorkItemInput,
};
use crate::local_runtime::storage::{
    AppendLocalMessageInput, BeginLocalTurnInput, BeginLocalTurnResult, CreateLocalSessionInput,
};
use crate::LocalRuntime;

use super::super::super::context::owner_context;
use super::super::super::error::LocalRuntimeApiError;
use super::rerun::{
    cleanup_replaced_local_execution_batch, resolve_local_execution_batch_state, source_status,
    source_status_is_stopped_terminal, validate_source_scope, LocalExecutionBatchState,
};
use super::ExecuteRequirementPayload;

pub(in crate::local_runtime::api::task_runs) async fn execute_requirement(
    Path((project_id, requirement_id)): Path<(String, String)>,
    State(runtime): State<LocalRuntime>,
    Json(payload): Json<ExecuteRequirementPayload>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let mut replacement_identity = ExecutionPlanIdentity::optional(
        payload.replaces_execution_group_id.as_deref(),
        payload.replaces_conversation_id.as_deref(),
    )
    .map_err(|message| {
        LocalRuntimeApiError::bad_request("local_execution_plan_identity_incomplete", message)
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
    let mut previous_planning_feedback = Vec::new();
    if let Some(identity) = replacement_identity.clone() {
        let messages = database
            .list_turn_messages(
                owner.owner_user_id.as_str(),
                identity.execution_group_id.as_str(),
            )
            .await?;
        if let Some(source) = messages.iter().find(|message| message.role == "user") {
            if source.session_id != identity.conversation_id {
                return Err(LocalRuntimeApiError::conflict(
                    "local_execution_plan_conversation_mismatch",
                    "The replaced local execution plan belongs to another conversation",
                ));
            }
            let metadata = source
                .metadata_json
                .as_deref()
                .and_then(|value| serde_json::from_str::<Value>(value).ok())
                .unwrap_or_else(|| json!({}));
            previous_planning_feedback =
                read_planning_feedback_history(metadata.get("project_requirement_execution"));
            validate_source_scope(&metadata, project_id.as_str(), requirement_id.as_str())?;
            let source_status = source_status(&metadata);
            let old_runs = database
                .list_local_execution_group_task_runs(
                    owner.owner_user_id.as_str(),
                    project_id.as_str(),
                    identity.conversation_id.as_str(),
                    identity.execution_group_id.as_str(),
                )
                .await?;
            match resolve_local_execution_batch_state(
                source_status.as_str(),
                old_runs.iter().map(|run| run.status.as_str()),
            ) {
                LocalExecutionBatchState::ReplacementReady => {
                    if source_status == STATUS_STOPPING
                        || !source_status_is_stopped_terminal(source_status.as_str())
                    {
                        database
                            .set_turn_task_runner_status(
                                owner.owner_user_id.as_str(),
                                identity.execution_group_id.as_str(),
                                STATUS_STOPPED,
                                STATUS_STOPPED,
                            )
                            .await?;
                    }
                }
                LocalExecutionBatchState::CancellationSettling(_) => {
                    return Err(LocalRuntimeApiError::conflict(
                        "local_execution_replan_has_active_runs",
                        "The previous local execution batch is still stopping",
                    ));
                }
                LocalExecutionBatchState::NotStopped => {
                    return Err(LocalRuntimeApiError::conflict(
                        "local_execution_replan_requires_stopped_batch",
                        "Cancel or stop the previous local execution batch before replanning",
                    ));
                }
            }
        } else {
            replacement_identity = None;
        }
    }

    let snapshot = database
        .local_project_plan(owner.owner_user_id.as_str(), project_id.as_str(), false)
        .await?;
    let requirements = execution_requirements(&snapshot);
    let root_requirement = requirements
        .iter()
        .find(|requirement| requirement.id == requirement_id)
        .cloned()
        .ok_or_else(|| {
            LocalRuntimeApiError::not_found(
                "local_requirement_not_found",
                "Local requirement was not found",
            )
        })?;
    let requirement_dependencies = dependency_map(&snapshot.dependency_graph, "requirement");
    let requirement_scope = collect_requirement_execution_scope(
        requirements.as_slice(),
        requirement_id.as_str(),
        &requirement_dependencies,
        payload.include_prerequisite_dependents,
    );
    validate_requirement_prerequisites(
        requirements.as_slice(),
        &requirement_scope,
        &requirement_dependencies,
    )
    .map_err(|message| {
        LocalRuntimeApiError::bad_request("local_requirement_prerequisite_incomplete", message)
    })?;

    let all_work_items = execution_work_items(&snapshot);
    let mut selected_work_items =
        select_pending_work_items(all_work_items.as_slice(), &requirement_scope);
    if selected_work_items.is_empty() {
        return Err(LocalRuntimeApiError::bad_request(
            "local_requirement_has_no_pending_tasks",
            "This local requirement execution scope has no pending work items",
        ));
    }
    let mut work_item_dependencies = dependency_map(&snapshot.dependency_graph, "work_item");
    add_requirement_work_item_dependencies(
        &mut work_item_dependencies,
        selected_work_items.as_slice(),
        &requirement_dependencies,
        &requirement_scope,
    );
    let creation_order =
        topological_work_item_order(selected_work_items.as_slice(), &work_item_dependencies)
            .map_err(|message| {
                LocalRuntimeApiError::bad_request("local_project_task_dependency_cycle", message)
            })?;
    sort_work_items_for_planning(selected_work_items.as_mut_slice());
    let executing_requirement_ids =
        executing_requirement_ids(root_requirement.id.as_str(), selected_work_items.as_slice());
    ensure_no_active_runs(
        database,
        owner.owner_user_id.as_str(),
        project_id.as_str(),
        &executing_requirement_ids,
    )
    .await?;

    let model_config_id = payload
        .model_config_id
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or(project_agent_model_id(&runtime).await?);
    let planning_feedback = payload
        .planning_feedback
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let planning_feedback_history = append_planning_feedback(
        previous_planning_feedback.as_slice(),
        planning_feedback.as_deref(),
    );
    let planning_feedback_context =
        format_planning_feedback_history(planning_feedback_history.as_slice());
    let documents = load_documents(
        database,
        owner.owner_user_id.as_str(),
        project_id.as_str(),
        &requirement_scope,
    )
    .await?;
    let session = database
        .create_session(CreateLocalSessionInput {
            project_id: project.project_id.clone(),
            owner_user_id: owner.owner_user_id.clone(),
            title: format!("执行需求：{}", root_requirement.title),
            selected_model_id: Some(model_config_id.clone()),
            selected_agent_id: None,
        })
        .await?;
    let planner_prompt = build_requirement_execution_planner_prompt(
        ExecutionPlane::LocalConnector,
        project_id.as_str(),
        &root_requirement,
        requirements.as_slice(),
        &requirement_scope,
        all_work_items.as_slice(),
        selected_work_items.as_slice(),
        creation_order.as_slice(),
        &work_item_dependencies,
        &documents,
        Some(model_config_id.as_str()),
        planning_feedback_context.as_deref(),
    )
    .map_err(|message| {
        LocalRuntimeApiError::bad_request("local_project_task_dependency_invalid", message)
    })?;
    let mut visible_message =
        build_requirement_execution_user_message(&root_requirement, selected_work_items.as_slice());
    if let Some(feedback) = planning_feedback_context.as_deref() {
        visible_message.push_str("\n\n执行计划调整要求（按提交顺序，全部保留）：\n");
        visible_message.push_str(feedback);
    }
    for scoped_requirement_id in &executing_requirement_ids {
        database
            .update_local_requirement(
                owner.owner_user_id.as_str(),
                scoped_requirement_id.as_str(),
                UpdateLocalRequirementInput {
                    status: Some("reviewing".to_string()),
                    ..Default::default()
                },
            )
            .await?;
    }

    let execution_group_id = format!("lc_execution_group_{}", Uuid::new_v4());
    let planner_user_metadata = json!({
        "hidden": true,
        "conversation_turn_id": execution_group_id,
        "model_config_id": model_config_id,
        "runtime_origin": "local_device",
        "project_requirement_execution": {
            "project_id": project_id,
            "requirement_id": root_requirement.id,
            "requirement_title": root_requirement.title,
            "execution_group_id": execution_group_id,
            "project_task_ids": selected_work_items
                .iter()
                .map(|item| item.id.clone())
                .collect::<Vec<_>>(),
            "execution_plane": ExecutionPlane::LocalConnector.as_str(),
            "planning_feedback": planning_feedback,
            "planning_feedback_history": planning_feedback_history,
            "replaced_execution_group_id": replacement_identity
                .as_ref()
                .map(|identity| identity.execution_group_id.as_str()),
            "replaced_conversation_id": replacement_identity
                .as_ref()
                .map(|identity| identity.conversation_id.as_str()),
            "include_prerequisite_dependents": payload.include_prerequisite_dependents,
        },
        "task_runner_async": {
            "mode": "project_requirement_execution",
            "overall_status": STATUS_PLANNING,
            "confirmation_status": STATUS_PLANNING,
            "source": "project_requirement_execute_button",
            "project_id": project_id,
            "requirement_id": root_requirement.id,
            "created_task_ids": [],
            "running_task_ids": [],
            "terminal_task_ids": [],
        }
    });
    let begin_result = database
        .begin_turn(BeginLocalTurnInput {
            session_id: session.id.clone(),
            owner_user_id: owner.owner_user_id.clone(),
            turn_id: execution_group_id.clone(),
            idempotency_key: execution_group_id.clone(),
            content: visible_message.clone(),
            metadata_json: Some(planner_user_metadata.to_string()),
        })
        .await;
    let begin_result = match begin_result {
        Ok(begin_result) => begin_result,
        Err(error) => {
            for scoped_requirement_id in &executing_requirement_ids {
                let _ = database
                    .update_local_requirement(
                        owner.owner_user_id.as_str(),
                        scoped_requirement_id.as_str(),
                        UpdateLocalRequirementInput {
                            status: Some("approved".to_string()),
                            ..Default::default()
                        },
                    )
                    .await;
            }
            return Err(error.into());
        }
    };
    let user_message_id = match begin_result {
        BeginLocalTurnResult::Started(snapshot) => snapshot.user_message.id,
        BeginLocalTurnResult::Existing(_) => {
            for scoped_requirement_id in &executing_requirement_ids {
                let _ = database
                    .update_local_requirement(
                        owner.owner_user_id.as_str(),
                        scoped_requirement_id.as_str(),
                        UpdateLocalRequirementInput {
                            status: Some("approved".to_string()),
                            ..Default::default()
                        },
                    )
                    .await;
            }
            return Err(LocalRuntimeApiError::conflict(
                "local_execution_group_conflict",
                "The local execution planning turn already exists",
            ));
        }
    };
    let runtime_for_planner = runtime.clone();
    let owner_user_id = owner.owner_user_id.clone();
    let project_id_for_planner = project_id.clone();
    let session_id = session.id.clone();
    let model_id_for_planner = model_config_id.clone();
    let selected_for_recovery = selected_work_items.clone();
    let scope_for_recovery = executing_requirement_ids.clone();
    let group_for_planner = execution_group_id.clone();
    let visible_message_for_planner = visible_message.clone();
    let project_for_recovery = project.clone();
    let replacement_for_recovery = replacement_identity.clone();
    tokio::spawn(async move {
        let result = execute_chat_turn(
            &runtime_for_planner,
            owner_user_id.as_str(),
            LocalChatSendRequest {
                conversation_id: session_id.clone(),
                content: visible_message_for_planner,
                turn_id: Some(group_for_planner.clone()),
                idempotency_key: Some(group_for_planner.clone()),
                model_config_id: Some(model_id_for_planner),
                reasoning_enabled: None,
                project_requirement_execution_planner: true,
                resume_precreated_turn: true,
                project_requirement_execution_task_ids: selected_for_recovery
                    .iter()
                    .map(|item| item.id.clone())
                    .collect(),
                project_requirement_execution_requirement_id: Some(root_requirement.id.clone()),
                project_requirement_execution_requirement_title: Some(
                    root_requirement.title.clone(),
                ),
                system_prompt: Some(planner_prompt),
                attachments: Vec::new(),
                ai_model_config: Default::default(),
            },
        )
        .await;
        let planner_error = result.err().map(|error| (error.code, error.message));
        if let Some((code, message)) = planner_error.as_ref() {
            if let Ok(database) = runtime_for_planner.local_database() {
                let _ = database
                    .fail_turn(
                        owner_user_id.as_str(),
                        group_for_planner.as_str(),
                        code,
                        message.as_str(),
                    )
                    .await;
            }
        }
        reconcile_local_planner_outcome(
            &runtime_for_planner,
            owner_user_id.as_str(),
            project_id_for_planner.as_str(),
            session_id.as_str(),
            group_for_planner.as_str(),
            selected_for_recovery.as_slice(),
            &scope_for_recovery,
            &project_for_recovery,
            replacement_for_recovery.as_ref(),
            planner_error.map(|(_, message)| message),
        )
        .await;
    });

    Ok(Json(json!({
        "success": true,
        "status": STATUS_PLANNING_STARTED,
        "next_action": NEXT_ACTION_PREVIEW_AND_CONFIRM,
        "execution_plane": ExecutionPlane::LocalConnector.as_str(),
        "project_id": project_id,
        "requirement_id": requirement_id,
        "conversation_id": session.id,
        "message_id": user_message_id,
        "message": null,
        "execution_group_id": execution_group_id,
        "model_config_id": model_config_id,
        "include_prerequisite_dependents": payload.include_prerequisite_dependents,
        "planning_feedback": planning_feedback,
        "planning_feedback_history": planning_feedback_history,
        "confirmation_status": STATUS_PLANNING_STARTED,
        "has_started_runs": false,
        "planner_agent_key": "project_requirement_execution_planner_agent",
        "plan_mode_enabled": false,
    })))
}

pub(super) fn execution_requirements(
    snapshot: &LocalProjectPlanSnapshot,
) -> Vec<RequirementPlanItem> {
    snapshot
        .requirements
        .iter()
        .map(|item| RequirementPlanItem {
            id: item.id.clone(),
            title: item.title.clone(),
            status: item.status.trim().to_ascii_lowercase(),
            parent_requirement_id: item.parent_requirement_id.clone(),
        })
        .collect()
}

pub(super) fn execution_work_items(snapshot: &LocalProjectPlanSnapshot) -> Vec<WorkItemPlanItem> {
    snapshot
        .work_items
        .iter()
        .map(|item| WorkItemPlanItem {
            id: item.id.clone(),
            requirement_id: item.requirement_id.clone(),
            title: item.title.clone(),
            description: item.description.clone(),
            status: item.status.trim().to_ascii_lowercase(),
            priority: item.priority,
            tags: item.tags.clone(),
            is_planning_task: item.is_planning_task,
        })
        .collect()
}

pub(super) fn dependency_map(
    graph: &LocalDependencyGraph,
    node_type: &str,
) -> BTreeMap<String, Vec<String>> {
    let prefix = format!("{node_type}:");
    let mut dependencies = BTreeMap::<String, Vec<String>>::new();
    for edge in &graph.edges {
        let Some(prerequisite_id) = edge.from.strip_prefix(prefix.as_str()) else {
            continue;
        };
        let Some(target_id) = edge.to.strip_prefix(prefix.as_str()) else {
            continue;
        };
        dependencies
            .entry(target_id.to_string())
            .or_default()
            .push(prerequisite_id.to_string());
    }
    for values in dependencies.values_mut() {
        values.sort();
        values.dedup();
    }
    dependencies
}

async fn load_documents(
    database: &crate::local_runtime::LocalDatabase,
    owner_user_id: &str,
    project_id: &str,
    requirement_scope: &BTreeSet<String>,
) -> Result<BTreeMap<String, Value>, LocalRuntimeApiError> {
    let mut documents = BTreeMap::new();
    for requirement_id in requirement_scope {
        let records = database
            .list_local_requirement_documents(owner_user_id, project_id, requirement_id.as_str())
            .await?;
        documents.insert(
            requirement_id.clone(),
            serde_json::to_value(records).unwrap_or_else(|_| json!([])),
        );
    }
    Ok(documents)
}

async fn ensure_no_active_runs(
    database: &crate::local_runtime::LocalDatabase,
    owner_user_id: &str,
    project_id: &str,
    requirement_scope: &BTreeSet<String>,
) -> Result<(), LocalRuntimeApiError> {
    for requirement_id in requirement_scope {
        if database
            .get_local_requirement(owner_user_id, requirement_id.as_str())
            .await?
            .is_some_and(|requirement| requirement.status == "reviewing")
        {
            return Err(LocalRuntimeApiError::conflict(
                "local_requirement_execution_planning",
                "This local requirement already has a task graph being generated or awaiting confirmation",
            ));
        }
    }
    let has_unconfirmed_plan = database
        .list_local_project_execution_tasks(owner_user_id, project_id)
        .await?
        .into_iter()
        .any(|task| {
            task.requirement_id
                .as_deref()
                .is_some_and(|id| requirement_scope.contains(id))
                && task.last_run_id.is_none()
                && matches!(task.status.as_str(), "todo" | "doing")
        });
    if has_unconfirmed_plan {
        return Err(LocalRuntimeApiError::conflict(
            "local_requirement_execution_awaiting_confirmation",
            "This local requirement already has a generated task graph awaiting confirmation",
        ));
    }
    for requirement_id in requirement_scope {
        let runs = database
            .list_local_requirement_task_runs(owner_user_id, project_id, requirement_id.as_str())
            .await?;
        if runs
            .iter()
            .any(|run| matches!(run.status.as_str(), "queued" | "running"))
        {
            return Err(LocalRuntimeApiError::conflict(
                "local_requirement_execution_active",
                "This local requirement execution scope already has active task runs",
            ));
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn reconcile_local_planner_outcome(
    runtime: &LocalRuntime,
    owner_user_id: &str,
    project_id: &str,
    session_id: &str,
    execution_group_id: &str,
    selected_work_items: &[WorkItemPlanItem],
    requirement_scope: &BTreeSet<String>,
    project: &crate::local_runtime::storage::LocalProjectRecord,
    replacement_identity: Option<&ExecutionPlanIdentity>,
    planner_error: Option<String>,
) {
    let Ok(database) = runtime.local_database() else {
        return;
    };
    let stopped = database
        .list_turn_messages(owner_user_id, execution_group_id)
        .await
        .unwrap_or_default()
        .into_iter()
        .find(|message| message.role == "user")
        .and_then(|message| message.metadata_json)
        .and_then(|value| serde_json::from_str::<Value>(value.as_str()).ok())
        .and_then(|metadata| {
            metadata
                .get("task_runner_async")
                .and_then(|value| value.get("overall_status"))
                .and_then(Value::as_str)
                .map(|value| value == "stopped")
        })
        .unwrap_or(false);
    if stopped {
        return;
    }
    let tasks = database
        .list_local_conversation_tasks(owner_user_id, session_id, 200)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|task| task.execution_group_id.as_deref() == Some(execution_group_id))
        .collect::<Vec<_>>();
    let linked_project_task_ids = tasks
        .iter()
        .filter_map(|task| task.project_work_item_id.clone())
        .collect::<BTreeSet<_>>();
    let missing = missing_project_task_ids(selected_work_items, &linked_project_task_ids);
    if missing.is_empty() {
        if let Some(identity) = replacement_identity {
            if let Err(error) =
                cleanup_replaced_local_execution_batch(runtime, owner_user_id, project, identity)
                    .await
            {
                let current_identity = ExecutionPlanIdentity {
                    execution_group_id: execution_group_id.to_string(),
                    conversation_id: session_id.to_string(),
                };
                let _ = cleanup_replaced_local_execution_batch(
                    runtime,
                    owner_user_id,
                    project,
                    &current_identity,
                )
                .await;
                let _ = database
                    .set_turn_task_runner_status(
                        owner_user_id,
                        execution_group_id,
                        "failed",
                        "failed",
                    )
                    .await;
                let _ = database
                    .append_turn_result_message(AppendLocalMessageInput {
                        session_id: session_id.to_string(),
                        owner_user_id: owner_user_id.to_string(),
                        turn_id: execution_group_id.to_string(),
                        message_id: None,
                        role: "assistant".to_string(),
                        content: format!(
                            "新的本地执行流程已经生成，但旧批次资源清理失败，因此没有切换为可执行状态：{error:?}"
                        ),
                        reasoning: None,
                        tool_calls_json: None,
                        tool_call_id: None,
                        metadata_json: Some(json!({
                            "hidden": true,
                            "runtime_origin": "local_device",
                            "message_mode": "project_requirement_execution_cleanup_error",
                            "execution_plane": ExecutionPlane::LocalConnector.as_str(),
                            "project_id": project_id,
                        }).to_string()),
                        created_at: None,
                    })
                    .await;
            }
        }
        return;
    }

    let _ = database
        .set_turn_task_runner_status(owner_user_id, execution_group_id, "failed", "failed")
        .await;

    for item in selected_work_items {
        if linked_project_task_ids.contains(item.id.as_str()) {
            continue;
        }
        let _ = database
            .update_local_work_item(
                owner_user_id,
                item.id.as_str(),
                UpdateLocalWorkItemInput {
                    status: Some("ready".to_string()),
                    ..Default::default()
                },
            )
            .await;
    }
    for requirement_id in requirement_scope {
        let _ = database
            .update_local_requirement(
                owner_user_id,
                requirement_id.as_str(),
                UpdateLocalRequirementInput {
                    status: Some("approved".to_string()),
                    ..Default::default()
                },
            )
            .await;
    }
    let detail =
        planner_error.unwrap_or_else(|| format!("规划 Agent 未覆盖 {} 个项目任务", missing.len()));
    let _ = database
        .append_turn_result_message(AppendLocalMessageInput {
            session_id: session_id.to_string(),
            owner_user_id: owner_user_id.to_string(),
            turn_id: execution_group_id.to_string(),
            message_id: None,
            role: "assistant".to_string(),
            content: format!(
                "本地需求执行规划未完整生成执行任务。未覆盖的项目任务已恢复为就绪状态；本次没有创建任何云端任务。详情：{detail}"
            ),
            reasoning: None,
            tool_calls_json: None,
            tool_call_id: None,
            metadata_json: Some(json!({
                "hidden": true,
                "runtime_origin": "local_device",
                "message_mode": "project_requirement_execution_error",
                "execution_plane": ExecutionPlane::LocalConnector.as_str(),
                "project_id": project_id,
            }).to_string()),
            created_at: None,
        })
        .await;
}

async fn project_agent_model_id(runtime: &LocalRuntime) -> Result<String, LocalRuntimeApiError> {
    runtime
        .state
        .read()
        .await
        .model_configs
        .settings
        .project_management_agent_model_config_id
        .clone()
        .ok_or_else(|| {
            LocalRuntimeApiError::conflict(
                "local_task_runner_model_required",
                "Configure the Project Management Agent model in Local Connector first",
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local_runtime::project_management::{
        LocalDependencyGraphEdge, LocalDependencyGraphNode,
    };

    #[test]
    fn local_dependency_map_uses_only_requested_node_type() {
        let graph = LocalDependencyGraph {
            root_id: None,
            nodes: Vec::<LocalDependencyGraphNode>::new(),
            edges: vec![
                LocalDependencyGraphEdge {
                    from: "requirement:req-a".to_string(),
                    to: "requirement:req-b".to_string(),
                    edge_type: "blocks".to_string(),
                },
                LocalDependencyGraphEdge {
                    from: "work_item:task-a".to_string(),
                    to: "work_item:task-b".to_string(),
                    edge_type: "blocks".to_string(),
                },
            ],
            blocked_by: Vec::new(),
            ready: true,
        };
        assert_eq!(
            dependency_map(&graph, "requirement"),
            BTreeMap::from([("req-b".to_string(), vec!["req-a".to_string()])])
        );
    }
}
