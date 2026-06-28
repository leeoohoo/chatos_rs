use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::{Extension, Json};

use super::access::{ensure_project_writable, require_project_access, require_work_item_access};
use super::ApiError;
use crate::auth::{AccessToken, CurrentUser};
use crate::models::{
    CreateTaskRunnerTaskFromWorkItemRequest, CreateTaskRunnerTaskFromWorkItemResponse,
    LinkTaskRunnerTaskRequest, ProjectWorkItemTaskRunnerLinkRecord,
    TaskRunnerExecutionOptionRecord, TaskRunnerExecutionOptionsResponse,
};
use crate::state::AppState;
use crate::task_runner_api_client;

pub(in crate::api) async fn list_task_runner_links(
    Path(work_item_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<Vec<ProjectWorkItemTaskRunnerLinkRecord>>, ApiError> {
    require_work_item_access(&state, &work_item_id, &user).await?;
    state
        .store
        .list_task_runner_links(&work_item_id)
        .await
        .map(Json)
        .map_err(ApiError::bad_request)
}

pub(in crate::api) async fn link_task_runner_task(
    Path(work_item_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(input): Json<LinkTaskRunnerTaskRequest>,
) -> Result<(StatusCode, Json<ProjectWorkItemTaskRunnerLinkRecord>), ApiError> {
    let item = require_work_item_access(&state, &work_item_id, &user).await?;
    let project = require_project_access(&state, &item.project_id, &user).await?;
    ensure_project_writable(&project)?;
    let link = state
        .store
        .upsert_task_runner_link(&work_item_id, input)
        .await
        .map_err(ApiError::bad_request)?;
    Ok((StatusCode::CREATED, Json(link)))
}

pub(in crate::api) async fn delete_task_runner_link(
    Path((work_item_id, link_id)): Path<(String, String)>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<StatusCode, ApiError> {
    let item = require_work_item_access(&state, &work_item_id, &user).await?;
    let project = require_project_access(&state, &item.project_id, &user).await?;
    ensure_project_writable(&project)?;
    let deleted = state
        .store
        .delete_task_runner_link(&work_item_id, &link_id)
        .await
        .map_err(ApiError::bad_request)?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found(format!(
            "TaskRunner 关联不存在: {link_id}"
        )))
    }
}

pub(in crate::api) async fn create_task_runner_task_from_work_item(
    Path(work_item_id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Extension(access_token): Extension<AccessToken>,
    Json(mut input): Json<CreateTaskRunnerTaskFromWorkItemRequest>,
) -> Result<(StatusCode, Json<CreateTaskRunnerTaskFromWorkItemResponse>), ApiError> {
    let item = require_work_item_access(&state, &work_item_id, &user).await?;
    let project = require_project_access(&state, &item.project_id, &user).await?;
    ensure_project_writable(&project)?;
    if input.prerequisite_task_ids.is_none() {
        input.prerequisite_task_ids = Some(
            derive_task_runner_prerequisite_task_ids(&state, &work_item_id)
                .await
                .map_err(ApiError::bad_request)?,
        );
    }
    let source_session_id = input.source_session_id.clone();
    let source_user_message_id = input.source_user_message_id.clone();
    let task = task_runner_api_client::create_task_from_work_item(
        &state.config,
        access_token.0.as_str(),
        &item,
        input,
    )
    .await
    .map_err(ApiError::bad_gateway)?;
    let link = state
        .store
        .upsert_task_runner_link(
            &work_item_id,
            LinkTaskRunnerTaskRequest {
                task_runner_task_id: task.id.clone(),
                task_runner_run_id: task.last_run_id.clone(),
                link_type: Some("execution".to_string()),
                source_session_id,
                source_user_message_id,
                task_runner_status: Some(task.status.clone()),
                last_callback_event: None,
                last_callback_at: None,
                last_error_message: None,
            },
        )
        .await
        .map_err(ApiError::bad_request)?;
    Ok((
        StatusCode::CREATED,
        Json(CreateTaskRunnerTaskFromWorkItemResponse { task, link }),
    ))
}

pub(in crate::api) async fn get_task_runner_execution_options(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<TaskRunnerExecutionOptionsResponse>, ApiError> {
    let owner_user_id = user
        .effective_owner_user_id()
        .ok_or_else(|| ApiError::unauthorized("当前登录态缺少用户归属信息"))?;
    let options = task_runner_api_client::fetch_execution_options(&state.config, owner_user_id)
        .await
        .map_err(ApiError::bad_gateway)?;
    Ok(Json(TaskRunnerExecutionOptionsResponse {
        model_configs: options
            .model_config_ids()
            .into_iter()
            .map(execution_option_record)
            .collect(),
        tools: options
            .tool_ids()
            .into_iter()
            .map(execution_option_record)
            .collect(),
    }))
}

fn execution_option_record(id: String) -> TaskRunnerExecutionOptionRecord {
    TaskRunnerExecutionOptionRecord {
        label: id.clone(),
        id,
    }
}

async fn derive_task_runner_prerequisite_task_ids(
    state: &AppState,
    work_item_id: &str,
) -> Result<Vec<String>, String> {
    let mut task_ids = Vec::new();
    for dependency in state
        .store
        .list_work_item_dependencies(work_item_id)
        .await?
    {
        for link in state
            .store
            .list_task_runner_links(&dependency.prerequisite_work_item_id)
            .await?
        {
            task_ids.push(link.task_runner_task_id);
        }
    }
    task_ids.sort();
    task_ids.dedup();
    Ok(task_ids)
}
