// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(super) async fn confirm_chatos_project_execution(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ConfirmChatosProjectExecutionRequest>,
) -> Result<Json<Value>, InternalApiError> {
    let identity = require_task_runner_internal_request(
        &state.config,
        &headers,
        &[CHATOS_CALLER],
        CHATOS_EXECUTION_START_SCOPE,
    )
    .map_err(|error| InternalApiError {
        status: error.status,
        message: error.message,
    })?;
    let project_id = request.project_id.trim();
    let requirement_id = request.requirement_id.trim();
    let source_session_id = request.source_session_id.trim();
    let source_user_message_id = request.source_user_message_id.trim();
    if project_id.is_empty()
        || requirement_id.is_empty()
        || source_session_id.is_empty()
        || source_user_message_id.is_empty()
    {
        return Err(InternalApiError::bad_request(
            "project_id, requirement_id, source_session_id and source_user_message_id are required",
        ));
    }
    let mut audit = TaskRunnerInternalAuditGuard::new(
        &identity,
        Some(project_id),
        "project_execution",
        requirement_id,
        "confirm",
    );

    let tasks = state
        .task_service
        .list_tasks_for_chatos_source(source_session_id, Some(source_user_message_id), None)
        .await
        .map_err(InternalApiError::internal)?;
    if tasks.is_empty() {
        return Err(InternalApiError::not_found(
            "project execution task graph is not ready",
        ));
    }
    enrich_project_execution_audit(&mut audit, tasks.as_slice());
    for task in &tasks {
        let payload = task.input_payload.as_ref();
        let payload_source = payload
            .and_then(|value| value.get("source"))
            .and_then(Value::as_str);
        let payload_requirement_id = payload
            .and_then(|value| value.get("root_requirement_id"))
            .or_else(|| payload.and_then(|value| value.get("requirement_id")))
            .and_then(Value::as_str);
        if task.project_id.as_deref() != Some(project_id)
            || payload_source != Some("chatos_project_requirement_execution")
            || payload_requirement_id != Some(requirement_id)
        {
            return Err(InternalApiError::conflict(
                "task graph does not belong to the requested project requirement execution",
            ));
        }
        if matches!(
            task.status,
            TaskStatus::Failed | TaskStatus::Blocked | TaskStatus::Cancelled | TaskStatus::Archived
        ) {
            return Err(InternalApiError::conflict(
                "project execution task graph contains failed or cancelled tasks",
            ));
        }
    }

    let root_task_ids = tasks
        .iter()
        .filter(|task| task.prerequisite_task_ids.is_empty())
        .map(|task| task.id.clone())
        .collect::<Vec<_>>();
    if root_task_ids.is_empty() {
        return Err(InternalApiError::conflict(
            "project execution task graph has no runnable roots",
        ));
    }
    let already_confirmed = tasks
        .iter()
        .any(|task| task.last_run_id.is_some() || task.status != TaskStatus::Ready);
    let started_runs = if already_confirmed {
        let mut existing_runs = Vec::new();
        for run_id in tasks.iter().filter_map(|task| task.last_run_id.as_deref()) {
            if let Some(run) = state
                .run_service
                .get_run(run_id)
                .await
                .map_err(InternalApiError::internal)?
            {
                existing_runs.push(run);
            }
        }
        existing_runs
    } else {
        state
            .run_service
            .dispatch_confirmed_project_execution_tasks(tasks.as_slice())
            .await
            .map_err(InternalApiError::internal)?
    };
    let task_ids = tasks.iter().map(|task| task.id.clone()).collect::<Vec<_>>();
    let response = Json(redact_workspace_paths_internal(
        &state,
        json!({
            "success": true,
            "status": if already_confirmed { STATUS_ALREADY_CONFIRMED } else { STATUS_EXECUTION_STARTED },
            "project_id": project_id,
            "requirement_id": requirement_id,
            "source_session_id": source_session_id,
            "source_user_message_id": source_user_message_id,
            "task_ids": task_ids,
            "root_task_ids": root_task_ids,
            "started_runs": started_runs,
        }),
    )?);
    audit.succeeded();
    Ok(response)
}

pub(super) async fn pause_chatos_project_execution(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<MutateChatosProjectExecutionRequest>,
) -> Result<Json<Value>, InternalApiError> {
    mutate_chatos_project_execution_pause(state, headers, request, true).await
}

pub(super) async fn resume_chatos_project_execution(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<MutateChatosProjectExecutionRequest>,
) -> Result<Json<Value>, InternalApiError> {
    mutate_chatos_project_execution_pause(state, headers, request, false).await
}

