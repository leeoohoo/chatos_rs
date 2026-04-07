use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde_json::{json, Value};

use crate::models::{CreateConversationRunRequest, UpdateConversationRunRequest};
use crate::repositories::{conversations, runs};

use super::event_publish::publish_conversation_scoped_event;
use super::shared::{ensure_conversation_access, require_auth};
use super::SharedState;

pub(super) async fn list_runs(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(conversation_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if let Err(err) =
        ensure_conversation_access(state.as_ref(), &auth, conversation_id.as_str()).await
    {
        return err;
    }

    match runs::list_runs_by_conversation(&state.pool, conversation_id.as_str(), 100).await {
        Ok(items) => (StatusCode::OK, Json(json!(items))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list runs failed", "detail": err})),
        ),
    }
}

pub(super) async fn create_run(
    State(state): State<SharedState>,
    Json(req): Json<CreateConversationRunRequest>,
) -> (StatusCode, Json<Value>) {
    match runs::create_run(&state.pool, req).await {
        Ok(item) => {
            if let Ok(Some(conversation)) =
                conversations::get_conversation_by_id(&state.pool, item.conversation_id.as_str()).await
            {
                publish_conversation_scoped_event(
                    &state,
                    conversation.owner_user_id.as_str(),
                    "im.run.created",
                    conversation.id.as_str(),
                    "run",
                    &item,
                );
            }
            (StatusCode::CREATED, Json(json!(item)))
        }
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "create run failed", "detail": err})),
        ),
    }
}

pub(super) async fn update_run(
    State(state): State<SharedState>,
    Path(run_id): Path<String>,
    Json(req): Json<UpdateConversationRunRequest>,
) -> (StatusCode, Json<Value>) {
    match runs::update_run(&state.pool, run_id.as_str(), req).await {
        Ok(Some(item)) => {
            if let Ok(Some(conversation)) =
                conversations::get_conversation_by_id(&state.pool, item.conversation_id.as_str()).await
            {
                publish_conversation_scoped_event(
                    &state,
                    conversation.owner_user_id.as_str(),
                    "im.run.updated",
                    conversation.id.as_str(),
                    "run",
                    &item,
                );
            }
            (StatusCode::OK, Json(json!(item)))
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "run not found"})),
        ),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "update run failed", "detail": err})),
        ),
    }
}
