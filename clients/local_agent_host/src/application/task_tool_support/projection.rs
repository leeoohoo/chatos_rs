use super::dependency::DependencyReduction;
use chatos_local_agent_protocol::{
    LocalAgentRunRecord, LocalTaskDependency, LocalTaskGraph, LocalTaskRecord, LocalTaskStatus,
};
use chrono::DateTime;
use serde_json::{json, Value};
use std::collections::{BTreeSet, HashMap};

#[derive(Debug, Clone)]
pub(in crate::application) struct CreatedTaskBinding {
    pub(in crate::application) client_ref: String,
    pub(in crate::application) task_id: String,
}

pub(in crate::application) fn task_for_agent_tool(
    task: &LocalTaskRecord,
    dependencies: &[LocalTaskDependency],
) -> Value {
    let input = task.input.as_object();
    let mut prerequisite_task_ids = dependencies
        .iter()
        .filter(|edge| edge.task_id == task.task_id)
        .map(|edge| edge.prerequisite_task_id.clone())
        .collect::<BTreeSet<_>>();
    prerequisite_task_ids.extend(
        input
            .and_then(|input| input.get("prerequisite_task_ids"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
    );
    json!({
        "id": task.task_id,
        "title": task.title,
        "description": input.and_then(|input| input.get("description")).cloned().unwrap_or(Value::Null),
        "objective": input.and_then(|input| input.get("objective")).cloned().unwrap_or(Value::Null),
        "expected_outputs": input.and_then(|input| input.get("expected_outputs")).cloned().unwrap_or_else(|| json!([])),
        "acceptance_criteria": input.and_then(|input| input.get("acceptance_criteria")).cloned().unwrap_or_else(|| json!([])),
        "acceptance_report_version": task.input.get("acceptance_report_version"),
        "final_response_max_chars": task.input.get("final_response_max_chars"),
        "graph_goal": input.and_then(|input| input.get("graph_goal")).cloned().unwrap_or(Value::Null),
        "decomposition_rationale": input.and_then(|input| input.get("decomposition_rationale")).cloned().unwrap_or(Value::Null),
        "graph_notification_policy": input.and_then(|input| input.get("graph_notification_policy")).cloned().unwrap_or(Value::Null),
        "notification_milestone": input.and_then(|input| input.get("notification_milestone")).cloned().unwrap_or(Value::Bool(false)),
        "final_delivery": input.and_then(|input| input.get("final_delivery")).cloned().unwrap_or(Value::Bool(false)),
        "input_payload": input.and_then(|input| input.get("input_payload")).cloned().unwrap_or(Value::Null),
        "status": task_status_for_agent(task),
        "execution_status": task_status_for_agent(task),
        "result_status": task_result_status_for_agent(task),
        "effective_status": task_effective_status_for_agent(task),
        "result_reason": task_result_reason_for_agent(task),
        "cancel_reason": task.cancel_reason,
        "replacement_task_ids": task.replacement_task_ids,
        "cancelled_because_task_id": task.cancelled_because_task_id,
        "is_superseded": !task.replacement_task_ids.is_empty(),
        "followup_review_state": task.input.get("followup_review_state"),
        "priority": input.and_then(|input| input.get("priority")).cloned().unwrap_or_else(|| Value::from(0)),
        "tags": input.and_then(|input| input.get("tags")).cloned().unwrap_or_else(|| json!([])),
        "result_summary": task.result_summary.clone(),
        "last_run_id": task.active_run_id,
        "schedule": input.and_then(|input| input.get("schedule")).cloned().unwrap_or_else(|| json!({"mode": "contact_async"})),
        "parent_task_id": input.and_then(|input| input.get("parent_task_id")).cloned().unwrap_or(Value::Null),
        "source_run_id": input.and_then(|input| input.get("source_run_id")).cloned().unwrap_or(Value::Null),
        "prerequisite_task_ids": prerequisite_task_ids,
        "supersedes_task_ids": input.and_then(|input| input.get("supersedes_task_ids")).cloned().unwrap_or_else(|| json!([])),
        "created_at": unix_ms_rfc3339(task.created_at_unix_ms),
        "updated_at": unix_ms_rfc3339(task.updated_at_unix_ms),
    })
}

pub(in crate::application) fn task_result_status_for_agent(task: &LocalTaskRecord) -> Option<&str> {
    if matches!(
        task.status,
        LocalTaskStatus::Pending | LocalTaskStatus::Ready | LocalTaskStatus::Running
    ) {
        return None;
    }
    task.result_status.as_deref().or_else(|| {
        (task.status == LocalTaskStatus::Succeeded).then_some(
            if task
                .input
                .get("acceptance_report_version")
                .and_then(Value::as_u64)
                == Some(1)
            {
                "missing"
            } else {
                "succeeded"
            },
        )
    })
}

pub(in crate::application) fn task_effective_status_for_agent(task: &LocalTaskRecord) -> &str {
    if task.status == LocalTaskStatus::Succeeded {
        task_result_status_for_agent(task).unwrap_or("missing")
    } else {
        task_status_for_agent(task)
    }
}

pub(in crate::application) fn task_result_reason_for_agent(task: &LocalTaskRecord) -> Option<&str> {
    task_result_status_for_agent(task).and(task.result_reason.as_deref())
}

pub(in crate::application) fn task_satisfies_dependency(task: &LocalTaskRecord) -> bool {
    task_effective_status_for_agent(task) == "succeeded"
}

pub(in crate::application) fn tasks_for_agent_tool(tasks: &[LocalTaskRecord]) -> Value {
    Value::Array(
        tasks
            .iter()
            .map(|task| task_for_agent_tool(task, &[]))
            .collect(),
    )
}

pub(in crate::application) fn batch_creation_value(
    graph: &LocalTaskGraph,
    bindings: &[CreatedTaskBinding],
    diagnostics: Option<&DependencyReduction>,
    idempotent_reused: bool,
    auto_started_runs: &[LocalAgentRunRecord],
) -> Value {
    let refs_by_task = bindings
        .iter()
        .map(|binding| (binding.task_id.as_str(), binding.client_ref.as_str()))
        .collect::<HashMap<_, _>>();
    let created_tasks = graph
        .tasks
        .iter()
        .map(|task| {
            let mut value = json!({
                "task_id": task.task_id,
                "title": task.title,
                "status": task_status_for_agent(task),
                "execution_status": task_status_for_agent(task),
                "result_status": task_result_status_for_agent(task),
                "effective_status": task_effective_status_for_agent(task),
                "result_reason": task_result_reason_for_agent(task),
            });
            if let Some(client_ref) = refs_by_task.get(task.task_id.as_str()) {
                value["client_ref"] = Value::String((*client_ref).to_string());
            }
            value
        })
        .collect::<Vec<_>>();
    let mut dependency_edges = if idempotent_reused {
        Vec::new()
    } else {
        graph
            .dependencies
            .iter()
            .map(|edge| {
                json!({
                    "task_id": edge.task_id,
                    "prerequisite_task_id": edge.prerequisite_task_id,
                })
            })
            .collect::<Vec<_>>()
    };
    if !idempotent_reused {
        for task in &graph.tasks {
            let Some(external) = task
                .input
                .get("prerequisite_task_ids")
                .and_then(Value::as_array)
            else {
                continue;
            };
            dependency_edges.extend(external.iter().filter_map(Value::as_str).map(|id| {
                json!({
                    "task_id": task.task_id,
                    "prerequisite_task_id": id,
                })
            }));
        }
    }
    let removed = diagnostics
        .map(|diagnostics| {
            diagnostics
                .removed_edges
                .iter()
                .map(|(dependent_id, prerequisite_id)| {
                    json!({
                        "dependent_id": dependent_id,
                        "prerequisite_id": prerequisite_id,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let submitted_edge_count = diagnostics
        .map(|diagnostics| diagnostics.submitted_edge_count)
        .unwrap_or(dependency_edges.len());
    let persisted_edge_count = diagnostics
        .map(|diagnostics| diagnostics.persisted_edge_count)
        .unwrap_or(dependency_edges.len());
    json!({
        "idempotent_reused": idempotent_reused,
        "created_tasks": created_tasks,
        "dependency_edges": dependency_edges,
        "removed_redundant_edges": removed,
        "dependency_diagnostics": {
            "submitted_edge_count": submitted_edge_count,
            "persisted_edge_count": persisted_edge_count,
        },
        "auto_started_runs": auto_started_runs.iter().map(|run| json!({
            "run_id": run.run_id,
            "task_id": run.owner_entity_id,
            "status": run.status.as_str(),
        })).collect::<Vec<_>>(),
    })
}

pub(in crate::application) fn cancellation_value(
    graph: &LocalTaskGraph,
    task_id: &str,
    reason: &str,
    previous_active_run_id: Option<&str>,
) -> Result<Value, String> {
    let task = graph
        .tasks
        .iter()
        .find(|task| task.task_id == task_id)
        .ok_or_else(|| format!("cancelled Task not found in Task Graph: {task_id}"))?;
    let cascade_cancelled_task_ids = graph
        .tasks
        .iter()
        .filter(|candidate| {
            candidate.task_id != task_id && candidate.status == LocalTaskStatus::Cancelled
        })
        .map(|candidate| candidate.task_id.clone())
        .collect::<Vec<_>>();
    Ok(json!({
        "cancelled": task.status == LocalTaskStatus::Cancelled || !cascade_cancelled_task_ids.is_empty(),
        "task_id": task.task_id,
        "status": task_status_for_agent(task),
        "reason": reason,
        "active_run_ids": previous_active_run_id.into_iter().collect::<Vec<_>>(),
        "cascade_cancelled_task_ids": cascade_cancelled_task_ids,
        "callback_event": "task.cancelled",
        "task": task_for_agent_tool(task, &graph.dependencies),
    }))
}

pub(in crate::application) fn task_status_for_agent(task: &LocalTaskRecord) -> &'static str {
    if task.status == LocalTaskStatus::Ready && task.active_run_id.is_some() {
        return "queued";
    }
    match task.status {
        LocalTaskStatus::Pending | LocalTaskStatus::Ready => "ready",
        LocalTaskStatus::Running => "running",
        LocalTaskStatus::Succeeded => "succeeded",
        LocalTaskStatus::Failed => "failed",
        LocalTaskStatus::Cancelled => "cancelled",
        LocalTaskStatus::Blocked => "blocked",
    }
}

fn unix_ms_rfc3339(value: i64) -> Value {
    DateTime::from_timestamp_millis(value)
        .map(|value| Value::String(value.to_rfc3339()))
        .unwrap_or(Value::Null)
}

#[cfg(test)]
mod result_tests {
    use super::*;

    #[test]
    fn task_tool_payload_returns_the_latest_model_output() {
        let task = LocalTaskRecord {
            graph_id: "graph-1".to_string(),
            owner_user_id: "user-1".to_string(),
            source_entity_type: "conversation_turn".to_string(),
            source_entity_id: "turn-1".to_string(),
            task_id: "task-1".to_string(),
            title: "Inspect project".to_string(),
            profile_key: "task_execution".to_string(),
            model_config_ref: "model-1".to_string(),
            model_config_revision: "revision-1".to_string(),
            capability_policy_revision: "policy-1".to_string(),
            input: json!({"objective": "Inspect the project"}),
            max_iterations: 600,
            status: LocalTaskStatus::Succeeded,
            active_run_id: None,
            cancel_reason: None,
            replacement_task_ids: Vec::new(),
            cancelled_because_task_id: None,
            result_summary: Some("Godot project; run with godot --path .".to_string()),
            result_status: Some("succeeded".to_string()),
            result_reason: Some("Inspection completed.".to_string()),
            version: 2,
            created_at_unix_ms: 1,
            updated_at_unix_ms: 2,
        };

        let output = task_for_agent_tool(&task, &[]);
        assert_eq!(
            output["result_summary"],
            "Godot project; run with godot --path ."
        );
        assert_eq!(output["execution_status"], "succeeded");
        assert_eq!(output["result_status"], "succeeded");
        assert_eq!(output["effective_status"], "succeeded");

        let blocked = LocalTaskRecord {
            result_status: Some("blocked".to_string()),
            result_reason: Some("External approval is required.".to_string()),
            ..task
        };
        let output = task_for_agent_tool(&blocked, &[]);
        assert_eq!(output["execution_status"], "succeeded");
        assert_eq!(output["result_status"], "blocked");
        assert_eq!(output["effective_status"], "blocked");
        assert_eq!(output["result_reason"], "External approval is required.");
        assert!(!task_satisfies_dependency(&blocked));
    }
}