async fn mutate_chatos_project_execution_pause(
    state: AppState,
    headers: HeaderMap,
    request: MutateChatosProjectExecutionRequest,
    paused: bool,
) -> Result<Json<Value>, InternalApiError> {
    let identity = require_chatos_execution_mutation(&state, &headers)?;
    let project_id = required_internal_text(request.project_id, "project_id")?;
    let requirement_id = required_internal_text(request.requirement_id, "requirement_id")?;
    let source_session_id = required_internal_text(request.source_session_id, "source_session_id")?;
    let source_user_message_id =
        required_internal_text(request.source_user_message_id, "source_user_message_id")?;
    let mut audit = TaskRunnerInternalAuditGuard::new(
        &identity,
        Some(project_id.as_str()),
        "project_execution",
        requirement_id.as_str(),
        if paused { "pause" } else { "resume" },
    );
    let tasks = state
        .task_service
        .list_tasks_for_chatos_source(
            source_session_id.as_str(),
            Some(source_user_message_id.as_str()),
            None,
        )
        .await
        .map_err(InternalApiError::internal)?;
    if tasks.is_empty() {
        return Err(InternalApiError::not_found(
            "project execution task graph is not ready",
        ));
    }
    enrich_project_execution_audit(&mut audit, tasks.as_slice());
    for task in &tasks {
        validate_project_execution_task(task, project_id.as_str(), requirement_id.as_str())?;
    }
    if !tasks
        .iter()
        .any(|task| task.last_run_id.is_some() || task.status != TaskStatus::Ready)
    {
        return Err(InternalApiError::conflict(
            "project execution has not started yet",
        ));
    }
    let started_runs = state
        .run_service
        .set_project_execution_paused(tasks.as_slice(), paused)
        .await
        .map_err(InternalApiError::internal)?;
    let task_ids = tasks.iter().map(|task| task.id.clone()).collect::<Vec<_>>();
    let mut running_count = 0usize;
    let mut queued_count = 0usize;
    for task_id in &task_ids {
        for run in state
            .run_service
            .list_runs(Some(task_id.as_str()))
            .await
            .map_err(InternalApiError::internal)?
        {
            match run.status {
                TaskRunStatus::Running => running_count += 1,
                TaskRunStatus::Queued => queued_count += 1,
                _ => {}
            }
        }
    }
    let response = Json(redact_workspace_paths_internal(
        &state,
        json!({
            "success": true,
            "status": if paused { STATUS_PAUSED } else { STATUS_EXECUTION_STARTED },
            "execution_paused": paused,
            "pause_scope": "future_dispatch",
            "active_runs_continue": true,
            "project_id": project_id,
            "requirement_id": requirement_id,
            "source_session_id": source_session_id,
            "source_user_message_id": source_user_message_id,
            "task_ids": task_ids,
            "running_count": running_count,
            "queued_count": queued_count,
            "started_runs": started_runs,
        }),
    )?);
    audit.succeeded();
    Ok(response)
}

pub(super) async fn clone_chatos_project_execution(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CloneChatosProjectExecutionRequest>,
) -> Result<Json<Value>, InternalApiError> {
    let identity = require_chatos_execution_mutation(&state, &headers)?;
    let project_id = required_internal_text(request.project_id, "project_id")?;
    let requirement_id = required_internal_text(request.requirement_id, "requirement_id")?;
    let old_source_session_id =
        required_internal_text(request.old_source_session_id, "old_source_session_id")?;
    let old_source_user_message_id = required_internal_text(
        request.old_source_user_message_id,
        "old_source_user_message_id",
    )?;
    let new_source_session_id =
        required_internal_text(request.new_source_session_id, "new_source_session_id")?;
    let new_source_user_message_id = required_internal_text(
        request.new_source_user_message_id,
        "new_source_user_message_id",
    )?;
    let mut audit = TaskRunnerInternalAuditGuard::new(
        &identity,
        Some(project_id.as_str()),
        "project_execution",
        requirement_id.as_str(),
        "clone",
    );
    let cloned = state
        .task_service
        .clone_stopped_project_execution_tasks(
            project_id.as_str(),
            requirement_id.as_str(),
            old_source_session_id.as_str(),
            old_source_user_message_id.as_str(),
            new_source_session_id.as_str(),
            new_source_user_message_id.as_str(),
        )
        .await
        .map_err(InternalApiError::conflict)?;
    let tasks = cloned
        .iter()
        .map(|item| item.task.clone())
        .collect::<Vec<_>>();
    enrich_project_execution_audit(&mut audit, tasks.as_slice());
    let task_mappings = cloned
        .iter()
        .map(|item| {
            json!({
                "old_task_id": item.old_task_id,
                "new_task_id": item.task.id,
                "project_task_id": item.project_task_id,
                "status": "ready",
                "run_id": Value::Null,
            })
        })
        .collect::<Vec<_>>();
    let response = Json(redact_workspace_paths_internal(
        &state,
        json!({
            "success": true,
            "status": STATUS_AWAITING_CONFIRMATION,
            "project_id": project_id,
            "requirement_id": requirement_id,
            "source_session_id": new_source_session_id,
            "source_user_message_id": new_source_user_message_id,
            "task_mappings": task_mappings,
            "task_ids": tasks.iter().map(|task| task.id.clone()).collect::<Vec<_>>(),
            "root_task_ids": tasks.iter().filter(|task| task.prerequisite_task_ids.is_empty()).map(|task| task.id.clone()).collect::<Vec<_>>(),
            "started_runs": [],
        }),
    )?);
    audit.succeeded();
    Ok(response)
}

