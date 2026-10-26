mod models;
mod projection;

use chatos_local_agent_protocol::{
    CancelTaskCommand, GetMessageTaskGraphCommand, GetRunCommand, GetTaskRunsCommand, HostCommand,
    HostResult, ListEventsCommand, LocalAgentEventPayloadMode, LocalAgentEventRecord,
    LocalAgentRunRecord, LocalMessageTaskGraph, LocalTaskGraph, RestartTaskCommand,
    RetryTaskCommand,
};
use tauri::State;

use crate::{host_lifecycle::HostManager, terminal_tool_worker::TerminalToolWorker};
use models::{
    MessageTaskGraphInput, MessageTaskGraphProjection, RunInspectorDetail, RunInspectorRun,
    TaskInspectorDetail, TaskInspectorInput, TaskMutationInput, TaskMutationReceipt,
};

const LATEST_EVENT_LIMIT: usize = 40;
const OLDER_EVENT_LIMIT: usize = 50;
const PROCESS_EVENT_LIMIT: u32 = 500;
const MAXIMUM_PROCESS_EVENT_PAGES: usize = 10;

#[tauri::command]
pub async fn load_message_task_graph(
    manager: State<'_, HostManager>,
    input: MessageTaskGraphInput,
) -> Result<Option<MessageTaskGraphProjection>, String> {
    Ok(message_graph(&manager, &validate_lookup(input)?)
        .await?
        .map(MessageTaskGraphProjection::from_graph))
}

#[tauri::command]
pub async fn load_task_inspector(
    manager: State<'_, HostManager>,
    input: TaskInspectorInput,
) -> Result<TaskInspectorDetail, String> {
    let lookup = validate_lookup(input.lookup)?;
    let task_id = required("task_id", input.task_id)?;
    let graph = message_graph(&manager, &lookup)
        .await?
        .ok_or_else(|| "没有找到对应的任务图。".to_owned())?;
    let task = graph
        .nodes
        .into_iter()
        .map(|node| node.task)
        .find(|task| task.task_id == task_id)
        .ok_or_else(|| "没有找到对应的任务。".to_owned())?;
    let run = latest_task_run(&manager, &lookup.owner_user_id, &task_id).await?;
    let entries = match run.as_ref() {
        Some(run) => projection::process_entries(
            &process_events(&manager, &lookup.owner_user_id, &run.run_id).await?,
        ),
        None => Vec::new(),
    };
    Ok(TaskInspectorDetail {
        task: projection::task(&task, run.as_ref(), entries),
        run: run.as_ref().map(RunInspectorRun::from_record),
    })
}

#[tauri::command]
pub async fn load_run_inspector(
    manager: State<'_, HostManager>,
    owner_user_id: String,
    run_id: String,
    event_offset: usize,
) -> Result<RunInspectorDetail, String> {
    let owner_user_id = required("owner_user_id", owner_user_id)?;
    let run_id = required("run_id", run_id)?;
    let result = manager
        .execute(HostCommand::GetRun(GetRunCommand {
            owner_user_id: owner_user_id.clone(),
            run_id: run_id.clone(),
        }))
        .await?;
    let HostResult::Run { run } = result else {
        return Err("Local Agent Host 返回了错误的 Run 响应。".into());
    };
    if run.owner_user_id != owner_user_id || run.run_id != run_id {
        return Err("Local Agent Host 返回了不匹配的 Run。".into());
    }
    let page_limit = if event_offset == 0 {
        LATEST_EVENT_LIMIT
    } else {
        OLDER_EVENT_LIMIT
    };
    let requested_count = (event_offset + page_limit + 1).clamp(1, 100);
    let result = manager
        .execute(HostCommand::ListEvents(ListEventsCommand {
            owner_user_id,
            after_cursor: 0,
            limit: requested_count as u32,
            run_id: Some(run_id),
            event_type: None,
            newest_first: true,
            payload_mode: LocalAgentEventPayloadMode::Process,
        }))
        .await?;
    let HostResult::Events { events, .. } = result else {
        return Err("Local Agent Host 返回了错误的执行记录响应。".into());
    };
    let selected = events
        .iter()
        .skip(event_offset)
        .take(page_limit)
        .map(projection::event)
        .collect::<Vec<_>>();
    let next_offset = event_offset + selected.len();
    Ok(RunInspectorDetail {
        run: RunInspectorRun::from_record(&run),
        events: selected,
        events_has_more: events.len() > next_offset,
        next_offset,
    })
}

