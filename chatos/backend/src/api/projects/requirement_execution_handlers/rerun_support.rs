// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};

use chatos_project_execution::{
    missing_project_task_ids, validate_exact_project_task_scope, ExecutionPlanIdentity,
    STATUS_STOPPED, STATUS_STOPPING,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::Config;
use crate::core::auth::AuthUser;
use crate::core::messages::set_task_runner_async_overall_status_for_session;
use crate::modules::conversation_runtime::messages as conversation_messages;
use crate::services::{chatos_sessions, task_runner_api_client};

use super::super::requirement_execution::{
    apply_task_runner_task_snapshot, create_execution_planner_failure_message,
    mark_execution_messages_for_stop, mark_execution_planner_failed, sync_execution_link_status,
    sync_execution_message_task_tracking, sync_requirement_execution_state,
    task_runner_callback_event_for_status, task_runner_status_is_active,
    task_runner_status_is_cancelled, value_string, ExecutionLink, HandlerError, WorkItemPlanItem,
};
use super::execution_message_status;
use super::plan_query::execution_status_is_stopped_terminal;

pub(super) async fn load_expected_execution_project_task_ids(
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

pub(super) fn expected_execution_project_task_ids(
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

pub(super) fn expand_project_task_scope_to_actual_graph(
    expected: &BTreeSet<String>,
    actual: &BTreeSet<String>,
) -> BTreeSet<String> {
    if !actual.is_empty() && expected.is_subset(actual) {
        actual.clone()
    } else {
        expected.clone()
    }
}

pub(super) struct RerunCloneMappings {
    pub(super) dag_node_count: usize,
    pub(super) mapped_project_task_ids: BTreeSet<String>,
    pub(super) task_mappings: Vec<(String, String)>,
}

pub(super) fn parse_rerun_clone_mappings(
    clone_result: &Value,
) -> Result<RerunCloneMappings, String> {
    let mappings = clone_result
        .get("task_mappings")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let task_mappings = mappings
        .iter()
        .map(|mapping| {
            let project_task_id = value_string(mapping, "project_task_id")
                .ok_or_else(|| format!("复制执行图缺少项目任务映射: {mapping}"))?;
            let task_runner_task_id = value_string(mapping, "new_task_id")
                .ok_or_else(|| format!("复制执行图缺少新任务标识: {mapping}"))?;
            Ok((project_task_id, task_runner_task_id))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mapped_project_task_ids = task_mappings
        .iter()
        .map(|(project_task_id, _)| project_task_id.clone())
        .collect();
    Ok(RerunCloneMappings {
        dag_node_count: mappings.len(),
        mapped_project_task_ids,
        task_mappings,
    })
}

pub(super) fn started_runs_by_task_id(started_runs: &Value) -> BTreeMap<String, (String, String)> {
    started_runs
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
        .collect()
}

pub(super) async fn discard_cloned_project_execution(
    task_runner_base_url: &str,
    project_id: &str,
    requirement_id: &str,
    session_id: &str,
    execution_group_id: &str,
) {
    let _ = task_runner_api_client::retire_project_execution(
        task_runner_base_url,
        project_id,
        requirement_id,
        session_id,
        execution_group_id,
    )
    .await;
}

pub(super) fn validate_rerun_cloned_project_task_scope(
    expected_project_task_ids: &BTreeSet<String>,
    mapped_project_task_ids: &BTreeSet<String>,
    cloned_dag_node_count: usize,
) -> Result<(), String> {
    if cloned_dag_node_count == 0 {
        return Err(format!(
            "expected {} project tasks, cloned 0 DAG nodes",
            expected_project_task_ids.len()
        ));
    }
    validate_exact_project_task_scope(expected_project_task_ids, mapped_project_task_ids).map_err(
        |mismatch| {
            format!(
                "project task scope mismatch: {mismatch}; expected_project_tasks={}, cloned_project_tasks={}, cloned_dag_nodes={}",
                expected_project_task_ids.len(),
                mapped_project_task_ids.len(),
                cloned_dag_node_count,
            )
        },
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn ensure_old_cloud_execution_batch_ready_for_replacement(
    old_message: &crate::models::message::Message,
    task_runner_base_url: &str,
    task_runner_agent_token: &str,
    user_access_token: &str,
    project_sync_secret: &str,
    conversation_id: &str,
    execution_group_id: &str,
    requirement_title: &str,
    not_stopped_message: &str,
    links: &mut [ExecutionLink],
) -> Result<(), HandlerError> {
    for link in links.iter_mut() {
        let task = task_runner_api_client::get_task_runner_task(
            task_runner_base_url,
            task_runner_agent_token,
            link.task_runner_task_id.as_str(),
        )
        .await
        .map_err(|err| HandlerError::bad_gateway("校验旧执行批次 Task Runner 状态失败", err))?;
        apply_task_runner_task_snapshot(link, &task);
        sync_execution_link_status(
            project_sync_secret,
            link,
            task.status.as_str(),
            task_runner_callback_event_for_status(task.status.as_str()),
        )
        .await?;
    }
    sync_execution_message_task_tracking(conversation_id, execution_group_id, links).await?;

    let active_links = links
        .iter()
        .filter(|link| task_runner_status_is_active(link.task_runner_status.as_deref()))
        .cloned()
        .collect::<Vec<_>>();
    if execution_batch_has_started_active_tasks(links) {
        return Err(HandlerError::conflict(not_stopped_message));
    }
    if active_links.is_empty() {
        return match resolve_old_cloud_execution_batch_state(old_message, links) {
            OldCloudExecutionBatchState::ReplacementReady => {
                let status = execution_message_status(old_message);
                if status == STATUS_STOPPING
                    || !execution_status_is_stopped_terminal(status.as_str())
                {
                    set_task_runner_async_overall_status_for_session(
                        conversation_id,
                        execution_group_id,
                        STATUS_STOPPED,
                    )
                    .await
                    .map_err(|error| HandlerError::internal("收敛旧执行批次停止状态失败", error))?;
                    sync_execution_message_task_tracking(
                        conversation_id,
                        execution_group_id,
                        links,
                    )
                    .await?;
                }
                Ok(())
            }
            OldCloudExecutionBatchState::CancellationSettling(_) => unreachable!(
                "active link count was already checked before resolving old batch state"
            ),
            OldCloudExecutionBatchState::NotStopped => {
                Err(HandlerError::bad_request(not_stopped_message))
            }
        };
    }
    mark_execution_messages_for_stop(active_links.as_slice(), STATUS_STOPPING).await;
    let _ = set_task_runner_async_overall_status_for_session(
        conversation_id,
        execution_group_id,
        STATUS_STOPPING,
    )
    .await;

    let mut cancel_errors = Vec::new();
    for link in &active_links {
        let cancel_result = task_runner_api_client::cancel_task_runner_task(
            task_runner_base_url,
            task_runner_agent_token,
            Some(user_access_token),
            link.task_runner_task_id.as_str(),
            &task_runner_api_client::CancelTaskRunnerTaskRequest {
                reason: format!("重新执行前继续取消旧需求执行：{requirement_title}"),
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
                if let Err(error) = sync_execution_link_status(
                    project_sync_secret,
                    link,
                    status.as_str(),
                    task_runner_callback_event_for_status(status.as_str())
                        .or(Some("task.cancelled")),
                )
                .await
                {
                    cancel_errors.push(format!("{}: {}", link.task_runner_task_id, error.error));
                }
            }
            Err(error) => cancel_errors.push(format!("{}: {}", link.task_runner_task_id, error)),
        }
    }
    if !cancel_errors.is_empty() {
        return Err(HandlerError::bad_gateway(
            "旧执行批次仍有运行中任务，且重新发送取消请求失败",
            cancel_errors.join("；"),
        ));
    }
    Err(HandlerError::conflict(format!(
        "旧执行批次仍有 {} 个 Task Runner 任务正在取消，已重新发送取消请求，请等待取消完成后再重新执行。",
        active_links.len()
    )))
}

pub(super) fn execution_batch_has_started_active_tasks(links: &[ExecutionLink]) -> bool {
    links.iter().any(|link| {
        link.task_runner_run_id
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
    }) && links
        .iter()
        .any(|link| task_runner_status_is_active(link.task_runner_status.as_deref()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OldCloudExecutionBatchState {
    ReplacementReady,
    CancellationSettling(usize),
    NotStopped,
}

pub(super) fn resolve_old_cloud_execution_batch_state(
    message: &crate::models::message::Message,
    links: &[ExecutionLink],
) -> OldCloudExecutionBatchState {
    let active_count = links
        .iter()
        .filter(|link| task_runner_status_is_active(link.task_runner_status.as_deref()))
        .count();
    if active_count > 0 {
        return OldCloudExecutionBatchState::CancellationSettling(active_count);
    }

    let message_status = execution_message_status(message);
    if execution_status_is_stopped_terminal(message_status.as_str())
        || message_status == STATUS_STOPPING
        || inactive_links_record_a_cancelled_batch(links)
    {
        OldCloudExecutionBatchState::ReplacementReady
    } else {
        OldCloudExecutionBatchState::NotStopped
    }
}

fn inactive_links_record_a_cancelled_batch(links: &[ExecutionLink]) -> bool {
    !links.is_empty()
        && links
            .iter()
            .all(|link| !task_runner_status_is_active(link.task_runner_status.as_deref()))
        && links
            .iter()
            .any(|link| task_runner_status_is_cancelled(link.task_runner_status.as_deref()))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct RequirementPlannerRecovery {
    #[serde(default)]
    pub(super) agent_run_error: Option<String>,
    #[serde(default)]
    pub(super) agent_run_status: Option<String>,
    pub(super) kind: String,
    pub(super) execution_group_id: String,
    pub(super) executing_requirement_ids: BTreeSet<String>,
    pub(super) link_scope_work_items: Vec<WorkItemPlanItem>,
    pub(super) project_id: String,
    pub(super) replacement_identity: Option<ExecutionPlanIdentity>,
    pub(super) replacement_work_items: Vec<WorkItemPlanItem>,
    pub(super) requirement_id: String,
    pub(super) selected_work_items: Vec<WorkItemPlanItem>,
    pub(super) session_id: String,
}

pub(super) fn replacement_link_scope(
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

pub(super) async fn reconcile_requirement_planner_outcome(
    recovery: RequirementPlannerRecovery,
) -> Result<(), HandlerError> {
    if recovery.kind != "requirement_planner" {
        return Err(HandlerError::bad_request(
            "invalid requirement planner owner context",
        ));
    }
    let config =
        Config::try_get().map_err(|error| HandlerError::internal("配置未初始化", error))?;
    let project_sync_secret = config
        .project_service_sync_secret
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            HandlerError::internal(
                "项目执行需要配置项目管理同步密钥",
                "CHATOS_PROJECT_SERVICE_INTERNAL_API_SECRET is required from configuration center",
            )
        })?;
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
    let links =
        load_execution_links_for_recovery(project_sync_secret, link_scope_work_items.as_slice())
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
    if !current_execution_links.is_empty() {
        sync_execution_message_task_tracking(
            recovery.session_id.as_str(),
            recovery.execution_group_id.as_str(),
            current_execution_links.as_slice(),
        )
        .await?;
    }
    if recovery
        .agent_run_status
        .as_deref()
        .is_some_and(|status| matches!(status.trim(), "failed" | "blocked"))
    {
        restore_missing_planner_work_items(&recovery, &linked_project_task_ids).await?;
        let failure_reason = build_planner_runtime_failure_message(
            recovery.agent_run_error.as_deref(),
            current_execution_links.len(),
            missing_project_task_ids.len(),
        );
        create_execution_planner_failure_message(
            recovery.session_id.as_str(),
            recovery.execution_group_id.as_str(),
            "planner_runtime_failed",
            failure_reason.clone(),
        )
        .await?;
        mark_execution_planner_failed(
            recovery.session_id.as_str(),
            recovery.execution_group_id.as_str(),
            "planner_runtime_failed",
            failure_reason.as_str(),
        )
        .await?;
        return Ok(());
    }
    if missing_project_task_ids.is_empty() {
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
            if let Err(error) = retire_cloud_execution_batch_for_recovery(
                project_sync_secret,
                recovery.project_id.as_str(),
                recovery.requirement_id.as_str(),
                identity.conversation_id.as_str(),
                identity.execution_group_id.as_str(),
                replaced_links.as_slice(),
            )
            .await
            {
                let _ = retire_cloud_execution_batch_for_recovery(
                    project_sync_secret,
                    recovery.project_id.as_str(),
                    recovery.requirement_id.as_str(),
                    recovery.session_id.as_str(),
                    recovery.execution_group_id.as_str(),
                    current_execution_links.as_slice(),
                )
                .await;
                let failure_reason = format!(
                    "新的执行流程已经生成，但旧批次任务及临时资源清理失败，因此新流程没有切换为可执行状态。请检查 Task Runner、沙箱和 Git 分支清理状态后重试。详情：{}",
                    error.error
                );
                create_execution_planner_failure_message(
                    recovery.session_id.as_str(),
                    recovery.execution_group_id.as_str(),
                    "replacement_cleanup_failed",
                    failure_reason.clone(),
                )
                .await?;
                mark_execution_planner_failed(
                    recovery.session_id.as_str(),
                    recovery.execution_group_id.as_str(),
                    "replacement_cleanup_failed",
                    failure_reason.as_str(),
                )
                .await?;
                return Err(error);
            }
        }
        return Ok(());
    }

    restore_missing_planner_work_items(&recovery, &linked_project_task_ids).await?;
    let failure_reason = build_planner_coverage_failure_message(
        recovery.selected_work_items.as_slice(),
        &linked_project_task_ids,
    );
    create_execution_planner_failure_message(
        recovery.session_id.as_str(),
        recovery.execution_group_id.as_str(),
        "planner_created_no_tasks",
        failure_reason.clone(),
    )
    .await?;
    mark_execution_planner_failed(
        recovery.session_id.as_str(),
        recovery.execution_group_id.as_str(),
        "planner_created_no_tasks",
        failure_reason.as_str(),
    )
    .await?;
    Ok(())
}

async fn restore_missing_planner_work_items(
    recovery: &RequirementPlannerRecovery,
    linked_project_task_ids: &BTreeSet<String>,
) -> Result<(), HandlerError> {
    let config =
        Config::try_get().map_err(|error| HandlerError::internal("配置未初始化", error))?;
    let project_sync_secret = config
        .project_service_sync_secret
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            HandlerError::internal(
                "项目执行需要配置项目管理同步密钥",
                "CHATOS_PROJECT_SERVICE_INTERNAL_API_SECRET is required from configuration center",
            )
        })?;
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
            project_sync_secret,
            requirement_id.as_str(),
            Some("approved"),
            missing_ids,
            Some("ready"),
            true,
        )
        .await?;
    }
    Ok(())
}

pub(super) fn build_planner_runtime_failure_message(
    error: Option<&str>,
    created_task_count: usize,
    missing_task_count: usize,
) -> String {
    let raw = error.unwrap_or_default().trim();
    let lower = raw.to_ascii_lowercase();
    let reason = if lower.contains("auth_unavailable") || lower.contains("no auth available") {
        "模型网关当前没有可用的认证资源，自动重试后仍未恢复".to_string()
    } else if lower.contains("unexpected eof")
        || lower.contains("response body failed")
        || lower.contains("响应解析异常")
        || lower.contains("传输或解码过程中中断")
    {
        "模型流式响应在传输过程中被中断，自动重试后仍未恢复".to_string()
    } else if raw.is_empty() || raw.eq_ignore_ascii_case("ChatOS Cloud Agent failed") {
        "模型规划运行异常，自动重试后仍未恢复".to_string()
    } else {
        raw.to_string()
    };
    format!(
        "执行计划生成失败：{reason}。本批次已停止；已创建 {created_task_count} 个执行节点，仍缺少 {missing_task_count} 个，未创建节点的项目任务已恢复为就绪状态，请重新生成。"
    )
}

pub(crate) async fn reconcile_requirement_planner_owner_context(
    context: Value,
) -> Result<(), HandlerError> {
    let recovery = serde_json::from_value::<RequirementPlannerRecovery>(context)
        .map_err(|error| HandlerError::bad_request(format!("invalid planner recovery: {error}")))?;
    reconcile_requirement_planner_outcome(recovery).await
}

async fn load_execution_links_for_recovery(
    project_sync_secret: &str,
    work_items: &[WorkItemPlanItem],
) -> Result<Vec<ExecutionLink>, HandlerError> {
    let values = crate::services::project_management_api_client::sync_list_execution_links(
        project_sync_secret,
        work_items.iter().map(|item| item.id.clone()).collect(),
    )
    .await
    .map_err(|error| HandlerError::bad_gateway("读取项目任务执行关联失败", error))?;
    Ok(values
        .into_iter()
        .filter_map(|value| {
            Some(ExecutionLink {
                link_id: value_string(&value, "id"),
                work_item_id: value_string(&value, "work_item_id")?,
                task_runner_task_id: value_string(&value, "task_runner_task_id")?,
                task_runner_run_id: value_string(&value, "task_runner_run_id"),
                task_runner_status: value_string(&value, "task_runner_status"),
                source_session_id: value_string(&value, "source_session_id"),
                source_user_message_id: value_string(&value, "source_user_message_id"),
            })
        })
        .collect())
}

async fn retire_cloud_execution_batch_for_recovery(
    project_sync_secret: &str,
    project_id: &str,
    requirement_id: &str,
    source_session_id: &str,
    source_user_message_id: &str,
    links: &[ExecutionLink],
) -> Result<Value, HandlerError> {
    let config =
        Config::try_get().map_err(|error| HandlerError::internal("配置未初始化", error))?;
    let retired = task_runner_api_client::retire_project_execution(
        config.task_runner_base_url.as_str(),
        project_id,
        requirement_id,
        source_session_id,
        source_user_message_id,
    )
    .await
    .map_err(|error| HandlerError::bad_gateway("回收旧 Task Runner 执行批次失败", error))?;
    let link_identities = links
        .iter()
        .filter_map(|link| {
            Some(
                crate::services::project_management_api_client::SyncExecutionLinkIdentity {
                    work_item_id: link.work_item_id.clone(),
                    link_id: link.link_id.clone()?,
                },
            )
        })
        .collect::<Vec<_>>();
    let deleted = crate::services::project_management_api_client::sync_delete_execution_links(
        project_sync_secret,
        link_identities,
    )
    .await
    .map_err(|error| HandlerError::bad_gateway("清理旧项目任务执行关联失败", error))?;
    Ok(serde_json::json!({
        "task_runner": retired,
        "project_management": deleted,
    }))
}

pub(super) fn build_planner_coverage_failure_message(
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
