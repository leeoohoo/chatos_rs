// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;

use crate::services::task_runner_api_client;

use super::errors::HandlerError;
use super::status::{
    project_work_item_status_is_active, task_runner_callback_event_for_status,
    task_runner_status_is_active,
};
use super::sync::{load_execution_links_for_work_items, sync_execution_link_status};
use super::types::{ExecutionLink, RequirementPlanItem, SelectedContactRuntime, WorkItemPlanItem};

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

#[cfg(test)]
mod tests {
    use super::*;

    fn work_item(is_planning_task: bool) -> WorkItemPlanItem {
        WorkItemPlanItem {
            id: "task-1".to_string(),
            requirement_id: "req-1".to_string(),
            title: "任务".to_string(),
            description: None,
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
