// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod response;

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chatos_mcp::{AskUserPromptPayload, AskUserResponseSubmission};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::local_runtime::storage::LocalAskUserPromptRecord;
use crate::LocalRuntime;

use super::context::owner_context;
use super::error::LocalRuntimeApiError;
use response::prompt_response;

#[derive(Debug, Default, Deserialize)]
struct PromptListQuery {
    include_pending: Option<bool>,
    limit: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
struct PromptMutationPayload {
    conversation_id: Option<String>,
    #[serde(rename = "conversationId")]
    conversation_id_camel: Option<String>,
    values: Option<Value>,
    selection: Option<Value>,
    reason: Option<String>,
}

pub(super) fn router() -> Router<LocalRuntime> {
    Router::new()
        .route(
            "/api/local/runtime/sessions/{session_id}/ask-user-prompts",
            get(list_prompts),
        )
        .route(
            "/api/local/runtime/sessions/{session_id}/ask-user-prompts/{prompt_id}/submit",
            post(submit_prompt),
        )
        .route(
            "/api/local/runtime/sessions/{session_id}/ask-user-prompts/{prompt_id}/cancel",
            post(cancel_prompt),
        )
}

async fn list_prompts(
    Path(session_id): Path<String>,
    Query(query): Query<PromptListQuery>,
    State(runtime): State<LocalRuntime>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    ensure_session(&runtime, owner.owner_user_id.as_str(), session_id.as_str()).await?;
    let prompts = runtime
        .local_database()?
        .list_ask_user_prompts(
            owner.owner_user_id.as_str(),
            session_id.as_str(),
            query.include_pending.unwrap_or(true),
            query.limit.unwrap_or(100),
        )
        .await?;
    Ok(Json(json!({
        "success": true,
        "conversation_id": session_id,
        "count": prompts.len(),
        "prompts": prompts.iter().map(prompt_response).collect::<Vec<_>>(),
    })))
}

async fn submit_prompt(
    Path((session_id, prompt_id)): Path<(String, String)>,
    State(runtime): State<LocalRuntime>,
    Json(payload): Json<PromptMutationPayload>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    resolve_prompt(&runtime, &session_id, &prompt_id, payload, "ok").await
}

async fn cancel_prompt(
    Path((session_id, prompt_id)): Path<(String, String)>,
    State(runtime): State<LocalRuntime>,
    Json(payload): Json<PromptMutationPayload>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    resolve_prompt(&runtime, &session_id, &prompt_id, payload, "canceled").await
}

async fn resolve_prompt(
    runtime: &LocalRuntime,
    session_id: &str,
    prompt_id: &str,
    payload: PromptMutationPayload,
    status: &str,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    validate_conversation_id(session_id, &payload)?;
    let owner = owner_context(runtime).await?;
    let current = runtime
        .local_database()?
        .get_ask_user_prompt(owner.owner_user_id.as_str(), prompt_id)
        .await?
        .filter(|record| record.session_id == session_id)
        .ok_or_else(|| {
            LocalRuntimeApiError::not_found(
                "local_ask_user_prompt_not_found",
                "Local Ask User prompt was not found",
            )
        })?;
    if current.status != "pending" {
        return Err(LocalRuntimeApiError::conflict(
            "local_ask_user_prompt_resolved",
            "Local Ask User prompt has already been resolved",
        ));
    }
    if status == "canceled" && !prompt_allows_cancel(&current)? {
        return Err(LocalRuntimeApiError::bad_request(
            "local_ask_user_cancel_not_allowed",
            "This Local Ask User prompt cannot be canceled",
        ));
    }
    let response = AskUserResponseSubmission {
        status: status.to_string(),
        values: payload.values,
        selection: payload.selection,
        reason: payload.reason,
    };
    let response_json = serde_json::to_string(&response)
        .map_err(|error| LocalRuntimeApiError::internal(error.to_string()))?;
    let resolved = runtime
        .local_database()?
        .resolve_ask_user_prompt(
            owner.owner_user_id.as_str(),
            prompt_id,
            status,
            response_json.as_str(),
        )
        .await?
        .ok_or_else(|| {
            LocalRuntimeApiError::conflict(
                "local_ask_user_prompt_resolved",
                "Local Ask User prompt has already been resolved",
            )
        })?;
    runtime.ask_user_prompts.notify(prompt_id).await;
    Ok(Json(json!({
        "success": true,
        "prompt": prompt_response(&resolved),
    })))
}

async fn ensure_session(
    runtime: &LocalRuntime,
    owner_user_id: &str,
    session_id: &str,
) -> Result<(), LocalRuntimeApiError> {
    if runtime
        .local_database()?
        .get_session(session_id, owner_user_id)
        .await?
        .is_none()
    {
        return Err(LocalRuntimeApiError::not_found(
            "local_session_not_found",
            "Local runtime session was not found",
        ));
    }
    Ok(())
}

fn validate_conversation_id(
    session_id: &str,
    payload: &PromptMutationPayload,
) -> Result<(), LocalRuntimeApiError> {
    let supplied = payload
        .conversation_id
        .as_deref()
        .or(payload.conversation_id_camel.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if supplied.is_some_and(|value| value != session_id) {
        return Err(LocalRuntimeApiError::bad_request(
            "local_ask_user_session_mismatch",
            "Ask User conversation ID does not match the local session",
        ));
    }
    Ok(())
}

fn prompt_allows_cancel(record: &LocalAskUserPromptRecord) -> Result<bool, LocalRuntimeApiError> {
    serde_json::from_str::<AskUserPromptPayload>(record.prompt_json.as_str())
        .map(|prompt| prompt.allow_cancel)
        .map_err(|error| LocalRuntimeApiError::internal(error.to_string()))
}
