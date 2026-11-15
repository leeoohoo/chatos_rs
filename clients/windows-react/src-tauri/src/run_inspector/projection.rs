use chatos_local_agent_protocol::{
    LocalAgentEventRecord, LocalAgentRunRecord, LocalRunCapabilityDisclosure, LocalTaskRecord,
};
use serde_json::Value;
use std::collections::BTreeMap;

use super::models::{
    RunInspectorEvent, RunInspectorProcessEntry, RunInspectorRun, TaskInspectorTask,
};

const PROCESS_TOOL_NAME: &str = "task_run_process_record_process";

pub fn task(
    record: &LocalTaskRecord,
    run: Option<&LocalAgentRunRecord>,
    process_entries: Vec<RunInspectorProcessEntry>,
    completed_events: &[LocalAgentEventRecord],
    requested_events: &[LocalAgentEventRecord],
    disclosures: &[LocalRunCapabilityDisclosure],
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
        capability_usage: capability_usage(record, requested_events, completed_events, disclosures),
        created_at_unix_ms: record.created_at_unix_ms,
        updated_at_unix_ms: record.updated_at_unix_ms,
    }
}

/// Build the same small, redacted capability summary shown by macOS. Raw tool
/// arguments and results are deliberately never included in this projection.
pub fn capability_usage(
    task: &LocalTaskRecord,
    requested_events: &[LocalAgentEventRecord],
    completed_events: &[LocalAgentEventRecord],
    disclosures: &[LocalRunCapabilityDisclosure],
) -> Option<Value> {
    let hints = task
        .input
        .pointer("/tool_options/plugin_hints")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let object = item.as_object()?;
                    let key = object.get("plugin_key")?.as_str()?.trim();
                    let component = object.get("component_key")?.as_str()?.trim();
                    if key.is_empty() || component.is_empty() {
                        return None;
                    }
                    Some((key.to_owned(), component.to_owned()))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    #[derive(Clone)]
    struct Call {
        name: String,
        display: String,
        route: String,
        plugin: Option<String>,
        status: String,
        at: i64,
    }
    let mut requested = BTreeMap::<String, (String, Option<String>, Option<String>)>::new();
    for event in requested_events {
        let Some(calls) = event.payload.get("tool_calls").and_then(Value::as_array) else {
            continue;
        };
        for call in calls {
            let Some(object) = call.as_object() else {
                continue;
            };
            let Some(id) = object.get("call_id").and_then(Value::as_str) else {
                continue;
            };
            let Some(name) = object.get("tool_name").and_then(Value::as_str) else {
                continue;
            };
            let args = object.get("arguments").and_then(Value::as_object);
            let plugin = args
                .and_then(|value| value.get("plugin_option"))
                .and_then(Value::as_str)
                .map(str::to_owned);
            let tool = args
                .and_then(|value| value.get("tool_option"))
                .and_then(Value::as_str)
                .map(str::to_owned);
            requested.insert(id.to_owned(), (name.to_owned(), plugin, tool));
        }
    }
    let mut calls = Vec::<Call>::new();
    for event in completed_events {
        let Some(invocations) = event.payload.get("invocations").and_then(Value::as_array) else {
            continue;
        };
        for invocation in invocations {
            let Some(object) = invocation.as_object() else {
                continue;
            };
            let Some(id) = object.get("call_id").and_then(Value::as_str) else {
                continue;
            };
            let Some(name) = object.get("tool_name").and_then(Value::as_str) else {
                continue;
            };
            let (requested_name, plugin_option, tool_option) = requested
                .get(id)
                .cloned()
                .unwrap_or_else(|| (name.to_owned(), None, None));
            let disclosure = plugin_option.as_ref().and_then(|plugin| {
                disclosures.iter().find(|item| {
                    item.plugin_option == *plugin
                        && tool_option
                            .as_deref()
                            .is_some_and(|tool| item.tool_option == tool)
                })
            });
            let plugin = disclosure
                .and_then(|item| item.provider_id.strip_prefix("plugin:"))
                .map(|value| value.split(':').next().unwrap_or(value).to_owned())
                .or(plugin_option);
            let display = disclosure
                .map(|item| item.tool_name.clone())
                .unwrap_or_else(|| humanized_tool_name(&requested_name));
            let route = object
                .get("execution_route")
                .and_then(Value::as_str)
                .unwrap_or("platform")
                .to_owned();
            let status = object
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("running")
                .to_owned();
            calls.push(Call {
                name: requested_name,
                display,
                route,
                plugin,
                status,
                at: event.created_at_unix_ms,
            });
        }
    }
    if hints.is_empty() && calls.is_empty() {
        return None;
    }
    let mut counts = BTreeMap::<String, usize>::new();
    for call in &calls {
        if let Some(plugin) = &call.plugin {
            *counts.entry(plugin.clone()).or_default() += 1;
        }
    }
    let plugins = hints.iter().map(|(key, component)| {
        let count = counts.get(key).copied().unwrap_or(0);
        serde_json::json!({"id": key, "key": key, "component_key": component, "display_name": key, "call_count": count, "status": if count > 0 { "used" } else { "available" }})
    }).collect::<Vec<_>>();
    let mut grouped = BTreeMap::<String, Vec<&Call>>::new();
    for call in &calls {
        grouped
            .entry(format!(
                "{}:{}:{}",
                call.plugin.as_deref().unwrap_or("builtin"),
                call.display,
                call.status
            ))
            .or_default()
            .push(call);
    }
    let tools = grouped.into_values().filter_map(|group| {
        let latest = group.iter().max_by_key(|call| call.at)?;
        Some(serde_json::json!({"name": latest.name, "display_name": latest.display, "route": latest.route, "plugin_id": latest.plugin, "plugin_name": latest.plugin, "status": latest.status, "call_count": group.len(), "latest_at_unix_ms": latest.at}))
    }).collect::<Vec<_>>();
    Some(serde_json::json!({"version": 1, "plugins": plugins, "tools": tools}))
}

