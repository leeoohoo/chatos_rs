// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

use crate::core::auth::AuthUser;
use crate::core::session_access::{ensure_owned_session, SessionAccessError};
use crate::models::memory_compat::MemoryCompatComposeContextResponse;
use crate::models::memory_runtime_types::{
    DeleteSummaryResultDto, SyncTurnRuntimeSnapshotRequestDto, TurnRuntimeSnapshotDto,
    TurnRuntimeSnapshotLookupResponseDto,
};
use crate::models::message::Message;
use crate::models::session::Session;
use crate::models::session_summary_v2::SessionSummaryV2;

use super::context_history;
use super::messages::{self};
use super::sessions;
use super::summaries;

pub use super::messages::CompatMessageInput;
pub use super::sessions::{
    map_compat_sync_session_error, CompatCreateSessionError, CompatCreateSessionInput,
    CompatSyncSessionError,
};

#[derive(Debug)]
pub enum CompatScopedOperationError {
    SessionAccess(SessionAccessError),
    Internal(String),
}

#[derive(Debug)]
pub enum CompatMessageOperationError {
    NotFound,
    SessionAccess(SessionAccessError),
    Internal(String),
}

#[derive(Debug, Clone)]
pub struct SyncConversationSessionCompatRequest {
    pub session_id: String,
    pub scope_user_id: String,
    pub project_id: Option<String>,
    pub title: Option<String>,
    pub metadata: Option<Value>,
    pub status: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

pub async fn list_sessions(
    user_id: &str,
    project_id: Option<&str>,
    limit: Option<i64>,
    offset: i64,
    status: Option<&str>,
) -> Result<Vec<Session>, String> {
    sessions::list_sessions(
        user_id,
        project_id,
        limit,
        offset,
        matches!(status, Some("archived")),
        false,
    )
    .await
}

pub async fn create_session(
    input: CompatCreateSessionInput,
) -> Result<Session, CompatCreateSessionError> {
    sessions::create_session_compat(input).await
}

pub async fn get_session_for_auth(
    auth: &AuthUser,
    session_id: &str,
) -> Result<Session, SessionAccessError> {
    ensure_owned_session(session_id, auth).await
}

pub async fn sync_session_for_auth(
    auth: &AuthUser,
    input: SyncConversationSessionCompatRequest,
) -> Result<Session, sessions::CompatSyncSessionError> {
    let SyncConversationSessionCompatRequest {
        session_id,
        scope_user_id,
        project_id,
        title,
        metadata,
        status,
        created_at,
        updated_at,
    } = input;

    let _ignored_created_at = created_at;
    let _ignored_updated_at = updated_at;

    sessions::sync_session_compat_for_auth(
        auth,
        sessions::SyncConversationSessionCompatInput {
            session_id,
            scope_user_id,
            project_id,
            title,
            metadata,
            status,
            existing_session: None,
        },
    )
    .await
}

pub async fn update_session_for_auth(
    auth: &AuthUser,
    session_id: &str,
    title: Option<String>,
    status: Option<String>,
    metadata: Option<Value>,
) -> Result<Option<Session>, CompatScopedOperationError> {
    require_owned_session(auth, session_id).await?;
    sessions::update_session_compat(session_id, title, status, metadata)
        .await
        .map_err(CompatScopedOperationError::Internal)
}

pub async fn delete_session_for_auth(
    auth: &AuthUser,
    session_id: &str,
) -> Result<bool, CompatScopedOperationError> {
    require_owned_session(auth, session_id).await?;
    sessions::archive_session(auth.user_id.as_str(), session_id)
        .await
        .map_err(CompatScopedOperationError::Internal)
}

pub async fn list_messages_for_auth(
    auth: &AuthUser,
    session_id: &str,
    limit: Option<i64>,
    offset: i64,
    asc: bool,
) -> Result<Vec<Message>, CompatScopedOperationError> {
    require_owned_session(auth, session_id).await?;
    messages::list_messages(session_id, limit, offset, asc)
        .await
        .map_err(CompatScopedOperationError::Internal)
}

pub async fn create_message_for_auth(
    auth: &AuthUser,
    session_id: &str,
    input: CompatMessageInput,
) -> Result<Message, CompatScopedOperationError> {
    require_owned_session(auth, session_id).await?;
    messages::upsert_compat_message(session_id, input, None)
        .await
        .map_err(CompatScopedOperationError::Internal)
}

pub async fn sync_message_for_auth(
    auth: &AuthUser,
    session_id: &str,
    message_id: String,
    created_at: Option<String>,
    input: CompatMessageInput,
) -> Result<Message, CompatScopedOperationError> {
    require_owned_session(auth, session_id).await?;
    messages::upsert_compat_message(session_id, input, Some((message_id, created_at)))
        .await
        .map_err(CompatScopedOperationError::Internal)
}

pub async fn batch_create_messages_for_auth(
    auth: &AuthUser,
    session_id: &str,
    inputs: Vec<CompatMessageInput>,
) -> Result<Vec<Message>, CompatScopedOperationError> {
    require_owned_session(auth, session_id).await?;
    messages::batch_upsert_compat_messages(session_id, inputs)
        .await
        .map_err(CompatScopedOperationError::Internal)
}

pub async fn clear_session_messages_for_auth(
    auth: &AuthUser,
    session_id: &str,
) -> Result<i64, CompatScopedOperationError> {
    require_owned_session(auth, session_id).await?;
    messages::delete_messages_by_session(session_id)
        .await
        .map_err(CompatScopedOperationError::Internal)
}

pub async fn get_message_for_auth(
    auth: &AuthUser,
    message_id: &str,
) -> Result<Message, CompatMessageOperationError> {
    let message = load_owned_message(auth, message_id).await?;
    Ok(message)
}

pub async fn delete_message_for_auth(
    auth: &AuthUser,
    message_id: &str,
) -> Result<bool, CompatMessageOperationError> {
    let message = load_owned_message(auth, message_id).await?;
    messages::delete_message_by_id(message.id.as_str())
        .await
        .map_err(CompatMessageOperationError::Internal)
}

pub async fn list_summaries_for_auth(
    auth: &AuthUser,
    session_id: &str,
    limit: Option<i64>,
    offset: i64,
) -> Result<Vec<SessionSummaryV2>, CompatScopedOperationError> {
    require_owned_session(auth, session_id).await?;
    summaries::list_summaries(session_id, limit, offset)
        .await
        .map_err(CompatScopedOperationError::Internal)
}

pub async fn delete_summary_for_auth(
    auth: &AuthUser,
    session_id: &str,
    summary_id: &str,
) -> Result<DeleteSummaryResultDto, CompatScopedOperationError> {
    require_owned_session(auth, session_id).await?;
    summaries::delete_summary(session_id, summary_id)
        .await
        .map_err(CompatScopedOperationError::Internal)
}

pub async fn sync_turn_runtime_snapshot_for_auth(
    auth: &AuthUser,
    session_id: &str,
    turn_id: &str,
    payload: &SyncTurnRuntimeSnapshotRequestDto,
) -> Result<TurnRuntimeSnapshotDto, CompatScopedOperationError> {
    require_owned_session(auth, session_id).await?;
    messages::sync_turn_runtime_snapshot(session_id, turn_id, payload)
        .await
        .map_err(CompatScopedOperationError::Internal)
}

pub async fn get_latest_turn_runtime_snapshot_for_auth(
    auth: &AuthUser,
    session_id: &str,
) -> Result<TurnRuntimeSnapshotLookupResponseDto, CompatScopedOperationError> {
    require_owned_session(auth, session_id).await?;
    messages::get_latest_turn_runtime_snapshot(session_id)
        .await
        .map_err(CompatScopedOperationError::Internal)
}

pub async fn get_turn_runtime_snapshot_by_turn_for_auth(
    auth: &AuthUser,
    session_id: &str,
    turn_id: &str,
) -> Result<TurnRuntimeSnapshotLookupResponseDto, CompatScopedOperationError> {
    require_owned_session(auth, session_id).await?;
    messages::get_turn_runtime_snapshot_by_turn(session_id, turn_id)
        .await
        .map_err(CompatScopedOperationError::Internal)
}

pub async fn compose_context_for_auth(
    auth: &AuthUser,
    session_id: &str,
    include_raw_messages: Option<bool>,
) -> Result<MemoryCompatComposeContextResponse, CompatScopedOperationError> {
    let session = require_owned_session(auth, session_id).await?;
    context_history::compose_context_compat_response(
        &session,
        context_history::compat_include_raw_messages(include_raw_messages),
    )
    .await
    .map_err(CompatScopedOperationError::Internal)
}

async fn require_owned_session(
    auth: &AuthUser,
    session_id: &str,
) -> Result<Session, CompatScopedOperationError> {
    ensure_owned_session(session_id, auth)
        .await
        .map_err(CompatScopedOperationError::SessionAccess)
}

async fn load_owned_message(
    auth: &AuthUser,
    message_id: &str,
) -> Result<Message, CompatMessageOperationError> {
    let message = messages::get_message_by_id(message_id)
        .await
        .map_err(CompatMessageOperationError::Internal)?
        .ok_or(CompatMessageOperationError::NotFound)?;
    ensure_owned_session(message.session_id.as_str(), auth)
        .await
        .map_err(CompatMessageOperationError::SessionAccess)?;
    Ok(message)
}
