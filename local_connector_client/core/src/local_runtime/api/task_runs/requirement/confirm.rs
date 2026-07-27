// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;

use axum::extract::{Path, State};
use axum::Json;
use chatos_project_execution::{
    execution_task_status_blocks_confirmation, validate_exact_project_task_scope,
    ExecutionPlanIdentity, ExecutionPlane, STATUS_ALREADY_CONFIRMED, STATUS_EXECUTION_STARTED,
};
use serde_json::{json, Value};

use crate::local_runtime::project_management::{
    UpdateLocalRequirementInput, UpdateLocalWorkItemInput,
};
use crate::LocalRuntime;

use super::super::super::context::owner_context;
use super::super::super::error::LocalRuntimeApiError;
use super::ConfirmRequirementExecutionPayload;

pub(in crate::local_runtime::api::task_runs) async fn confirm_requirement_execution(
    Path((project_id, requirement_id)): Path<(String, String)>,
    State(runtime): State<LocalRuntime>,
    Json(payload): Json<ConfirmRequirementExecutionPayload>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let identity = ExecutionPlanIdentity::required(
        payload.execution_group_id.as_str(),
        payload.conversation_id.as_str(),
    )
    .map_err(|message| {
        LocalRuntimeApiError::bad_request("local_execution_plan_identity_required", message)
    })?;
    let execution_group_id = identity.execution_group_id;
    let conversation_id = identity.conversation_id;
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
    let expected_project_task_ids = load_expected_project_task_ids(
        database,
        owner.owner_user_id.as_str(),
        execution_group_id.as_str(),
        conversation_id.as_str(),
        project_id.as_str(),
        requirement_id.as_str(),
    )
    .await?;
    let tasks = database
        .list_local_project_execution_tasks(owner.owner_user_id.as_str(), project_id.as_str())
        .await?
        .into_iter()
        .filter(|task| {
            task.execution_group_id.as_deref() == Some(execution_group_id.as_str())
                && task.conversation_id == conversation_id
        })
        .collect::<Vec<_>>();
    if tasks.is_empty() {
        return Err(LocalRuntimeApiError::conflict(
            "local_execution_plan_not_ready",
            "The local execution task graph is not ready yet",
        ));
    }
    let actual_project_task_ids = tasks
        .iter()
        .filter_map(|task| task.project_work_item_id.clone())
        .collect::<BTreeSet<_>>();
    if let Err(mismatch) =
        validate_exact_project_task_scope(&expected_project_task_ids, &actual_project_task_ids)
    {
        return Err(LocalRuntimeApiError::conflict(
            "local_execution_plan_incomplete",
            format!(
                "The local execution task graph is incomplete; missing=[{}], unexpected=[{}]",
                mismatch.missing.join(","),
                mismatch.unexpected.join(",")
            ),
        ));
    }
    if tasks
        .iter()
        .any(|task| task.last_run_id.is_none() && task.status != "todo")
    {
        return Err(LocalRuntimeApiError::conflict(
            "local_execution_plan_not_startable",
            "The local execution task graph contains cancelled, blocked, or otherwise non-startable tasks",
        ));
    }
    if tasks
        .iter()
        .any(|task| execution_task_status_blocks_confirmation(task.status.as_str()))
    {
        return Err(LocalRuntimeApiError::conflict(
            "local_execution_plan_terminal_failure",
            "The local execution task graph contains failed or cancelled tasks and cannot be confirmed",
        ));
    }

    let mut started_runs = Vec::new();
    let mut project_work_item_ids = BTreeSet::new();
    let mut executing_requirement_ids = BTreeSet::new();
    for task in &tasks {
        if matches!(task.status.as_str(), "todo" | "doing") {
            if let Some(project_work_item_id) = task.project_work_item_id.as_deref() {
                project_work_item_ids.insert(project_work_item_id.to_string());
            }
            if let Some(requirement_id) = task.requirement_id.as_deref() {
                executing_requirement_ids.insert(requirement_id.to_string());
            }
        }
        if let Some(run) = database
            .enqueue_deferred_local_conversation_task(
                owner.owner_user_id.as_str(),
                project_id.as_str(),
                task,
            )
            .await?
        {
            started_runs.push(run);
        }
    }
    for work_item_id in &project_work_item_ids {
        database
            .update_local_work_item(
                owner.owner_user_id.as_str(),
                work_item_id.as_str(),
                UpdateLocalWorkItemInput {
                    status: Some("in_progress".to_string()),
                    ..Default::default()
                },
            )
            .await?;
    }
    for executing_requirement_id in &executing_requirement_ids {
        database
            .update_local_requirement(
                owner.owner_user_id.as_str(),
                executing_requirement_id.as_str(),
                UpdateLocalRequirementInput {
                    status: Some("in_progress".to_string()),
                    ..Default::default()
                },
            )
            .await?;
    }
    database
        .set_turn_messages_hidden(
            owner.owner_user_id.as_str(),
            execution_group_id.as_str(),
            false,
        )
        .await?;

    Ok(Json(json!({
        "success": true,
        "status": if started_runs.is_empty() { STATUS_ALREADY_CONFIRMED } else { STATUS_EXECUTION_STARTED },
        "execution_plane": ExecutionPlane::LocalConnector.as_str(),
        "project_id": project_id,
        "requirement_id": requirement_id,
        "conversation_id": conversation_id,
        "execution_group_id": execution_group_id,
        "task_ids": tasks.iter().map(|task| task.id.clone()).collect::<Vec<_>>(),
        "root_task_ids": tasks.iter()
            .filter(|task| task.prerequisite_task_ids.is_empty())
            .map(|task| task.id.clone())
            .collect::<Vec<_>>(),
        "started_runs": started_runs,
    })))
}

