// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_ai_runtime::{AiTurnStatus, TaskRunReport};
use serde_json::json;

use crate::local_runtime::project_management::{
    UpdateLocalRequirementInput, UpdateLocalWorkItemInput,
};
use crate::local_runtime::storage::CompleteLocalTurnInput;
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
    if canceled || report.status == AiTurnStatus::Aborted {
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
        set_work_item_status(runtime, run, "todo").await?;
        return Ok(());
    }
    if report.status != AiTurnStatus::Completed {
        let error = report
            .error
            .clone()
            .unwrap_or_else(|| report.user_message());
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
        set_work_item_status(runtime, run, "blocked").await?;
        return Ok(());
    }
    database
        .complete_turn(CompleteLocalTurnInput {
            turn_id: run.turn_id.clone(),
            owner_user_id: run.owner_user_id.clone(),
            content: report.content.clone().unwrap_or_default(),
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
    database
        .complete_local_task_run(run, &report)
        .await
        .map_err(|error| error.to_string())?;
    set_work_item_status(runtime, run, "done").await?;
    complete_requirement_if_done(runtime, run).await
}

pub(super) async fn set_work_item_status(
    runtime: &LocalRuntime,
    run: &LocalTaskRunRecord,
    status: &str,
) -> Result<(), String> {
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

async fn complete_requirement_if_done(
    runtime: &LocalRuntime,
    run: &LocalTaskRunRecord,
) -> Result<(), String> {
    let Some(requirement_id) = run.requirement_id.as_deref() else {
        return Ok(());
    };
    let database = runtime
        .local_database()
        .map_err(|error| error.to_string())?;
    let items = database
        .list_local_work_items_for_requirement(
            run.owner_user_id.as_str(),
            run.project_id.as_str(),
            requirement_id,
            false,
        )
        .await
        .map_err(|error| error.to_string())?;
    if items
        .iter()
        .all(|item| matches!(item.status.as_str(), "done" | "completed"))
    {
        database
            .update_local_requirement(
                run.owner_user_id.as_str(),
                requirement_id,
                UpdateLocalRequirementInput {
                    status: Some("completed".to_string()),
                    ..Default::default()
                },
            )
            .await
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}
