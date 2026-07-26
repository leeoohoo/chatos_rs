// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_ai_runtime::{AiTurnStatus, TaskRunReport};
use serde_json::json;

use crate::local_runtime::project_management::{
    UpdateLocalRequirementInput, UpdateLocalWorkItemInput,
};
use crate::local_runtime::storage::{
    AppendLocalMessageInput, CompleteLocalTurnInput, LocalDatabase,
};
use crate::local_runtime::task_runner::LocalTaskRunRecord;
use crate::LocalRuntime;

pub(super) async fn finish_task_run(
    runtime: &LocalRuntime,
    run: &LocalTaskRunRecord,
    report: TaskRunReport,
    canceled: bool,
) -> Result<(), String> {
    let database = runtime
        .local_database()
        .map_err(|error| error.to_string())?;
    if canceled {
        finalize_task_manager_session(database, run, "canceled").await?;
        let _ = persist_task_run_receipt(
            database,
            run,
            "canceled",
            "任务已停止，未继续执行后续操作。".to_string(),
        )
        .await;
        database
            .cancel_turn(
                run.owner_user_id.as_str(),
                run.turn_id.as_str(),
                "Task run canceled",
            )
            .await
            .map_err(|error| error.to_string())?;
        database
            .fail_local_task_run(run, "canceled", "Task run canceled")
            .await
            .map_err(|error| error.to_string())?;
        if run.task_kind == "conversation_task" {
            set_conversation_task_status_and_sync(
                runtime,
                run,
                "cancelled",
                None,
                Some("Task run canceled"),
            )
            .await?;
        } else {
            set_work_item_status(runtime, run, "todo").await?;
        }
        restore_requirement_after_cancellation(runtime, run).await?;
        return Ok(());
    }
    if report.status != AiTurnStatus::Completed {
        let error = report
            .error
            .clone()
            .unwrap_or_else(|| report.user_message());
        finalize_task_manager_session(database, run, "failed").await?;
        let _ = persist_task_run_receipt(
            database,
            run,
            "failed",
            user_visible_task_run_failure_receipt(error.as_str()),
        )
        .await;
        database
            .fail_turn(
                run.owner_user_id.as_str(),
                run.turn_id.as_str(),
                "task_run_failed",
                &error,
            )
            .await
            .map_err(|failure| failure.to_string())?;
        database
            .fail_local_task_run(run, "failed", error.as_str())
            .await
            .map_err(|failure| failure.to_string())?;
        if run.task_kind == "conversation_task" {
            set_conversation_task_status_and_sync(
                runtime,
                run,
                "blocked",
                None,
                Some(error.as_str()),
            )
            .await?;
        } else {
            set_work_item_status(runtime, run, "blocked").await?;
        }
        set_requirement_status(runtime, run, "failed").await?;
        return Ok(());
    }
    let task_session = database
        .local_task_manager_session_snapshot(
            run.owner_user_id.as_str(),
            run.session_id.as_str(),
            run.id.as_str(),
        )
        .await
        .map_err(|error| error.to_string())?;
    if !task_session.terminal_blocked.is_empty() {
        let error = format!(
            "Task Manager 运行会话存在 {} 个终态阻塞清单，父任务不能标记成功",
            task_session.terminal_blocked.len()
        );
        finalize_task_manager_session(database, run, "blocked").await?;
        let _ = persist_task_run_receipt(database, run, "blocked", error.clone()).await;
        database
            .fail_turn(
                run.owner_user_id.as_str(),
                run.turn_id.as_str(),
                "task_run_blocked",
                error.as_str(),
            )
            .await
            .map_err(|failure| failure.to_string())?;
        database
            .fail_local_task_run(run, "blocked", error.as_str())
            .await
            .map_err(|failure| failure.to_string())?;
        if run.task_kind == "conversation_task" {
            set_conversation_task_status_and_sync(
                runtime,
                run,
                "blocked",
                None,
                Some(error.as_str()),
            )
            .await?;
        } else {
            set_work_item_status(runtime, run, "blocked").await?;
        }
        set_requirement_status(runtime, run, "blocked").await?;
        return Ok(());
    }
    finalize_task_manager_session(database, run, "completed").await?;
    let completion_content = report
        .content
        .clone()
        .filter(|content| !content.trim().is_empty())
        .unwrap_or_else(|| "任务已完成。".to_string());
    if run.task_kind == "conversation_task" {
        database
            .complete_background_turn(run.owner_user_id.as_str(), run.turn_id.as_str())
            .await
            .map_err(|error| error.to_string())?;
        persist_task_run_receipt(database, run, "completed", completion_content.clone()).await?;
    } else {
        database
            .complete_turn(CompleteLocalTurnInput {
                turn_id: run.turn_id.clone(),
                owner_user_id: run.owner_user_id.clone(),
                content: completion_content.clone(),
                reasoning: report.reasoning.clone(),
                tool_calls_json: report.tool_calls.as_ref().map(ToString::to_string),
                metadata_json: Some(
                    json!({
                        "runtime_origin": "local_device", "message_mode": "task_run",
                        "task_id": run.task_id, "run_id": run.id,
                    })
                    .to_string(),
                ),
            })
            .await
            .map_err(|error| error.to_string())?;
    }
    database
        .complete_local_task_run(run, &report)
        .await
        .map_err(|error| error.to_string())?;
    if run.task_kind == "conversation_task" {
        set_conversation_task_status_and_sync(
            runtime,
            run,
            "done",
            Some(completion_content.as_str()),
            None,
        )
        .await?;
    } else {
        set_work_item_status(runtime, run, "done").await?;
    }
    complete_requirement_if_done(runtime, run).await
}