#[tauri::command]
pub async fn retry_task_inspector(
    manager: State<'_, HostManager>,
    terminal_worker: State<'_, TerminalToolWorker>,
    input: TaskMutationInput,
) -> Result<TaskMutationReceipt, String> {
    let input = validate_mutation(input)?;
    let previous_run = latest_task_run(&manager, &input.owner_user_id, &input.task_id).await?;
    if let Some(run) = previous_run.as_ref() {
        terminal_worker.release_run(&run.run_id).await;
    }
    let result = manager
        .execute(HostCommand::RetryTask(RetryTaskCommand {
            owner_user_id: input.owner_user_id,
            task_id: input.task_id.clone(),
            expected_version: input.expected_version,
            retry_instruction: optional_text(input.instruction),
        }))
        .await?;
    let receipt = mutation_receipt(result, &input.task_id)?;
    terminal_worker.wake();
    Ok(receipt)
}

#[tauri::command]
pub async fn cancel_task_inspector(
    manager: State<'_, HostManager>,
    terminal_worker: State<'_, TerminalToolWorker>,
    input: TaskMutationInput,
) -> Result<TaskMutationReceipt, String> {
    let input = validate_mutation(input)?;
    let previous_run = latest_task_run(&manager, &input.owner_user_id, &input.task_id).await?;
    let result = manager
        .execute(HostCommand::CancelTask(CancelTaskCommand {
            owner_user_id: input.owner_user_id,
            task_id: input.task_id.clone(),
            expected_version: Some(input.expected_version),
            reason: optional_text(input.reason)
                .unwrap_or_else(|| "user requested cancellation from task inspector".to_owned()),
            replacement_task_ids: Vec::new(),
        }))
        .await?;
    let receipt = mutation_receipt(result, &input.task_id)?;
    if let Some(run) = previous_run.as_ref() {
        terminal_worker.release_run(&run.run_id).await;
    }
    Ok(receipt)
}

#[tauri::command]
pub async fn restart_task_inspector(
    manager: State<'_, HostManager>,
    terminal_worker: State<'_, TerminalToolWorker>,
    input: TaskMutationInput,
) -> Result<TaskMutationReceipt, String> {
    let input = validate_mutation(input)?;
    let previous_run = latest_task_run(&manager, &input.owner_user_id, &input.task_id).await?;
    if let Some(run) = previous_run.as_ref() {
        terminal_worker.release_run(&run.run_id).await;
    }
    let result = manager
        .execute(HostCommand::RestartTask(RestartTaskCommand {
            owner_user_id: input.owner_user_id,
            task_id: input.task_id.clone(),
            expected_version: input.expected_version,
            reason: optional_text(input.reason)
                .unwrap_or_else(|| "user requested recovery from task inspector".to_owned()),
        }))
        .await?;
    let receipt = mutation_receipt(result, &input.task_id)?;
    terminal_worker.wake();
    Ok(receipt)
}

async fn message_graph(
    manager: &HostManager,
    input: &MessageTaskGraphInput,
) -> Result<Option<LocalMessageTaskGraph>, String> {
    let result = manager
        .execute_detailed(HostCommand::GetMessageTaskGraph(
            GetMessageTaskGraphCommand {
                owner_user_id: input.owner_user_id.clone(),
                source_conversation_id: input.conversation_id.clone(),
                source_turn_id: input.turn_id.clone(),
                source_user_message_id: Some(input.user_message_id.clone()),
            },
        ))
        .await;
    match result {
        Ok(HostResult::MessageTaskGraph { graph }) => Ok(Some(graph)),
        Ok(_) => Err("Local Agent Host 返回了错误的任务图响应。".into()),
        Err(error) if error.code.as_deref() == Some("not_found") => Ok(None),
        Err(error) => Err(error.message),
    }
}

