// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

use crate::config::Config;
use crate::core::messages::message_turn_id;
use crate::core::time::now_rfc3339;
use crate::models::message::Message;
use crate::models::session::Session;
use crate::services::{project_management_api_client, task_runner_api_client};

use super::errors::HandlerError;
use super::status::{
    is_done_status, project_work_item_status_is_active, task_runner_callback_event_for_status,
    task_runner_status_is_active,
};
use super::sync::{load_execution_links_for_work_items, sync_execution_link_status};
use super::types::{
    CreatedExecutionTask, ExecutionLink, RequirementPlanItem, SelectedContactRuntime,
    WorkItemPlanItem,
};
use super::values::{normalize_tags, value_string};

pub(in crate::api::projects) async fn create_and_start_execution_tasks(
    cfg: &Config,
    project_sync_secret: &str,
    user_access_token: &str,
    contact_runtime: &SelectedContactRuntime,
    session: &Session,
    message: &Message,
    project_id: &str,
    work_items: &[WorkItemPlanItem],
    creation_order: &[String],
    dependency_map: &BTreeMap<String, Vec<String>>,
    external_prerequisite_task_ids: &BTreeMap<String, Vec<String>>,
    execution_options: &task_runner_api_client::TaskRunnerExecutionOptions,
    builtin_prompt_locale: &str,
) -> Result<Vec<CreatedExecutionTask>, HandlerError> {
    let work_item_ids = work_items
        .iter()
        .map(|item| item.id.clone())
        .collect::<BTreeSet<_>>();
    let by_id = work_items
        .iter()
        .map(|item| (item.id.clone(), item.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut created_by_work_item = BTreeMap::<String, CreatedExecutionTask>::new();

    for work_item_id in creation_order {
        let Some(work_item) = by_id.get(work_item_id.as_str()) else {
            continue;
        };
        let mut prerequisite_task_ids = external_prerequisite_task_ids
            .get(work_item_id.as_str())
            .cloned()
            .unwrap_or_default();
        for dep_id in dependency_map
            .get(work_item_id.as_str())
            .into_iter()
            .flatten()
            .filter(|dep_id| work_item_ids.contains(dep_id.as_str()))
        {
            if let Some(created) = created_by_work_item.get(dep_id) {
                prerequisite_task_ids.push(created.task_runner_task_id.clone());
            } else {
                return Err(HandlerError::bad_request(format!(
                    "项目任务前置尚未创建执行任务，无法继续: {}",
                    work_item.title
                )));
            }
        }
        prerequisite_task_ids.sort();
        prerequisite_task_ids.dedup();

        let mut mcp_config = execution_options
            .mcp_config_for_tool_ids(&work_item.task_runner_enabled_tool_ids)
            .map_err(HandlerError::bad_request)?;
        mcp_config.builtin_prompt_locale = Some(builtin_prompt_locale.to_string());
        let task_profile = task_profile_for_work_item(work_item);
        let tags = execution_tags_for_work_item(work_item);
        let create_request = task_runner_api_client::CreateTaskRunnerTaskRequest {
            title: work_item.title.clone(),
            description: build_task_description(work_item),
            objective: build_task_objective(work_item),
            input_payload: Some(json!({
                "source": "chatos_project_requirement_execution",
                "project_id": project_id,
                "requirement_id": work_item.requirement_id,
                "project_task_id": work_item.id,
                "is_planning_task": work_item.is_planning_task,
                "source_session_id": session.id,
                "source_user_message_id": message.id,
                "source_turn_id": message_turn_id(message),
            })),
            status: Some("ready".to_string()),
            priority: Some(work_item.priority),
            tags: Some(normalize_tags(tags)),
            default_model_config_id: Some(work_item.task_runner_default_model_config_id.clone()),
            project_id: Some(project_id.to_string()),
            task_profile: Some(task_profile.to_string()),
            schedule: Some(task_runner_api_client::TaskRunnerTaskScheduleRequest {
                mode: "contact_async".to_string(),
                run_at: Some(now_rfc3339()),
            }),
            mcp_config: Some(mcp_config),
            prerequisite_task_ids: Some(prerequisite_task_ids),
        };
        let task = task_runner_api_client::create_task_runner_task(
            contact_runtime.task_runner_base_url.as_str(),
            contact_runtime.task_runner_agent_token.as_str(),
            Some(user_access_token),
            Some(session.id.as_str()),
            Some(message.id.as_str()),
            message_turn_id(message),
            &create_request,
        )
        .await
        .map_err(|err| HandlerError::bad_gateway("创建 Task Runner 执行任务失败", err))?;
        let task_runner_status = "queued".to_string();

        project_management_api_client::link_work_item_task_runner_task(
            cfg.project_service_base_url.as_str(),
            user_access_token,
            work_item.id.as_str(),
            &project_management_api_client::LinkTaskRunnerTaskRequest {
                task_runner_task_id: task.id.clone(),
                task_runner_run_id: task.last_run_id.clone(),
                link_type: Some("execution".to_string()),
                source_session_id: Some(session.id.clone()),
                source_user_message_id: Some(message.id.clone()),
                task_runner_status: Some(task_runner_status.clone()),
                last_callback_event: None,
                last_callback_at: None,
                last_error_message: None,
            },
        )
        .await
        .map_err(|err| HandlerError::bad_gateway("写入项目任务执行关联失败", err))?;

        project_management_api_client::sync_work_item_task_runner_status(
            cfg.project_service_base_url.as_str(),
            project_sync_secret,
            work_item.id.as_str(),
            &project_management_api_client::SyncTaskRunnerWorkItemStatusRequest {
                task_runner_task_id: task.id.clone(),
                task_runner_run_id: task.last_run_id.clone(),
                task_runner_status: Some(task_runner_status.clone()),
                last_callback_event: Some("task.queued".to_string()),
                last_callback_at: None,
                last_error_message: None,
                source_session_id: Some(session.id.clone()),
                source_user_message_id: Some(message.id.clone()),
            },
        )
        .await
        .map_err(|err| HandlerError::bad_gateway("同步项目任务执行状态失败", err))?;

        created_by_work_item.insert(
            work_item.id.clone(),
            CreatedExecutionTask {
                project_task_id: work_item.id.clone(),
                requirement_id: work_item.requirement_id.clone(),
                task_runner_task_id: task.id,
                task_runner_run_id: task.last_run_id,
                task_runner_status,
            },
        );
    }

    Ok(work_items
        .iter()
        .filter_map(|item| created_by_work_item.get(item.id.as_str()).cloned())
        .collect())
}

pub(in crate::api::projects) async fn load_external_prerequisite_task_ids(
    base_url: &str,
    access_token: &str,
    work_items: &[WorkItemPlanItem],
    all_work_items: &[WorkItemPlanItem],
    dependency_map: &BTreeMap<String, Vec<String>>,
    requirement_dependency_map: &BTreeMap<String, Vec<String>>,
    requirement_scope: &BTreeSet<String>,
) -> Result<BTreeMap<String, Vec<String>>, HandlerError> {
    let selected_ids = work_items
        .iter()
        .map(|item| item.id.clone())
        .collect::<BTreeSet<_>>();
    let work_item_by_id = all_work_items
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect::<BTreeMap<_, _>>();
    let mut out = BTreeMap::new();
    for item in work_items {
        let mut task_ids = Vec::new();
        let mut blockers = Vec::new();
        for dep_id in dependency_map
            .get(item.id.as_str())
            .into_iter()
            .flatten()
            .filter(|dep_id| !selected_ids.contains(dep_id.as_str()))
        {
            if let Some(task_id) =
                linked_task_runner_task_id(base_url, access_token, dep_id.as_str()).await?
            {
                task_ids.push(task_id);
                continue;
            }
            match work_item_by_id.get(dep_id.as_str()) {
                Some(dep_item) if is_done_status(dep_item.status.as_str()) => {}
                Some(dep_item) => blockers.push(format!(
                    "{} 前置项目任务未完成且没有可等待的执行任务：{}",
                    item.title, dep_item.title
                )),
                None => blockers.push(format!(
                    "{} 前置项目任务不存在或不可见：{}",
                    item.title, dep_id
                )),
            }
        }

        for prerequisite_requirement_id in requirement_dependency_map
            .get(item.requirement_id.as_str())
            .into_iter()
            .flatten()
            .filter(|requirement_id| !requirement_scope.contains(requirement_id.as_str()))
        {
            for dep_item in all_work_items.iter().filter(|candidate| {
                candidate.requirement_id == *prerequisite_requirement_id
                    && candidate.status != "archived"
            }) {
                if let Some(task_id) =
                    linked_task_runner_task_id(base_url, access_token, dep_item.id.as_str()).await?
                {
                    task_ids.push(task_id);
                    continue;
                }
                if !is_done_status(dep_item.status.as_str()) {
                    blockers.push(format!(
                        "{} 前置需求下的项目任务未完成且没有可等待的执行任务：{}",
                        item.title, dep_item.title
                    ));
                }
            }
        }
        if !blockers.is_empty() {
            blockers.sort();
            blockers.dedup();
            return Err(HandlerError::bad_request(format!(
                "存在未满足的前置项目任务，无法执行：{}",
                blockers.join("；")
            )));
        }
        task_ids.sort();
        task_ids.dedup();
        out.insert(item.id.clone(), task_ids);
    }
    Ok(out)
}

async fn linked_task_runner_task_id(
    base_url: &str,
    access_token: &str,
    work_item_id: &str,
) -> Result<Option<String>, HandlerError> {
    let links = project_management_api_client::list_work_item_task_runner_links(
        base_url,
        access_token,
        work_item_id,
    )
    .await
    .map_err(|err| HandlerError::bad_gateway("读取前置项目任务执行关联失败", err))?;
    Ok(links
        .iter()
        .find_map(|link| value_string(link, "task_runner_task_id")))
}

pub(in crate::api::projects) async fn ensure_requirement_execution_not_active(
    requirement: &RequirementPlanItem,
    work_items: &[WorkItemPlanItem],
    base_url: &str,
    project_sync_secret: &str,
    access_token: &str,
    contact_runtime: &SelectedContactRuntime,
) -> Result<(), HandlerError> {
    let mut links = load_execution_links_for_work_items(base_url, access_token, work_items).await?;
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
            base_url,
            project_sync_secret,
            link,
            task.status.as_str(),
            task_runner_callback_event_for_status(task.status.as_str()),
        )
        .await?;
    }

    if requirement.status == "in_progress" && requirement_execution_should_block(&links) {
        return Err(HandlerError::bad_request(
            "该需求已有执行中的任务，请先停止当前执行",
        ));
    }
    if let Some(item) = active_work_item_blocker(work_items, &links) {
        return Err(HandlerError::bad_request(format!(
            "项目任务正在执行或待执行，请先停止当前执行：{}",
            item.title
        )));
    }
    if let Some(link) = links
        .iter()
        .find(|link| task_runner_status_is_active(link.task_runner_status.as_deref()))
    {
        return Err(HandlerError::bad_request(format!(
            "项目任务已有执行中的 Task Runner 任务，请先停止当前执行：{}",
            link.task_runner_task_id
        )));
    }
    Ok(())
}

