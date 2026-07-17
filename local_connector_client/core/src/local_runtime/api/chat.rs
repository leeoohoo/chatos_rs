// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::State;
use axum::Json;
use serde::Serialize;

use crate::local_runtime::chat::{
    execute_chat_turn, LocalChatExecutionError, LocalChatExecutionErrorKind, LocalChatSendRequest,
};
use crate::LocalRuntime;

use super::context::owner_context;
use super::error::LocalRuntimeApiError;
use super::messages::{message_response, LocalMessageResponse};

#[derive(Debug, Serialize)]
pub(super) struct LocalChatCommandResponse {
    accepted: bool,
    conversation_id: String,
    turn_id: String,
    user_message_id: String,
    source_user_message_id: String,
    user_message: LocalMessageResponse,
    process_messages: Vec<LocalMessageResponse>,
    assistant_message: LocalMessageResponse,
    reused: bool,
}

pub(super) async fn send_chat(
    State(runtime): State<LocalRuntime>,
    Json(request): Json<LocalChatSendRequest>,
) -> Result<Json<LocalChatCommandResponse>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let result = execute_chat_turn(&runtime, owner.owner_user_id.as_str(), request)
        .await
        .map_err(api_error)?;
    let assistant_message = result.snapshot.assistant_message.ok_or_else(|| {
        LocalRuntimeApiError::internal("Local chat completed without an assistant message")
    })?;
    let conversation_id = result.snapshot.turn.session_id.clone();
    let turn_id = result.snapshot.turn.id.clone();
    let user_message_id = result.snapshot.user_message.id.clone();
    Ok(Json(LocalChatCommandResponse {
        accepted: true,
        conversation_id,
        turn_id,
        user_message_id: user_message_id.clone(),
        source_user_message_id: user_message_id,
        user_message: message_response(result.snapshot.user_message),
        process_messages: result
            .process_messages
            .into_iter()
            .map(message_response)
            .collect(),
        assistant_message: message_response(assistant_message),
        reused: result.reused,
    }))
}

fn api_error(error: LocalChatExecutionError) -> LocalRuntimeApiError {
    match error.kind {
        LocalChatExecutionErrorKind::BadRequest => {
            LocalRuntimeApiError::bad_request(error.code, error.message)
        }
        LocalChatExecutionErrorKind::NotFound => {
            LocalRuntimeApiError::not_found(error.code, error.message)
        }
        LocalChatExecutionErrorKind::Conflict => {
            LocalRuntimeApiError::conflict(error.code, error.message)
        }
        LocalChatExecutionErrorKind::Cancelled => {
            LocalRuntimeApiError::conflict(error.code, error.message)
        }
        LocalChatExecutionErrorKind::Model => {
            LocalRuntimeApiError::bad_gateway(error.code, error.message)
        }
        LocalChatExecutionErrorKind::Internal => LocalRuntimeApiError::internal(error.message),
    }
}
