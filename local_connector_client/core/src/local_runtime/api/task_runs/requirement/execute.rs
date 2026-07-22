// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, State};
use axum::Json;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::local_runtime::project_management::UpdateLocalRequirementInput;
use crate::local_runtime::storage::CreateLocalSessionInput;
use crate::local_runtime::EnqueueLocalTaskRunInput;
use crate::LocalRuntime;

use super::super::super::context::owner_context;
use super::super::super::error::LocalRuntimeApiError;
use super::super::prompt::task_run_prompt;
use super::ExecuteRequirementPayload;

pub(in crate::local_runtime::api::task_runs) async fn execute_requirement(
    Path((project_id, requirement_id)): Path<(String, String)>,
    State(runtime): State<LocalRuntime>,
    Json(_payload): Json<ExecuteRequirementPayload>,
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
    let requirement = database
        .get_local_requirement(owner.owner_user_id.as_str(), requirement_id.as_str())
        .await?
        .filter(|record| record.project_id == project_id)
        .ok_or_else(|| {
            LocalRuntimeApiError::not_found(
                "local_requirement_not_found",
                "Local requirement was not found",
            )
        })?;
    ensure_no_active_runs(
        database,
        owner.owner_user_id.as_str(),
        project_id.as_str(),
        requirement_id.as_str(),
    )
    .await?;
    let work_items = database
        .list_local_work_items_for_requirement(
            owner.owner_user_id.as_str(),
            project_id.as_str(),
            requirement_id.as_str(),
            false,
        )
        .await?
        .into_iter()
        .filter(|item| {
            !crate::local_runtime::project_management::is_completed_project_status(
                item.status.as_str(),
            ) && item.status != "archived"
        })
        .collect::<Vec<_>>();
    if work_items.is_empty() {
        return Err(LocalRuntimeApiError::bad_request(
            "local_requirement_has_no_pending_tasks",
            "This local requirement has no pending work items",
        ));
    }
    let model_config_id = project_agent_model_id(&runtime).await?;
    let session = database
        .create_session(CreateLocalSessionInput {
            project_id: project.project_id.clone(),
            owner_user_id: owner.owner_user_id.clone(),
            title: format!("执行需求：{}", requirement.title),
            selected_model_id: Some(model_config_id.clone()),
            selected_agent_id: None,
        })
        .await?;
    let documents = database
        .list_local_requirement_documents(
            owner.owner_user_id.as_str(),
            project_id.as_str(),
            requirement_id.as_str(),
        )
        .await?;
    let execution_group_id = format!("lc_execution_group_{}", Uuid::new_v4());
    let mut runs = Vec::new();
    for item in &work_items {
        runs.push(
            database
                .enqueue_local_task_run(EnqueueLocalTaskRunInput {
                    owner_user_id: owner.owner_user_id.clone(),
                    project_id: project_id.clone(),
                    requirement_id: Some(requirement_id.clone()),
                    task_kind: "project_work_item".to_string(),
                    task_id: item.id.clone(),
                    session_id: session.id.clone(),
                    execution_group_id: execution_group_id.clone(),
                    priority: item.priority,
                    prompt: task_run_prompt(&requirement, item, documents.as_slice()),
                    model_config_id: model_config_id.clone(),
                })
                .await?,
        );
    }
    database
        .update_local_requirement(
            owner.owner_user_id.as_str(),
            requirement_id.as_str(),
            UpdateLocalRequirementInput {
                status: Some("in_progress".to_string()),
                ..Default::default()
            },
        )
        .await?;
    Ok(Json(json!({
        "success": true, "status": "queued", "project_id": project_id,
        "requirement_id": requirement_id, "conversation_id": session.id,
        "message": null, "execution_group_id": execution_group_id,
        "planner_agent_key": "local_task_runner", "plan_mode_enabled": false,
        "runs": runs,
    })))
}

async fn ensure_no_active_runs(
    database: &crate::local_runtime::LocalDatabase,
    owner_user_id: &str,
    project_id: &str,
    requirement_id: &str,
) -> Result<(), LocalRuntimeApiError> {
    let existing = database
        .list_local_requirement_task_runs(owner_user_id, project_id, requirement_id)
        .await?;
    if existing
        .iter()
        .any(|run| matches!(run.status.as_str(), "queued" | "running"))
    {
        return Err(LocalRuntimeApiError::conflict(
            "local_requirement_execution_active",
            "This local requirement already has active task runs",
        ));
    }
    Ok(())
}

async fn project_agent_model_id(runtime: &LocalRuntime) -> Result<String, LocalRuntimeApiError> {
    runtime
        .state
        .read()
        .await
        .model_configs
        .settings
        .project_management_agent_model_config_id
        .clone()
        .ok_or_else(|| {
            LocalRuntimeApiError::conflict(
                "local_task_runner_model_required",
                "Configure the Project Management Agent model in Local Connector first",
            )
        })
}
