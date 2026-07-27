// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, Query, State};
use axum::Json;
use chatos_project_execution::{
    read_planning_feedback_history, ExecutionPlanIdentity, ExecutionPlane, STATUS_PLANNING_STARTED,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::local_runtime::storage::LocalMessageRecord;
use crate::local_runtime::task_board::LocalTaskBoardTaskRecord;
use crate::LocalRuntime;

use super::super::super::context::owner_context;
use super::super::super::error::LocalRuntimeApiError;

#[derive(Debug, Default, Deserialize)]
pub(in crate::local_runtime::api::task_runs) struct RequirementExecutionPlanQuery {
    #[serde(alias = "executionGroupId")]
    execution_group_id: Option<String>,
    #[serde(alias = "conversationId")]
    conversation_id: Option<String>,
}

pub(in crate::local_runtime::api::task_runs) async fn get_requirement_execution_plan(
    Path((project_id, requirement_id)): Path<(String, String)>,
    State(runtime): State<LocalRuntime>,
    Query(query): Query<RequirementExecutionPlanQuery>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let database = runtime.local_database()?;
    let project = database
        .get_project(project_id.as_str(), owner.owner_user_id.as_str())
        .await?
        .ok_or_else(|| {
            LocalRuntimeApiError::not_found(
                "local_project_not_found",
                "Local project was not found",
            )
        })?;
    if project.execution_plane != "local_connector" {
        return Err(LocalRuntimeApiError::conflict(
            "local_execution_plane_mismatch",
            "Local requirement execution is only available for local_connector projects",
        ));
    }
    let precise_identity = match (query.conversation_id, query.execution_group_id) {
        (Some(conversation_id), Some(execution_group_id)) => Some(
            ExecutionPlanIdentity::required(execution_group_id.as_str(), conversation_id.as_str())
                .map_err(|message| {
                    LocalRuntimeApiError::bad_request(
                        "local_execution_plan_identity_required",
                        message,
                    )
                })?,
        ),
        (None, None) => None,
        _ => {
            return Err(LocalRuntimeApiError::bad_request(
                "local_execution_plan_identity_incomplete",
                "conversation_id and execution_group_id must be provided together",
            ))
        }
    };
    let tasks = database
        .list_local_project_execution_tasks(owner.owner_user_id.as_str(), project_id.as_str())
        .await?;

    if let Some(identity) = precise_identity {
        let messages = database
            .list_turn_messages(
                owner.owner_user_id.as_str(),
                identity.execution_group_id.as_str(),
            )
            .await?;
        let Some(source) = messages.into_iter().find(|message| message.role == "user") else {
            return Ok(Json(json!({
                "found": false,
                "execution_plane": ExecutionPlane::LocalConnector.as_str(),
                "project_id": project_id,
                "requirement_id": requirement_id,
                "conversation_id": identity.conversation_id,
                "execution_group_id": identity.execution_group_id,
            })));
        };
        validate_source(
            &source,
            identity.conversation_id.as_str(),
            project_id.as_str(),
            requirement_id.as_str(),
        )?;
        let current_tasks = tasks
            .iter()
            .filter(|task| {
                task.execution_group_id.as_deref() == Some(identity.execution_group_id.as_str())
                    && task.conversation_id == identity.conversation_id
            })
            .collect::<Vec<_>>();
        return Ok(Json(build_response(
            project_id.as_str(),
            requirement_id.as_str(),
            source,
            current_tasks.as_slice(),
        )));
    }

    let sessions = database
        .list_sessions(owner.owner_user_id.as_str(), project_id.as_str())
        .await?;
    let mut latest_source = None;
    for session in sessions {
        let messages = database
            .list_messages(owner.owner_user_id.as_str(), session.id.as_str())
            .await?;
        for source in messages {
            if source.role != "user"
                || validate_source(
                    &source,
                    session.id.as_str(),
                    project_id.as_str(),
                    requirement_id.as_str(),
                )
                .is_err()
            {
                continue;
            }
            let replace = latest_source
                .as_ref()
                .is_none_or(|current| local_execution_message_is_newer(&source, current));
            if replace {
                latest_source = Some(source);
            }
        }
    }
    if let Some(source) = latest_source {
        let conversation_id = source.session_id.clone();
        let execution_group_id = source_execution_group_id(&source);
        let current_tasks = tasks
            .iter()
            .filter(|task| {
                task.execution_group_id.as_deref() == Some(execution_group_id.as_str())
                    && task.conversation_id == conversation_id
            })
            .collect::<Vec<_>>();
        return Ok(Json(build_response(
            project_id.as_str(),
            requirement_id.as_str(),
            source,
            current_tasks.as_slice(),
        )));
    }

    Ok(Json(json!({
        "found": false,
        "execution_plane": ExecutionPlane::LocalConnector.as_str(),
        "project_id": project_id,
        "requirement_id": requirement_id,
    })))
}

fn metadata(message: &LocalMessageRecord) -> Value {
    message
        .metadata_json
        .as_deref()
        .and_then(|value| serde_json::from_str::<Value>(value).ok())
        .unwrap_or_else(|| json!({}))
}

fn source_execution_group_id(message: &LocalMessageRecord) -> String {
    metadata(message)
        .get("project_requirement_execution")
        .and_then(|value| value.get("execution_group_id"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| message.turn_id.clone())
        .unwrap_or_else(|| message.id.clone())
}

fn local_execution_message_is_newer(
    candidate: &LocalMessageRecord,
    current: &LocalMessageRecord,
) -> bool {
    (candidate.created_at.as_str(), candidate.id.as_str())
        > (current.created_at.as_str(), current.id.as_str())
}

fn validate_source(
    message: &LocalMessageRecord,
    conversation_id: &str,
    project_id: &str,
    requirement_id: &str,
) -> Result<(), LocalRuntimeApiError> {
    if message.session_id != conversation_id {
        return Err(LocalRuntimeApiError::conflict(
            "local_execution_plan_conversation_mismatch",
            "Local execution plan does not belong to this conversation",
        ));
    }
    let metadata = metadata(message);
    let execution = metadata.get("project_requirement_execution");
    if execution
        .and_then(|value| value.get("project_id"))
        .and_then(Value::as_str)
        != Some(project_id)
        || execution
            .and_then(|value| value.get("requirement_id"))
            .and_then(Value::as_str)
            != Some(requirement_id)
    {
        return Err(LocalRuntimeApiError::conflict(
            "local_execution_plan_scope_mismatch",
            "Local execution plan does not belong to this project requirement",
        ));
    }
    Ok(())
}

fn message_status(message: &LocalMessageRecord) -> String {
    let metadata = metadata(message);
    let task_runner = metadata.get("task_runner_async");
    task_runner
        .and_then(|value| value.get("overall_status"))
        .and_then(Value::as_str)
        .or_else(|| {
            task_runner
                .and_then(|value| value.get("confirmation_status"))
                .and_then(Value::as_str)
        })
        .unwrap_or(STATUS_PLANNING_STARTED)
        .trim()
        .to_ascii_lowercase()
}

fn build_response(
    project_id: &str,
    requirement_id: &str,
    message: LocalMessageRecord,
    tasks: &[&LocalTaskBoardTaskRecord],
) -> Value {
    let metadata = metadata(&message);
    let execution = metadata.get("project_requirement_execution");
    let task_runner = metadata.get("task_runner_async");
    let execution_group_id = execution
        .and_then(|value| value.get("execution_group_id"))
        .and_then(Value::as_str)
        .unwrap_or_else(|| message.turn_id.as_deref().unwrap_or(message.id.as_str()));
    let status = message_status(&message);
    let confirmation_status = task_runner
        .and_then(|value| value.get("confirmation_status"))
        .and_then(Value::as_str)
        .unwrap_or(status.as_str());
    let execution_paused = task_runner
        .and_then(|value| value.get("execution_paused"))
        .and_then(Value::as_bool)
        .unwrap_or_else(|| status == "paused");
    json!({
        "found": true,
        "execution_plane": ExecutionPlane::LocalConnector.as_str(),
        "project_id": project_id,
        "requirement_id": requirement_id,
        "conversation_id": message.session_id,
        "execution_group_id": execution_group_id,
        "message_id": message.id,
        "model_config_id": metadata.get("model_config_id").and_then(Value::as_str),
        "include_prerequisite_dependents": execution
            .and_then(|value| value.get("include_prerequisite_dependents"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
        "planning_feedback": execution
            .and_then(|value| value.get("planning_feedback"))
            .and_then(Value::as_str),
        "planning_feedback_history": read_planning_feedback_history(execution),
        "status": status,
        "confirmation_status": confirmation_status,
        "execution_paused": execution_paused,
        "task_count": tasks.len(),
        "has_started_runs": tasks.iter().any(|task| task.last_run_id.is_some()),
        "created_at": message.created_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source_message(id: &str, created_at: &str, status: &str) -> LocalMessageRecord {
        LocalMessageRecord {
            id: id.to_string(),
            session_id: "session-1".to_string(),
            turn_id: Some(id.to_string()),
            sequence_no: 1,
            role: "user".to_string(),
            content: id.to_string(),
            reasoning: None,
            tool_calls_json: None,
            tool_call_id: None,
            metadata_json: Some(
                json!({
                    "project_requirement_execution": {
                        "project_id": "project-1",
                        "requirement_id": "requirement-1",
                        "execution_group_id": id,
                    },
                    "task_runner_async": { "overall_status": status }
                })
                .to_string(),
            ),
            created_at: created_at.to_string(),
        }
    }

    #[test]
    fn latest_local_execution_message_is_restorable_without_created_tasks() {
        let stopped = source_message("group-old", "2026-07-23T05:00:00Z", "stopped");
        let failed = source_message("group-new", "2026-07-23T06:00:00Z", "failed");

        assert!(local_execution_message_is_newer(&failed, &stopped));
        assert_eq!(source_execution_group_id(&failed), "group-new");
        assert_eq!(message_status(&failed), "failed");
    }
}