fn humanized_tool_name(name: &str) -> String {
    match name {
        "capability_search" => "查找工具能力".to_owned(),
        "capability_describe" => "读取工具说明".to_owned(),
        "capability_invoke" => "调用 Plugin 工具".to_owned(),
        _ => name.replace('_', " "),
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

pub(crate) fn display_status(task: &LocalTaskRecord) -> String {
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

    #[test]
    fn capability_usage_projects_counts_without_tool_arguments() {
        let task = LocalTaskRecord {
            graph_id: "graph-1".to_owned(),
            owner_user_id: "owner-1".to_owned(),
            source_entity_type: "conversation_turn".to_owned(),
            source_entity_id: "turn-1".to_owned(),
            task_id: "task-1".to_owned(),
            title: "Task".to_owned(),
            profile_key: "task_execution".to_owned(),
            model_config_ref: "model-1".to_owned(),
            model_config_revision: "revision-1".to_owned(),
            capability_policy_revision: "policy-1".to_owned(),
            input: json!({"tool_options":{"plugin_hints":[{"plugin_key":"document-plugin","component_key":"document-mcp"}]}}),
            max_iterations: 8,
            status: LocalTaskStatus::Succeeded,
            active_run_id: Some("run-1".to_owned()),
            cancel_reason: None,
            replacement_task_ids: Vec::new(),
            cancelled_because_task_id: None,
            result_summary: None,
            version: 1,
            created_at_unix_ms: 1,
            updated_at_unix_ms: 2,
        };
        let requested = vec![LocalAgentEventRecord {
            cursor: 1,
            event_id: "requested".to_owned(),
            run_id: "run-1".to_owned(),
            event_type: "tool_batch_requested".to_owned(),
            created_at_unix_ms: 10,
            payload: json!({"tool_calls":[{"call_id":"call-1","tool_name":"capability_invoke","arguments":{"plugin_option":"document-plugin","secret":"must-not-leak"}}]}),
        }];
        let completed = vec![LocalAgentEventRecord {
            cursor: 2,
            event_id: "completed".to_owned(),
            run_id: "run-1".to_owned(),
            event_type: "tool_batch_completed".to_owned(),
            created_at_unix_ms: 20,
            payload: json!({"invocations":[{"call_id":"call-1","tool_name":"capability_invoke","execution_route":"plugin","status":"succeeded","arguments":{"secret":"must-not-leak"}}]}),
        }];
        let usage = capability_usage(&task, &requested, &completed, &[]).expect("usage");
        assert_eq!(usage["plugins"][0]["call_count"], 1);
        assert_eq!(usage["tools"][0]["call_count"], 1);
        assert!(usage.to_string().contains("document-plugin"));
        assert!(!usage.to_string().contains("must-not-leak"));
    }
}