pub(super) async fn retire_chatos_project_execution(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<RetireChatosProjectExecutionRequest>,
) -> Result<Json<Value>, InternalApiError> {
    let identity = require_chatos_execution_mutation(&state, &headers)?;
    let project_id = required_internal_text(request.project_id, "project_id")?;
    let requirement_id = required_internal_text(request.requirement_id, "requirement_id")?;
    let source_session_id = required_internal_text(request.source_session_id, "source_session_id")?;
    let source_user_message_id =
        required_internal_text(request.source_user_message_id, "source_user_message_id")?;
    let mut audit = TaskRunnerInternalAuditGuard::new(
        &identity,
        Some(project_id.as_str()),
        "project_execution",
        requirement_id.as_str(),
        "retire",
    );
    let tasks = state
        .task_service
        .list_tasks_for_chatos_source(
            source_session_id.as_str(),
            Some(source_user_message_id.as_str()),
            None,
        )
        .await
        .map_err(InternalApiError::internal)?;
    enrich_project_execution_audit(&mut audit, tasks.as_slice());
    for task in &tasks {
        validate_project_execution_task(task, project_id.as_str(), requirement_id.as_str())?;
        if state
            .run_service
            .has_active_run_for_task(task.id.as_str())
            .await
            .map_err(InternalApiError::internal)?
        {
            return Err(InternalApiError::conflict(format!(
                "project execution task still has an active run: {}",
                task.id
            )));
        }
    }
    let mut deleted_task_ids = Vec::new();
    for task in &tasks {
        if state
            .task_service
            .delete_task(task.id.as_str())
            .await
            .map_err(InternalApiError::internal)?
        {
            deleted_task_ids.push(task.id.clone());
        }
    }
    let response = Json(json!({
        "success": true,
        "project_id": project_id,
        "requirement_id": requirement_id,
        "source_session_id": source_session_id,
        "source_user_message_id": source_user_message_id,
        "deleted_task_ids": deleted_task_ids,
    }));
    audit.succeeded();
    Ok(response)
}

fn enrich_project_execution_audit(
    audit: &mut TaskRunnerInternalAuditGuard,
    tasks: &[crate::models::TaskRecord],
) {
    let represented_user_id = tasks.iter().find_map(|task| {
        task.owner_user_id
            .as_deref()
            .or(task.creator_user_id.as_deref())
    });
    audit.represented_user_id(represented_user_id);
    audit.tenant_id(tasks.first().map(|task| task.tenant_id.as_str()));
}

pub(super) fn required_internal_text(
    value: String,
    field: &str,
) -> Result<String, InternalApiError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(InternalApiError::bad_request(format!(
            "{field} is required"
        )));
    }
    Ok(value.to_string())
}

pub(super) fn require_chatos_execution_mutation(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<TaskRunnerInternalRequestIdentity, InternalApiError> {
    require_task_runner_internal_request(
        &state.config,
        headers,
        &[CHATOS_CALLER],
        CHATOS_EXECUTION_START_SCOPE,
    )
    .map_err(|error| InternalApiError {
        status: error.status,
        message: error.message,
    })
}

fn validate_project_execution_task(
    task: &crate::models::TaskRecord,
    project_id: &str,
    requirement_id: &str,
) -> Result<(), InternalApiError> {
    let payload = task.input_payload.as_ref();
    let payload_source = payload
        .and_then(|value| value.get("source"))
        .and_then(Value::as_str);
    let payload_requirement_id = payload
        .and_then(|value| value.get("root_requirement_id"))
        .or_else(|| payload.and_then(|value| value.get("requirement_id")))
        .and_then(Value::as_str);
    if task.project_id.as_deref() != Some(project_id)
        || payload_source != Some("chatos_project_requirement_execution")
        || payload_requirement_id != Some(requirement_id)
    {
        return Err(InternalApiError::conflict(
            "task graph does not belong to the requested project requirement execution",
        ));
    }
    Ok(())
}
