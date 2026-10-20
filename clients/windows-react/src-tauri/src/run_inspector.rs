use chatos_local_agent_protocol::{
    GetMessageTaskGraphCommand, GetRunCommand, HostCommand, HostResult, ListEventsCommand,
    LocalAgentEventPayloadMode, LocalMessageTaskGraph,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::State;

use crate::host_lifecycle::HostManager;

const LATEST_EVENT_LIMIT: usize = 40;
const OLDER_EVENT_LIMIT: usize = 50;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageTaskGraphInput {
    owner_user_id: String,
    conversation_id: String,
    turn_id: String,
    user_message_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunInspectorDetail {
    run: RunInspectorRun,
    events: Vec<RunInspectorEvent>,
    events_has_more: bool,
    next_offset: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RunInspectorRun {
    run_id: String,
    task_id: String,
    status: String,
    iteration: u32,
    max_iterations: u32,
    updated_at_unix_ms: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RunInspectorEvent {
    cursor: i64,
    event_id: String,
    event_type: String,
    summary: Option<String>,
    status: Option<String>,
    created_at_unix_ms: i64,
}

#[tauri::command]
pub async fn load_message_task_graph(
    manager: State<'_, HostManager>,
    input: MessageTaskGraphInput,
) -> Result<Option<LocalMessageTaskGraph>, String> {
    let owner_user_id = required("owner_user_id", input.owner_user_id)?;
    let conversation_id = required("conversation_id", input.conversation_id)?;
    let turn_id = required("turn_id", input.turn_id)?;
    let user_message_id = required("user_message_id", input.user_message_id)?;
    let result = manager
        .execute_detailed(HostCommand::GetMessageTaskGraph(
            GetMessageTaskGraphCommand {
                owner_user_id,
                source_conversation_id: conversation_id,
                source_turn_id: turn_id,
                source_user_message_id: Some(user_message_id),
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
        .map(map_event)
        .collect::<Vec<_>>();
    let next_offset = event_offset + selected.len();
    Ok(RunInspectorDetail {
        run: RunInspectorRun {
            run_id: run.run_id,
            task_id: run.owner_entity_id,
            status: run.status.to_string(),
            iteration: run.iteration,
            max_iterations: run.max_iterations,
            updated_at_unix_ms: run.updated_at_unix_ms,
        },
        events: selected,
        events_has_more: events.len() > next_offset,
        next_offset,
    })
}

fn map_event(event: &chatos_local_agent_protocol::LocalAgentEventRecord) -> RunInspectorEvent {
    RunInspectorEvent {
        cursor: event.cursor,
        event_id: event.event_id.clone(),
        event_type: event.event_type.clone(),
        summary: string_field(&event.payload, &["summary", "message", "reason", "error"]),
        status: string_field(&event.payload, &["status"]),
        created_at_unix_ms: event.created_at_unix_ms,
    }
}

fn string_field(value: &Value, fields: &[&str]) -> Option<String> {
    let object = value.as_object()?;
    fields.iter().find_map(|field| {
        object
            .get(*field)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    })
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
    fn summaries_are_projected_without_exposing_the_payload() {
        let value = serde_json::json!({"summary": " completed ", "secret": "hidden"});
        assert_eq!(
            string_field(&value, &["summary"]).as_deref(),
            Some("completed")
        );
    }
}
