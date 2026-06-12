use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::{
    services::{
        ChatosMessageRunDetail, ChatosMessageTaskDetail, ChatosMessageTaskRun,
        ChatosMessageTaskRunEvent, ChatosMessageTaskSummary,
    },
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/internal/chatos/message-tasks",
            get(list_chatos_message_tasks),
        )
        .route(
            "/internal/chatos/message-tasks/:task_id",
            get(get_chatos_message_task),
        )
        .route(
            "/internal/chatos/message-runs/:run_id",
            get(get_chatos_message_run),
        )
}

#[derive(Debug, Deserialize)]
struct ChatosMessageTaskQuery {
    source_session_id: String,
    source_user_message_id: Option<String>,
    source_turn_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct ChatosMessageTasksResponse {
    items: Vec<ChatosMessageTaskSummary>,
}

#[derive(Debug)]
struct InternalApiError {
    status: StatusCode,
    message: String,
}

impl InternalApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
        }
    }
}

impl IntoResponse for InternalApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorBody {
                error: self.message,
            }),
        )
            .into_response()
    }
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

fn validate_chatos_message_query(
    query: &ChatosMessageTaskQuery,
) -> Result<(&str, Option<&str>, Option<&str>), InternalApiError> {
    let source_session_id = query.source_session_id.trim();
    let source_user_message_id = query
        .source_user_message_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let source_turn_id = query
        .source_turn_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if source_session_id.is_empty()
        || (source_user_message_id.is_none() && source_turn_id.is_none())
    {
        return Err(InternalApiError::bad_request(
            "source_session_id and source_user_message_id or source_turn_id are required",
        ));
    }
    Ok((source_session_id, source_user_message_id, source_turn_id))
}

async fn list_chatos_message_tasks(
    State(state): State<AppState>,
    Query(query): Query<ChatosMessageTaskQuery>,
) -> Result<Json<ChatosMessageTasksResponse>, InternalApiError> {
    let (source_session_id, source_user_message_id, source_turn_id) =
        validate_chatos_message_query(&query)?;
    let items = state
        .task_service
        .list_message_task_summaries_for_chatos_source(
            source_session_id,
            source_user_message_id,
            source_turn_id,
        )
        .await
        .map_err(InternalApiError::internal)?;
    Ok(Json(ChatosMessageTasksResponse { items }))
}

async fn get_chatos_message_task(
    Path(task_id): Path<String>,
    State(state): State<AppState>,
    Query(query): Query<ChatosMessageTaskQuery>,
) -> Result<Json<ChatosMessageTaskDetail>, InternalApiError> {
    let (source_session_id, source_user_message_id, source_turn_id) =
        validate_chatos_message_query(&query)?;
    state
        .task_service
        .get_message_task_detail_for_chatos_source(
            task_id.trim(),
            source_session_id,
            source_user_message_id,
            source_turn_id,
        )
        .await
        .map_err(InternalApiError::internal)?
        .map(Json)
        .ok_or_else(|| InternalApiError::not_found("task not found for message"))
}

async fn get_chatos_message_run(
    Path(run_id): Path<String>,
    State(state): State<AppState>,
    Query(query): Query<ChatosMessageTaskQuery>,
) -> Result<Json<ChatosMessageRunDetail>, InternalApiError> {
    let (source_session_id, source_user_message_id, source_turn_id) =
        validate_chatos_message_query(&query)?;
    let run = state
        .run_service
        .get_run(run_id.trim())
        .await
        .map_err(InternalApiError::internal)?
        .ok_or_else(|| InternalApiError::not_found("run not found for message"))?;
    let task = state
        .task_service
        .get_message_task_detail_for_chatos_source(
            run.task_id.as_str(),
            source_session_id,
            source_user_message_id,
            source_turn_id,
        )
        .await
        .map_err(InternalApiError::internal)?
        .ok_or_else(|| InternalApiError::not_found("run not found for message"))?;
    let events = state
        .run_service
        .list_run_events(run.id.as_str())
        .await
        .map_err(InternalApiError::internal)?;
    Ok(Json(ChatosMessageRunDetail {
        task,
        run: ChatosMessageTaskRun::from(run),
        events: events
            .into_iter()
            .map(ChatosMessageTaskRunEvent::from)
            .collect(),
    }))
}
