// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(super) async fn retry_chatos_message_run(
    Path(run_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<RetryChatosMessageRunRequest>,
) -> Result<(StatusCode, Json<Value>), InternalApiError> {
    let identity = require_chatos_execution_mutation(&state, &headers)?;
    let run_id = required_internal_text(run_id, "run_id")?;
    let mut audit =
        TaskRunnerInternalAuditGuard::new(&identity, None, "task_run", run_id.as_str(), "retry");
    let (source_session_id, source_user_message_id, source_turn_id) =
        validate_chatos_message_query(&request.source)?;
    let retry_instruction = request
        .retry_instruction
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let execution_service_id = request
        .execution_service_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if retry_instruction.is_some_and(|value| value.chars().count() > 4000) {
        return Err(InternalApiError::bad_request(
            "retry_instruction must not exceed 4000 characters",
        ));
    }
    if execution_service_id.is_some_and(|value| value.chars().count() > 255) {
        return Err(InternalApiError::bad_request(
            "execution_service_id must not exceed 255 characters",
        ));
    }
    let run = require_chatos_message_run(
        &state,
        run_id.as_str(),
        source_session_id,
        source_user_message_id,
        source_turn_id,
    )
    .await?;
    if let Ok(Some(task)) = state.task_service.get_task(run.task_id.as_str()).await {
        audit.represented_user_id(
            task.owner_user_id
                .as_deref()
                .or(task.creator_user_id.as_deref()),
        );
        audit.tenant_id(Some(task.tenant_id.as_str()));
        audit.project_id(task.project_id.as_deref());
        audit.resource_name(Some(task.title.as_str()));
    }
    require_retryable_message_run(&run.status)?;
    let retried = state
        .run_service
        .retry_run_with_instruction_and_execution_service(
            run.id.as_str(),
            retry_instruction.map(ToOwned::to_owned),
            execution_service_id.map(ToOwned::to_owned),
        )
        .await
        .map_err(InternalApiError::bad_request)?
        .ok_or_else(|| InternalApiError::not_found("run not found for message"))?;
    let response = (
        StatusCode::CREATED,
        Json(json!({
            "success": true,
            "run": ChatosMessageTaskRun::from(retried),
        })),
    );
    audit.succeeded();
    Ok(response)
}

pub(super) async fn retry_chatos_message_run_integration(
    Path(run_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<RetryChatosMessageRunIntegrationRequest>,
) -> Result<Json<Value>, InternalApiError> {
    let identity = require_chatos_execution_mutation(&state, &headers)?;
    let run_id = required_internal_text(run_id, "run_id")?;
    let mut audit = TaskRunnerInternalAuditGuard::new(
        &identity,
        None,
        "task_run_integration",
        run_id.as_str(),
        "retry",
    );
    let (source_session_id, source_user_message_id, source_turn_id) =
        validate_chatos_message_query(&request.source)?;
    let run = require_chatos_message_run(
        &state,
        run_id.as_str(),
        source_session_id,
        source_user_message_id,
        source_turn_id,
    )
    .await?;
    if let Ok(Some(task)) = state.task_service.get_task(run.task_id.as_str()).await {
        audit.represented_user_id(
            task.owner_user_id
                .as_deref()
                .or(task.creator_user_id.as_deref()),
        );
        audit.tenant_id(Some(task.tenant_id.as_str()));
        audit.project_id(task.project_id.as_deref());
        audit.resource_name(Some(task.title.as_str()));
    }
    let retried = state
        .run_service
        .retry_run_workspace_integration(run.id.as_str())
        .await
        .map_err(InternalApiError::bad_request)?
        .ok_or_else(|| {
            InternalApiError::conflict("run does not have a retryable code integration conflict")
        })?;
    audit.succeeded();
    Ok(Json(json!({
        "success": true,
        "run": ChatosMessageTaskRun::from(retried),
    })))
}

pub(super) async fn waive_chatos_message_run_integration(
    Path(run_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<WaiveChatosMessageRunIntegrationRequest>,
) -> Result<Json<Value>, InternalApiError> {
    let identity = require_chatos_execution_mutation(&state, &headers)?;
    let run_id = required_internal_text(run_id, "run_id")?;
    let mut audit = TaskRunnerInternalAuditGuard::new(
        &identity,
        None,
        "task_run_integration",
        run_id.as_str(),
        "waive",
    );
    let (source_session_id, source_user_message_id, source_turn_id) =
        validate_chatos_message_query(&request.source)?;
    let run = require_chatos_message_run(
        &state,
        run_id.as_str(),
        source_session_id,
        source_user_message_id,
        source_turn_id,
    )
    .await?;
    if let Ok(Some(task)) = state.task_service.get_task(run.task_id.as_str()).await {
        audit.represented_user_id(
            task.owner_user_id
                .as_deref()
                .or(task.creator_user_id.as_deref()),
        );
        audit.tenant_id(Some(task.tenant_id.as_str()));
        audit.project_id(task.project_id.as_deref());
        audit.resource_name(Some(task.title.as_str()));
    }
    let waived = state
        .run_service
        .waive_run_workspace_integration(run.id.as_str(), request.reason.as_str())
        .await
        .map_err(InternalApiError::bad_request)?
        .ok_or_else(|| {
            InternalApiError::conflict("run does not have a waivable code integration conflict")
        })?;
    audit.succeeded();
    Ok(Json(json!({
        "success": true,
        "run": ChatosMessageTaskRun::from(waived),
    })))
}
