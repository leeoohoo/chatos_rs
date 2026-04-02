use crate::services::task_manager::normalizer::trimmed_non_empty;
use crate::services::task_manager::types::{TaskRecord, TaskUpdatePatch, TASK_NOT_FOUND_ERR};
use crate::services::task_service_client::{self, UpdateTaskRequestDto};

use super::remote_support::{
    map_legacy_status_to_remote, map_remote_task_to_record, resolve_task_scope_context,
};

async fn load_session_scoped_remote_task(
    session_id: &str,
    task_id: &str,
) -> Result<task_service_client::TaskRecordDto, String> {
    let session_id =
        trimmed_non_empty(session_id).ok_or_else(|| "session_id is required".to_string())?;
    let task_id = trimmed_non_empty(task_id).ok_or_else(|| "task_id is required".to_string())?;
    let scope = resolve_task_scope_context(session_id).await?;
    let task = task_service_client::get_task(task_id)
        .await?
        .ok_or_else(|| TASK_NOT_FOUND_ERR.to_string())?;
    let same_scope = task.user_id == scope.user_id
        && task.contact_agent_id == scope.contact_agent_id
        && task.project_id == scope.project_id
        && task
            .session_id
            .as_deref()
            .map(str::trim)
            .unwrap_or_default()
            == session_id;
    if !same_scope {
        return Err(TASK_NOT_FOUND_ERR.to_string());
    }
    Ok(task)
}

pub async fn update_task_by_id(
    session_id: &str,
    task_id: &str,
    patch: TaskUpdatePatch,
) -> Result<TaskRecord, String> {
    let _existing = load_session_scoped_remote_task(session_id, task_id).await?;
    let patch = patch.normalized()?;
    if patch.is_empty() {
        return Err("at least one task field is required".to_string());
    }

    let updated = task_service_client::update_task(
        task_id,
        &UpdateTaskRequestDto {
            title: patch.title,
            content: patch.details,
            priority: patch.priority,
            status: map_legacy_status_to_remote(patch.status),
            confirm_note: None,
            execution_note: None,
            model_config_id: None,
            result_summary: None,
            result_message_id: None,
            last_error: None,
        },
    )
    .await?
    .ok_or_else(|| TASK_NOT_FOUND_ERR.to_string())?;

    Ok(map_remote_task_to_record(updated))
}

pub async fn complete_task_by_id(session_id: &str, task_id: &str) -> Result<TaskRecord, String> {
    let _existing = load_session_scoped_remote_task(session_id, task_id).await?;
    let updated = task_service_client::update_task(
        task_id,
        &UpdateTaskRequestDto {
            status: Some("completed".to_string()),
            ..UpdateTaskRequestDto::default()
        },
    )
    .await?
    .ok_or_else(|| TASK_NOT_FOUND_ERR.to_string())?;
    Ok(map_remote_task_to_record(updated))
}

pub async fn delete_task_by_id(session_id: &str, task_id: &str) -> Result<bool, String> {
    let _existing = load_session_scoped_remote_task(session_id, task_id).await?;
    task_service_client::delete_task(task_id).await
}