fn requirement_execution_should_block(links: &[ExecutionLink]) -> bool {
    links.is_empty()
        || links
            .iter()
            .any(|link| task_runner_status_is_active(link.task_runner_status.as_deref()))
}

fn active_work_item_blocker<'a>(
    work_items: &'a [WorkItemPlanItem],
    links: &[ExecutionLink],
) -> Option<&'a WorkItemPlanItem> {
    let linked_work_item_ids = links
        .iter()
        .map(|link| link.work_item_id.as_str())
        .collect::<BTreeSet<_>>();
    let active_link_work_item_ids = links
        .iter()
        .filter(|link| task_runner_status_is_active(link.task_runner_status.as_deref()))
        .map(|link| link.work_item_id.as_str())
        .collect::<BTreeSet<_>>();

    work_items
        .iter()
        .filter(|item| project_work_item_status_is_active(item.status.as_str()))
        .find(|item| {
            !linked_work_item_ids.contains(item.id.as_str())
                || active_link_work_item_ids.contains(item.id.as_str())
        })
}

fn build_task_objective(work_item: &WorkItemPlanItem) -> String {
    let mut parts = Vec::new();
    parts.push(format!("完成项目任务：{}", work_item.title));
    if let Some(description) = work_item.description.as_deref() {
        if !description.trim().is_empty() {
            parts.push(format!("任务说明：{}", description.trim()));
        }
    }
    parts.join("\n\n")
}