pub(in crate::local_runtime::task_runner) async fn finalize_task_manager_session(
    database: &LocalDatabase,
    run: &LocalTaskRunRecord,
    terminal_status: &str,
) -> Result<(), String> {
    let summary = database
        .finalize_local_task_manager_session(
            run.owner_user_id.as_str(),
            run.session_id.as_str(),
            run.id.as_str(),
            terminal_status,
        )
        .await
        .map_err(|error| error.to_string())?;
    if summary.total_changed() > 0 || summary.blocked_terminal > 0 {
        database
            .append_local_task_run_event(
                run.owner_user_id.as_str(),
                run.id.as_str(),
                "task_session_finalized",
                json!({
                    "task_session_id": run.id,
                    "terminal_status": terminal_status,
                    "satisfied": summary.satisfied,
                    "waived": summary.waived,
                    "blocked_terminal": summary.blocked_terminal,
                    "cancelled": summary.cancelled,
                    "orphaned": summary.orphaned,
                    "durable_detached": summary.durable_detached,
                }),
            )
            .await
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub(in crate::local_runtime::task_runner) async fn persist_task_run_receipt(
    database: &LocalDatabase,
    run: &LocalTaskRunRecord,
    status: &str,
    content: String,
) -> Result<(), String> {
    let (turn_id, append_to_source) = if run.task_kind == "conversation_task" {
        let task = database
            .get_local_task_board_task(
                run.owner_user_id.as_str(),
                run.session_id.as_str(),
                run.task_id.as_str(),
            )
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "Local Task Runner conversation task was not found".to_string())?;
        (task.source_turn_id, true)
    } else {
        (run.turn_id.clone(), false)
    };
    let input = AppendLocalMessageInput {
        session_id: run.session_id.clone(),
        owner_user_id: run.owner_user_id.clone(),
        turn_id,
        message_id: None,
        role: "assistant".to_string(),
        content,
        reasoning: None,
        tool_calls_json: None,
        tool_call_id: None,
        metadata_json: Some(
            json!({
                "runtime_origin": "local_device",
                "message_mode": "task_run_receipt",
                "response_status": status,
                "task_id": run.task_id,
                "run_id": run.id,
                "task_runner_async": {
                    "last_task_id": run.task_id,
                    "overall_status": status,
                }
            })
            .to_string(),
        ),
        created_at: None,
    };
    let result = if append_to_source {
        database.append_turn_result_message(input).await
    } else {
        database.append_turn_message(input).await
    };
    result.map(|_| ()).map_err(|error| error.to_string())
}

pub(in crate::local_runtime::task_runner) async fn set_work_item_status(
    runtime: &LocalRuntime,
    run: &LocalTaskRunRecord,
    status: &str,
) -> Result<(), String> {
    if run.task_kind == "conversation_task" {
        let status = match status {
            "in_progress" => "doing",
            "done" => "done",
            "blocked" => "blocked",
            "todo" => "todo",
            other => other,
        };
        return set_conversation_task_status_and_sync(runtime, run, status, None, None).await;
    }
    runtime
        .local_database()
        .map_err(|error| error.to_string())?
        .update_local_work_item(
            run.owner_user_id.as_str(),
            run.task_id.as_str(),
            UpdateLocalWorkItemInput {
                status: Some(status.to_string()),
                ..Default::default()
            },
        )
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

async fn set_conversation_task_status_and_sync(
    runtime: &LocalRuntime,
    run: &LocalTaskRunRecord,
    status: &str,
    result: Option<&str>,
    error_message: Option<&str>,
) -> Result<(), String> {
    let task = runtime
        .local_database()
        .map_err(|error| error.to_string())?
        .set_local_conversation_task_status(
            run.owner_user_id.as_str(),
            run.session_id.as_str(),
            run.task_id.as_str(),
            status,
            result,
            error_message,
        )
        .await
        .map_err(|error| error.to_string())?;
    if let Some(task) = task {
        sync_local_project_execution_work_item(runtime, run, &task).await?;
    }
    Ok(())
}

async fn sync_local_project_execution_work_item(
    runtime: &LocalRuntime,
    run: &LocalTaskRunRecord,
    task: &crate::local_runtime::task_board::LocalTaskBoardTaskRecord,
) -> Result<(), String> {
    let Some(project_work_item_id) = task.project_work_item_id.as_deref() else {
        return Ok(());
    };
    let Some(execution_group_id) = task.execution_group_id.as_deref() else {
        return Ok(());
    };
    let database = runtime
        .local_database()
        .map_err(|error| error.to_string())?;
    let related = database
        .list_local_conversation_tasks(run.owner_user_id.as_str(), run.session_id.as_str(), 200)
        .await
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter(|candidate| {
            candidate.execution_group_id.as_deref() == Some(execution_group_id)
                && candidate.project_work_item_id.as_deref() == Some(project_work_item_id)
        })
        .collect::<Vec<_>>();
    if related.is_empty() {
        return Ok(());
    }
    let status = aggregate_local_project_execution_status(
        related.iter().map(|candidate| candidate.status.as_str()),
    );
    database
        .update_local_work_item(
            run.owner_user_id.as_str(),
            project_work_item_id,
            UpdateLocalWorkItemInput {
                status: Some(status.to_string()),
                ..Default::default()
            },
        )
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn aggregate_local_project_execution_status<'a>(
    statuses: impl Iterator<Item = &'a str>,
) -> &'static str {
    let statuses = statuses.collect::<Vec<_>>();
    if statuses.iter().any(|status| *status == "blocked") {
        "blocked"
    } else if statuses
        .iter()
        .any(|status| matches!(*status, "todo" | "doing"))
    {
        "in_progress"
    } else if !statuses.is_empty() && statuses.iter().all(|status| *status == "done") {
        "done"
    } else {
        "ready"
    }
}

pub(in crate::local_runtime::task_runner) async fn set_requirement_status(
    runtime: &LocalRuntime,
    run: &LocalTaskRunRecord,
    status: &str,
) -> Result<(), String> {
    let Some(requirement_id) = run.requirement_id.as_deref() else {
        return Ok(());
    };
    let database = runtime
        .local_database()
        .map_err(|error| error.to_string())?;
    let mut requirement_ids = vec![requirement_id.to_string()];
    if matches!(status, "failed" | "blocked") {
        let snapshot = database
            .local_project_plan(run.owner_user_id.as_str(), run.project_id.as_str(), false)
            .await
            .map_err(|error| error.to_string())?;
        let by_id = snapshot
            .requirements
            .iter()
            .map(|requirement| (requirement.id.as_str(), requirement))
            .collect::<std::collections::BTreeMap<_, _>>();
        let mut current = by_id
            .get(requirement_id)
            .and_then(|requirement| requirement.parent_requirement_id.as_deref());
        while let Some(parent_id) = current {
            requirement_ids.push(parent_id.to_string());
            current = by_id
                .get(parent_id)
                .and_then(|requirement| requirement.parent_requirement_id.as_deref());
        }
    }
    for requirement_id in requirement_ids {
        database
            .update_local_requirement(
                run.owner_user_id.as_str(),
                requirement_id.as_str(),
                UpdateLocalRequirementInput {
                    status: Some(status.to_string()),
                    ..Default::default()
                },
            )
            .await
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub(crate) fn user_visible_task_run_failure_receipt(error: &str) -> String {
    let trimmed = error.trim();
    if [
        "任务执行失败：当前模型配置暂时不可用，请检查模型设置后重试。",
        "任务执行失败：服务暂时不可用，请稍后重试。",
        "任务执行失败：任务没有所需的本地文件或目录权限，请检查项目授权后重试。",
        "任务执行失败：请检查任务设置后重试；如果问题持续出现，请查看本地执行日志。",
    ]
    .contains(&trimmed)
    {
        return trimmed.to_string();
    }
    let normalized = error.to_ascii_lowercase();
    let detail = if normalized.contains("model config") || normalized.contains("api key") {
        "当前模型配置暂时不可用，请检查模型设置后重试。"
    } else if normalized.contains("timed out")
        || normalized.contains("timeout")
        || normalized.contains("error sending request")
        || normalized.contains("connection")
        || normalized.contains("dns")
    {
        "服务暂时不可用，请稍后重试。"
    } else if normalized.contains("permission")
        || normalized.contains("not authorized")
        || normalized.contains("outside")
        || normalized.contains("workspace")
    {
        "任务没有所需的本地文件或目录权限，请检查项目授权后重试。"
    } else {
        "请检查任务设置后重试；如果问题持续出现，请查看本地执行日志。"
    };
    format!("任务执行失败：{detail}")
}

async fn restore_requirement_after_cancellation(
    runtime: &LocalRuntime,
    run: &LocalTaskRunRecord,
) -> Result<(), String> {
    let Some(requirement_id) = run.requirement_id.as_deref() else {
        return Ok(());
    };
    let database = runtime
        .local_database()
        .map_err(|error| error.to_string())?;
    let runs = database
        .list_local_requirement_task_runs(
            run.owner_user_id.as_str(),
            run.project_id.as_str(),
            requirement_id,
        )
        .await
        .map_err(|error| error.to_string())?;
    if runs
        .iter()
        .any(|candidate| matches!(candidate.status.as_str(), "queued" | "running"))
    {
        return Ok(());
    }
    set_requirement_status(runtime, run, "approved").await
}

pub(in crate::local_runtime::task_runner) async fn complete_requirement_if_done(
    runtime: &LocalRuntime,
    run: &LocalTaskRunRecord,
) -> Result<(), String> {
    let Some(requirement_id) = run.requirement_id.as_deref() else {
        return Ok(());
    };
    let database = runtime
        .local_database()
        .map_err(|error| error.to_string())?;
    let snapshot = database
        .local_project_plan(run.owner_user_id.as_str(), run.project_id.as_str(), false)
        .await
        .map_err(|error| error.to_string())?;
    let by_id = snapshot
        .requirements
        .iter()
        .map(|requirement| (requirement.id.as_str(), requirement))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut candidate_ids = vec![requirement_id.to_string()];
    let mut current = by_id
        .get(requirement_id)
        .and_then(|requirement| requirement.parent_requirement_id.as_deref());
    while let Some(parent_id) = current {
        candidate_ids.push(parent_id.to_string());
        current = by_id
            .get(parent_id)
            .and_then(|requirement| requirement.parent_requirement_id.as_deref());
    }
    for candidate_id in candidate_ids {
        let subtree_ids = collect_local_requirement_subtree_ids(
            snapshot.requirements.as_slice(),
            candidate_id.as_str(),
        );
        let items = snapshot
            .work_items
            .iter()
            .filter(|item| subtree_ids.contains(item.requirement_id.as_str()))
            .filter(|item| item.status != "archived")
            .collect::<Vec<_>>();
        if !items.is_empty()
            && items.iter().all(|item| {
                crate::local_runtime::project_management::is_completed_project_status(
                    item.status.as_str(),
                )
            })
        {
            database
                .update_local_requirement(
                    run.owner_user_id.as_str(),
                    candidate_id.as_str(),
                    UpdateLocalRequirementInput {
                        status: Some("done".to_string()),
                        ..Default::default()
                    },
                )
                .await
                .map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn collect_local_requirement_subtree_ids(
    requirements: &[crate::local_runtime::project_management::LocalRequirementRecord],
    root_id: &str,
) -> std::collections::BTreeSet<String> {
    let mut ids = std::collections::BTreeSet::from([root_id.to_string()]);
    loop {
        let before = ids.len();
        for requirement in requirements {
            if requirement
                .parent_requirement_id
                .as_deref()
                .is_some_and(|parent_id| ids.contains(parent_id))
            {
                ids.insert(requirement.id.clone());
            }
        }
        if ids.len() == before {
            break;
        }
    }
    ids
}

#[cfg(test)]
mod tests {
    use super::{aggregate_local_project_execution_status, user_visible_task_run_failure_receipt};

    #[test]
    fn aggregates_generated_local_tasks_into_project_work_item_status() {
        assert_eq!(
            aggregate_local_project_execution_status(["done", "doing"].into_iter()),
            "in_progress"
        );
        assert_eq!(
            aggregate_local_project_execution_status(["done", "done"].into_iter()),
            "done"
        );
        assert_eq!(
            aggregate_local_project_execution_status(["done", "blocked"].into_iter()),
            "blocked"
        );
        assert_eq!(
            aggregate_local_project_execution_status(["done", "cancelled"].into_iter()),
            "ready"
        );
    }

    #[test]
    fn hides_internal_model_identifiers_in_failure_receipts() {
        let receipt = user_visible_task_run_failure_receipt(
            "model config is disabled: model_c51e3b8ea24ce092911ccf83a701204b",
        );
        assert_eq!(
            receipt,
            "任务执行失败：当前模型配置暂时不可用，请检查模型设置后重试。"
        );
        assert!(!receipt.contains("model_c51"));
        assert!(!receipt.contains("model config"));
    }

    #[test]
    fn hides_internal_network_details_in_failure_receipts() {
        let receipt = user_visible_task_run_failure_receipt(
            "error sending request for url (http://internal-provider.local/v1/responses)",
        );
        assert_eq!(receipt, "任务执行失败：服务暂时不可用，请稍后重试。");
        assert!(!receipt.contains("internal-provider"));
    }

    #[test]
    fn preserves_already_sanitized_failure_receipts() {
        let receipt = "任务执行失败：当前模型配置暂时不可用，请检查模型设置后重试。";
        assert_eq!(user_visible_task_run_failure_receipt(receipt), receipt);
    }
}
