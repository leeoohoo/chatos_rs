// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::{
    extract::{Path, Query},
    http::StatusCode,
    Json,
};
use serde_json::Value;

use crate::core::auth::AuthUser;
use crate::core::pagination::{parse_non_negative_offset, parse_positive_limit};
use crate::core::session_access::{ensure_owned_session, map_session_access_error};
use crate::core::user_scope::resolve_user_id;
use crate::core::validation::normalize_non_empty;
use crate::modules::conversation_runtime::sessions::{
    archive_session, create_session as create_conversation_session,
    list_sessions as list_conversation_sessions, update_session as update_conversation_session,
    CreateConversationSessionInput,
};
use crate::services::chatos_sessions;

use super::contracts::{CreateSessionRequest, SessionQuery, UpdateSessionRequest};

pub(super) async fn list_sessions(
    auth: AuthUser,
    Query(query): Query<SessionQuery>,
) -> (StatusCode, Json<Value>) {
    let SessionQuery {
        user_id,
        project_id,
        limit,
        offset,
        include_archived,
        include_archiving,
    } = query;
    let user_id = match resolve_user_id(user_id, &auth) {
        Ok(user_id) => user_id,
        Err(err) => return err,
    };
    let limit = parse_positive_limit(limit);
    let offset = parse_non_negative_offset(offset);
    let include_archived = include_archived
        .as_deref()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes"
            )
        })
        .unwrap_or(false);
    let include_archiving = include_archiving
        .as_deref()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes"
            )
        })
        .unwrap_or(false);

    let result = list_conversation_sessions(
        user_id.as_str(),
        project_id.as_deref(),
        limit,
        offset,
        include_archived,
        include_archiving,
    )
    .await;
    match result {
        Ok(list) => (
            StatusCode::OK,
            Json(serde_json::to_value(list).unwrap_or(Value::Null)),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": err})),
        ),
    }
}

pub(super) async fn create_session(
    auth: AuthUser,
    Json(req): Json<CreateSessionRequest>,
) -> (StatusCode, Json<Value>) {
    let CreateSessionRequest {
        title,
        description,
        metadata,
        user_id,
        project_id,
    } = req;
    let user_id = match resolve_user_id(user_id, &auth) {
        Ok(user_id) => user_id,
        Err(err) => return err,
    };

    let Some(title) = normalize_non_empty(title) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "对话线程标题不能为空"})),
        );
    };

    let _ = description;
    match create_conversation_session(CreateConversationSessionInput {
        actor_user_id: auth.user_id.clone(),
        user_id,
        title,
        project_id,
        metadata,
    })
    .await
    {
        Ok(saved) => (
            StatusCode::CREATED,
            Json(serde_json::to_value(saved).unwrap_or(Value::Null)),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": err})),
        ),
    }
}

pub(super) async fn get_session(
    auth: AuthUser,
    Path(id): Path<String>,
) -> (StatusCode, Json<Value>) {
    match ensure_owned_session(&id, &auth).await {
        Ok(session) => (
            StatusCode::OK,
            Json(serde_json::to_value(session).unwrap_or(Value::Null)),
        ),
        Err(err) => map_session_access_error(err),
    }
}

pub(super) async fn update_session(
    auth: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateSessionRequest>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_owned_session(&id, &auth).await {
        return map_session_access_error(err);
    }

    let _ = req.description;
    match update_conversation_session(
        auth.user_id.as_str(),
        &id,
        req.title.clone(),
        req.metadata.clone(),
    )
    .await
    {
        Ok(Some(session)) => (
            StatusCode::OK,
            Json(serde_json::to_value(session).unwrap_or(Value::Null)),
        ),
        Ok(None) => (StatusCode::OK, Json(Value::Null)),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": err})),
        ),
    }
}

pub(super) async fn delete_session(
    auth: AuthUser,
    Path(id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let session = match ensure_owned_session(&id, &auth).await {
        Ok(session) => session,
        Err(err) => return map_session_access_error(err),
    };

    match archive_session(auth.user_id.as_str(), &id).await {
        Ok(true) => {
            let _archived_session = chatos_sessions::get_session_by_id(&id)
                .await
                .ok()
                .flatten()
                .unwrap_or(session);
            (
                StatusCode::OK,
                Json(serde_json::json!({"success": true, "message": "对话线程已归档"})),
            )
        }
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "对话线程不存在"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": err})),
        ),
    }
}