fn build_task_description(work_item: &WorkItemPlanItem) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(description) = work_item.description.as_deref() {
        if !description.trim().is_empty() {
            parts.push(format!("## 项目任务说明\n\n{}", description.trim()));
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n\n"))
    }
}

fn task_profile_for_work_item(work_item: &WorkItemPlanItem) -> &'static str {
    if work_item.is_planning_task {
        "chatos_plan"
    } else {
        "default"
    }
}

fn execution_tags_for_work_item(work_item: &WorkItemPlanItem) -> Vec<String> {
    let mut tags = work_item
        .tags
        .iter()
        .cloned()
        .chain(std::iter::once("project_requirement_execution".to_string()))
        .collect::<Vec<_>>();
    if work_item.is_planning_task {
        tags.push("project_planning_task".to_string());
    }
    tags
}

#[cfg(test)]
mod tests {
    use super::*;

    fn work_item(is_planning_task: bool) -> WorkItemPlanItem {
        WorkItemPlanItem {
            id: "task-1".to_string(),
            requirement_id: "req-1".to_string(),
            title: "任务".to_string(),
            description: None,
            task_runner_default_model_config_id: "model-1".to_string(),
            task_runner_enabled_tool_ids: vec!["tool-1".to_string()],
            status: "todo".to_string(),
            priority: 0,
            tags: vec!["custom".to_string()],
            is_planning_task,
        }
    }

