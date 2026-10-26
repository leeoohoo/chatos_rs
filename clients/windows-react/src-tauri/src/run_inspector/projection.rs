use chatos_local_agent_protocol::{LocalAgentEventRecord, LocalAgentRunRecord, LocalTaskRecord};
use serde_json::Value;

use super::models::{
    RunInspectorEvent, RunInspectorProcessEntry, RunInspectorRun, TaskInspectorTask,
};

const PROCESS_TOOL_NAME: &str = "task_run_process_record_process";

pub fn task(
    record: &LocalTaskRecord,
    run: Option<&LocalAgentRunRecord>,
    process_entries: Vec<RunInspectorProcessEntry>,
) -> TaskInspectorTask {
    let projected_run = run.map(RunInspectorRun::from_record);
    TaskInspectorTask {
        task_id: record.task_id.clone(),
        title: record.title.clone(),
        status: display_status(record),
        version: record.version,
        description: string_field(&record.input, &["description"]),
        objective: string_field(&record.input, &["objective"]),
        result_summary: record.result_summary.clone().or_else(|| {
            projected_run
                .as_ref()
                .and_then(|value| value.result_summary.clone())
        }),
        process_entries,
        last_run_id: run
            .map(|value| value.run_id.clone())
            .or_else(|| record.active_run_id.clone()),
        model_config_ref: record.model_config_ref.clone(),
        thinking_level: thinking_level(&record.input),
        created_at_unix_ms: record.created_at_unix_ms,
        updated_at_unix_ms: record.updated_at_unix_ms,
    }
}

pub fn event(record: &LocalAgentEventRecord) -> RunInspectorEvent {
    RunInspectorEvent {
        cursor: record.cursor,
        event_id: record.event_id.clone(),
        event_type: record.event_type.clone(),
        summary: string_field(&record.payload, &["summary", "message", "reason", "error"]),
        status: string_field(&record.payload, &["status"]),
        created_at_unix_ms: record.created_at_unix_ms,
    }
}

pub fn process_entries(events: &[LocalAgentEventRecord]) -> Vec<RunInspectorProcessEntry> {
    let mut ordered = events.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|event| event.cursor);
    let mut entries = Vec::new();
    for event in ordered {
        let Some(invocations) = event.payload.get("invocations").and_then(Value::as_array) else {
            continue;
        };
        for (index, invocation) in invocations.iter().enumerate() {
            if string_field(invocation, &["tool_name"]).as_deref() != Some(PROCESS_TOOL_NAME)
                || string_field(invocation, &["status"]).as_deref() != Some("succeeded")
            {
                continue;
            }
            let Some(arguments) = invocation.get("arguments") else {
                continue;
            };
            let operation =
                string_field(arguments, &["operation"]).unwrap_or_else(|| "append".to_owned());
            let heading = string_field(arguments, &["heading"]);
            let content = string_field(arguments, &["content"]);
            match operation.as_str() {
                "clear" => entries.clear(),
                "replace" => {
                    entries.clear();
                    if let Some(content) = content {
                        entries.push(process_entry(event, index, heading, content));
                    }
                }
                "append" => {
                    if let Some(content) = content {
                        entries.push(process_entry(event, index, heading, content));
                    }
                }
                _ => {}
            }
        }
    }
    entries
}

fn process_entry(
    event: &LocalAgentEventRecord,
    index: usize,
    heading: Option<String>,
    content: String,
) -> RunInspectorProcessEntry {
    RunInspectorProcessEntry {
        id: format!("{}-{index}", event.event_id),
        heading,
        content,
        created_at_unix_ms: event.created_at_unix_ms,
    }
}

pub(super) fn display_status(task: &LocalTaskRecord) -> String {
    if matches!(task.status.as_str(), "pending" | "ready" | "running") {
        match string_field(&task.input, &["followup_review_state"]).as_deref() {
            Some("pending") => return "reviewing_followup".to_owned(),
            Some("held") => return "waiting_confirmation".to_owned(),
            _ => {}
        }
    }
    if task.status.as_str() == "ready" && task.active_run_id.is_some() {
        return "queued".to_owned();
    }
    task.status.as_str().to_owned()
}