pub(super) async fn load_expected_project_task_ids(
    database: &crate::local_runtime::LocalDatabase,
    owner_user_id: &str,
    execution_group_id: &str,
    conversation_id: &str,
    project_id: &str,
    requirement_id: &str,
) -> Result<BTreeSet<String>, LocalRuntimeApiError> {
    let messages = database
        .list_turn_messages(owner_user_id, execution_group_id)
        .await?;
    let user_message = messages
        .iter()
        .find(|message| message.role == "user")
        .ok_or_else(|| {
            LocalRuntimeApiError::conflict(
                "local_execution_plan_source_missing",
                "The local execution plan source message is missing",
            )
        })?;
    if user_message.session_id != conversation_id {
        return Err(LocalRuntimeApiError::conflict(
            "local_execution_plan_conversation_mismatch",
            "The local execution plan does not belong to this conversation",
        ));
    }
    let metadata = user_message
        .metadata_json
        .as_deref()
        .and_then(|value| serde_json::from_str::<Value>(value).ok())
        .unwrap_or_else(|| json!({}));
    let execution = metadata.get("project_requirement_execution");
    let source_project_id = execution
        .and_then(|value| value.get("project_id"))
        .and_then(Value::as_str);
    let source_requirement_id = execution
        .and_then(|value| value.get("requirement_id"))
        .and_then(Value::as_str);
    if source_project_id != Some(project_id) || source_requirement_id != Some(requirement_id) {
        return Err(LocalRuntimeApiError::conflict(
            "local_execution_plan_scope_mismatch",
            "The local execution plan does not belong to this project requirement",
        ));
    }
    let expected = execution
        .and_then(|value| value.get("project_task_ids"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect::<BTreeSet<_>>();
    if expected.is_empty() {
        return Err(LocalRuntimeApiError::conflict(
            "local_execution_plan_scope_missing",
            "The local execution plan is missing its selected project task scope",
        ));
    }
    Ok(expected)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;
    use uuid::Uuid;

    use super::load_expected_project_task_ids;
    use crate::local_runtime::storage::{
        BeginLocalTurnInput, CreateLocalSessionInput, LocalDatabase, UpsertLocalProjectInput,
    };

    #[tokio::test]
    async fn confirmation_scope_comes_from_the_original_local_planner_message() {
        let root =
            std::env::temp_dir().join(format!("chatos-local-confirm-scope-{}", Uuid::new_v4()));
        fs::create_dir_all(root.as_path()).expect("create local confirmation test directory");
        let database = LocalDatabase::open(root.join("runtime.sqlite3"))
            .await
            .expect("open local confirmation database");
        database
            .upsert_project(UpsertLocalProjectInput {
                project_id: "project-1".to_string(),
                owner_user_id: "user-1".to_string(),
                device_id: "device-1".to_string(),
                workspace_id: "workspace-1".to_string(),
                project_name: "Project 1".to_string(),
                root_relative_path: None,
            })
            .await
            .expect("create local project");
        let session = database
            .create_session(CreateLocalSessionInput {
                project_id: "project-1".to_string(),
                owner_user_id: "user-1".to_string(),
                title: "Execution plan".to_string(),
                selected_model_id: Some("model-1".to_string()),
                selected_agent_id: None,
            })
            .await
            .expect("create local session");
        let session_id = session.id.clone();
        database
            .begin_turn(BeginLocalTurnInput {
                session_id: session_id.clone(),
                owner_user_id: "user-1".to_string(),
                turn_id: "execution-group-1".to_string(),
                idempotency_key: "execution-group-1".to_string(),
                content: "Generate execution plan".to_string(),
                metadata_json: Some(
                    json!({
                        "project_requirement_execution": {
                            "project_id": "project-1",
                            "requirement_id": "requirement-1",
                            "project_task_ids": ["task-1", "task-2"]
                        }
                    })
                    .to_string(),
                ),
            })
            .await
            .expect("create local planner turn");

        let expected = load_expected_project_task_ids(
            &database,
            "user-1",
            "execution-group-1",
            session_id.as_str(),
            "project-1",
            "requirement-1",
        )
        .await
        .expect("load original local execution scope");
        assert_eq!(
            expected,
            std::collections::BTreeSet::from(["task-1".to_string(), "task-2".to_string()])
        );

        database.close().await;
        fs::remove_dir_all(root).expect("remove local confirmation test directory");
    }
}