    fn execution_link(work_item_id: &str, status: &str) -> ExecutionLink {
        ExecutionLink {
            work_item_id: work_item_id.to_string(),
            task_runner_task_id: format!("runner-{work_item_id}"),
            task_runner_run_id: None,
            task_runner_status: Some(status.to_string()),
            source_session_id: None,
            source_user_message_id: None,
        }
    }

    #[test]
    fn planning_work_item_uses_plan_profile_and_tag() {
        let item = work_item(true);

        assert_eq!(task_profile_for_work_item(&item), "chatos_plan");
        assert!(execution_tags_for_work_item(&item)
            .iter()
            .any(|tag| tag == "project_planning_task"));
    }

    #[test]
    fn concrete_work_item_uses_default_profile_without_plan_tag() {
        let item = work_item(false);

        assert_eq!(task_profile_for_work_item(&item), "default");
        assert!(!execution_tags_for_work_item(&item)
            .iter()
            .any(|tag| tag == "project_planning_task"));
    }

    #[test]
    fn execution_task_mcp_config_keeps_requested_builtin_tools() {
        let config = task_runner_api_client::TaskRunnerMcpConfigRequest {
            enabled_builtin_kinds: vec![
                "CodeMaintainerRead".to_string(),
                "CodeMaintainerWrite".to_string(),
                "TerminalController".to_string(),
                "WebTools".to_string(),
            ],
            ..task_runner_api_client::TaskRunnerMcpConfigRequest::default()
        };

        assert_eq!(
            config.enabled_builtin_kinds,
            vec![
                "CodeMaintainerRead".to_string(),
                "CodeMaintainerWrite".to_string(),
                "TerminalController".to_string(),
                "WebTools".to_string(),
            ]
        );
    }

    #[test]
    fn terminal_execution_link_clears_stale_active_work_item_blocker() {
        let mut item = work_item(false);
        item.status = "in_progress".to_string();
        let links = vec![execution_link(item.id.as_str(), "failed")];

        assert!(active_work_item_blocker(&[item], &links).is_none());
        assert!(!requirement_execution_should_block(&links));
    }

    #[test]
    fn active_work_item_without_execution_link_still_blocks() {
        let mut item = work_item(false);
        item.status = "in_progress".to_string();

        assert!(active_work_item_blocker(&[item], &[]).is_some());
        assert!(requirement_execution_should_block(&[]));
    }
}