fn thinking_level(input: &Value) -> Option<String> {
    let settings = input.get("runtime_settings")?;
    if settings.get("reasoning_enabled").and_then(Value::as_bool) == Some(false) {
        return Some("none".to_owned());
    }
    string_field(settings, &["selected_thinking_level"])
}

pub fn string_field(value: &Value, fields: &[&str]) -> Option<String> {
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

pub fn model_output(outcome: &Value) -> Option<String> {
    string_field(outcome, &["content", "answer", "text", "output"])
}

pub fn execution_error(outcome: &Value) -> Option<String> {
    if string_field(outcome, &["reason_code"]).as_deref() == Some("execution_interrupted") {
        return Some("任务执行意外中断，无法确认当前步骤的结果。请先检查执行记录，再决定是否重试，避免重复操作。".to_owned());
    }
    string_field(outcome, &["error", "reason"])
}

#[cfg(test)]
mod tests {
    use chatos_local_agent_protocol::{LocalAgentEventRecord, LocalTaskRecord, LocalTaskStatus};
    use serde_json::json;

    use super::*;

    fn process_event(cursor: i64, operation: &str, content: Option<&str>) -> LocalAgentEventRecord {
        LocalAgentEventRecord {
            cursor,
            event_id: format!("event-{cursor}"),
            run_id: "run-1".to_owned(),
            event_type: "tool_batch_completed".to_owned(),
            payload: json!({"invocations": [{
                "tool_name": PROCESS_TOOL_NAME,
                "status": "succeeded",
                "arguments": {"operation": operation, "heading": "阶段", "content": content}
            }]}),
            created_at_unix_ms: cursor,
        }
    }

    #[test]
    fn process_projection_matches_append_replace_and_clear_semantics() {
        let events = vec![
            process_event(1, "append", Some("one")),
            process_event(2, "replace", Some("two")),
            process_event(3, "append", Some("three")),
        ];
        let entries = process_entries(&events);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].content, "two");
        assert_eq!(entries[1].content, "three");
        assert!(process_entries(&[process_event(4, "clear", None)]).is_empty());
    }

    #[test]
    fn event_projection_does_not_expose_arbitrary_payload_fields() {
        let record = LocalAgentEventRecord {
            cursor: 1,
            event_id: "event-1".to_owned(),
            run_id: "run-1".to_owned(),
            event_type: "tool_completed".to_owned(),
            payload: json!({"summary": "done", "secret": "hidden"}),
            created_at_unix_ms: 2,
        };
        let projected = event(&record);
        assert_eq!(projected.summary.as_deref(), Some("done"));
    }

    #[test]
    fn followup_gates_override_stale_execution_status() {
        let task = |status, followup_review_state: &str| LocalTaskRecord {
            graph_id: "graph-1".to_owned(),
            owner_user_id: "owner-1".to_owned(),
            source_entity_type: "conversation_turn".to_owned(),
            source_entity_id: "turn-1".to_owned(),
            task_id: "task-1".to_owned(),
            title: "Task".to_owned(),
            profile_key: "main_chat".to_owned(),
            model_config_ref: "model-1".to_owned(),
            model_config_revision: "revision-1".to_owned(),
            capability_policy_revision: "policy-1".to_owned(),
            input: json!({"followup_review_state": followup_review_state}),
            max_iterations: 8,
            status,
            active_run_id: Some("run-1".to_owned()),
            cancel_reason: None,
            replacement_task_ids: Vec::new(),
            cancelled_because_task_id: None,
            result_summary: None,
            version: 1,
            created_at_unix_ms: 1,
            updated_at_unix_ms: 2,
        };

        assert_eq!(
            display_status(&task(LocalTaskStatus::Running, "pending")),
            "reviewing_followup"
        );
        assert_eq!(
            display_status(&task(LocalTaskStatus::Pending, "held")),
            "waiting_confirmation"
        );
        assert_eq!(
            display_status(&task(LocalTaskStatus::Succeeded, "pending")),
            "succeeded"
        );
    }
}