async fn latest_task_run(
    manager: &HostManager,
    owner_user_id: &str,
    task_id: &str,
) -> Result<Option<LocalAgentRunRecord>, String> {
    let result = manager
        .execute(HostCommand::GetTaskRuns(GetTaskRunsCommand {
            owner_user_id: owner_user_id.to_owned(),
            task_id: task_id.to_owned(),
            limit: 1,
        }))
        .await?;
    let HostResult::TaskRuns {
        task_id: returned_task_id,
        runs,
    } = result
    else {
        return Err("Local Agent Host 返回了错误的 Task Runs 响应。".into());
    };
    if returned_task_id != task_id {
        return Err("Local Agent Host 返回了不匹配的 Task Runs。".into());
    }
    Ok(runs.into_iter().next())
}

async fn process_events(
    manager: &HostManager,
    owner_user_id: &str,
    run_id: &str,
) -> Result<Vec<LocalAgentEventRecord>, String> {
    let mut collected = Vec::new();
    let mut cursor = 0;
    for _ in 0..MAXIMUM_PROCESS_EVENT_PAGES {
        let result = manager
            .execute(HostCommand::ListEvents(ListEventsCommand {
                owner_user_id: owner_user_id.to_owned(),
                after_cursor: cursor,
                limit: PROCESS_EVENT_LIMIT,
                run_id: Some(run_id.to_owned()),
                event_type: Some("tool_batch_completed".to_owned()),
                newest_first: false,
                payload_mode: LocalAgentEventPayloadMode::Full,
            }))
            .await?;
        let HostResult::Events {
            events,
            next_cursor,
        } = result
        else {
            return Err("Local Agent Host 返回了错误的过程记录响应。".into());
        };
        let count = events.len();
        collected.extend(events);
        if count < PROCESS_EVENT_LIMIT as usize || next_cursor <= cursor {
            break;
        }
        cursor = next_cursor;
    }
    Ok(collected)
}

fn mutation_receipt(result: HostResult, task_id: &str) -> Result<TaskMutationReceipt, String> {
    let HostResult::TaskGraph {
        graph: LocalTaskGraph { tasks, .. },
    } = result
    else {
        return Err("Local Agent Host 返回了错误的任务变更响应。".into());
    };
    tasks
        .into_iter()
        .find(|task| task.task_id == task_id)
        .map(TaskMutationReceipt::from_task)
        .ok_or_else(|| "任务变更响应缺少目标任务。".to_owned())
}

fn validate_lookup(mut input: MessageTaskGraphInput) -> Result<MessageTaskGraphInput, String> {
    input.owner_user_id = required("owner_user_id", input.owner_user_id)?;
    input.conversation_id = required("conversation_id", input.conversation_id)?;
    input.turn_id = required("turn_id", input.turn_id)?;
    input.user_message_id = required("user_message_id", input.user_message_id)?;
    Ok(input)
}

fn validate_mutation(mut input: TaskMutationInput) -> Result<TaskMutationInput, String> {
    input.owner_user_id = required("owner_user_id", input.owner_user_id)?;
    input.task_id = required("task_id", input.task_id)?;
    if input.expected_version == 0 {
        return Err("expected_version must be greater than zero".to_owned());
    }
    Ok(input)
}

fn optional_text(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn required(field: &str, value: String) -> Result<String, String> {
    let value = value.trim().to_owned();
    if value.is_empty() || value.chars().any(char::is_control) {
        Err(format!("{field} is required"))
    } else {
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_limits_match_macos_application_use_case() {
        assert_eq!(LATEST_EVENT_LIMIT, 40);
        assert_eq!(OLDER_EVENT_LIMIT, 50);
    }

    #[test]
    fn mutation_identity_and_version_are_required() {
        let invalid = TaskMutationInput {
            owner_user_id: "user-1".to_owned(),
            task_id: "task-1".to_owned(),
            expected_version: 0,
            instruction: None,
            reason: None,
        };
        assert!(validate_mutation(invalid).is_err());
    }
}
