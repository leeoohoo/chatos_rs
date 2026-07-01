// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::StatusCode;
use axum::{
    extract::{Path, Query},
    routing::{delete, get, post, put},
    Json, Router,
};
use serde_json::{json, Value};

use crate::core::auth::AuthUser;
use crate::models::memory_runtime_types::SyncTurnRuntimeSnapshotRequestDto;
use crate::modules::conversation_runtime::memory_compat as compat_runtime;

mod contracts;
mod support;

use self::contracts::{
    CompatBatchCreateMessagesRequest, CompatComposeContextRequest, CompatCreateMessageRequest,
    CompatCreateSessionRequest, CompatListMessagesQuery, CompatListSummariesQuery,
    CompatPatchSessionRequest, CompatSessionQuery, CompatSyncMessageRequest,
    CompatSyncSessionRequest,
};
use self::support::{
    compat_internal_error, compat_message_input_from_create, compat_message_input_from_sync,
    compat_message_result, compat_scoped_result, compat_session_access_error,
    resolve_scope_user_id,
};

pub fn router() -> Router {
    Router::new()
        .route(
            "/api/memory/v1/sessions",
            get(list_sessions).post(create_session),
        )
        .route(
            "/api/memory/v1/sessions/:session_id",
            get(get_session)
                .patch(update_session)
                .delete(delete_session),
        )
        .route(
            "/api/memory/v1/sessions/:session_id/sync",
            put(sync_session),
        )
        .route(
            "/api/memory/v1/sessions/:session_id/messages",
            get(list_messages)
                .post(create_message)
                .delete(clear_session_messages),
        )
        .route(
            "/api/memory/v1/sessions/:session_id/messages/batch",
            post(batch_create_messages),
        )
        .route(
            "/api/memory/v1/sessions/:session_id/messages/:message_id/sync",
            put(sync_message),
        )
        .route(
            "/api/memory/v1/messages/:message_id",
            get(get_message).delete(delete_message),
        )
        .route(
            "/api/memory/v1/sessions/:session_id/summaries",
            get(list_summaries),
        )
        .route(
            "/api/memory/v1/sessions/:session_id/summaries/:summary_id",
            delete(delete_summary),
        )
        .route(
            "/api/memory/v1/sessions/:session_id/turn-runtime-snapshots/:turn_id/sync",
            put(sync_turn_runtime_snapshot),
        )
        .route(
            "/api/memory/v1/sessions/:session_id/turn-runtime-snapshots/latest",
            get(get_latest_turn_runtime_snapshot),
        )
        .route(
            "/api/memory/v1/sessions/:session_id/turn-runtime-snapshots/by-turn/:turn_id",
            get(get_turn_runtime_snapshot_by_turn),
        )
        .route("/api/memory/v1/context/compose", post(compose_context))
}

async fn list_sessions(
    auth: AuthUser,
    Query(query): Query<CompatSessionQuery>,
) -> (StatusCode, Json<Value>) {
    let user_id = match resolve_scope_user_id(query.user_id, &auth) {
        Ok(value) => value,
        Err(err) => return err,
    };
    match compat_runtime::list_sessions(
        user_id.as_str(),
        query.project_id.as_deref(),
        Some(query.limit.unwrap_or(50).max(1).min(500)),
        query.offset.unwrap_or(0).max(0),
        query.status.as_deref(),
    )
    .await
    {
        Ok(items) => (StatusCode::OK, Json(json!({ "items": items }))),
        Err(err) => compat_internal_error("list sessions failed", err),
    }
}

async fn create_session(
    auth: AuthUser,
    Json(req): Json<CompatCreateSessionRequest>,
) -> (StatusCode, Json<Value>) {
    let user_id = match resolve_scope_user_id(Some(req.user_id), &auth) {
        Ok(value) => value,
        Err(err) => return err,
    };
    match compat_runtime::create_session(compat_runtime::CompatCreateSessionInput {
        actor_user_id: auth.user_id.clone(),
        user_id,
        title: req.title,
        project_id: req.project_id,
        metadata: req.metadata,
    })
    .await
    {
        Ok(session) => (StatusCode::OK, Json(json!(session))),
        Err(compat_runtime::CompatCreateSessionError::EmptyTitle) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "title is required"})),
        ),
        Err(compat_runtime::CompatCreateSessionError::Internal(err)) => {
            compat_internal_error("create session failed", err)
        }
    }
}

async fn sync_session(
    auth: AuthUser,
    Path(session_id): Path<String>,
    Json(req): Json<CompatSyncSessionRequest>,
) -> (StatusCode, Json<Value>) {
    let scope_user_id = match resolve_scope_user_id(Some(req.user_id), &auth) {
        Ok(value) => value,
        Err(err) => return err,
    };
    let session = match compat_runtime::sync_session_for_auth(
        &auth,
        compat_runtime::SyncConversationSessionCompatRequest {
            session_id,
            scope_user_id,
            project_id: req.project_id,
            title: req.title,
            metadata: req.metadata,
            status: req.status,
            created_at: req.created_at,
            updated_at: req.updated_at,
        },
    )
    .await
    {
        Ok(session) => session,
        Err(compat_runtime::CompatSyncSessionError::Internal(err)) => {
            return compat_internal_error("sync session failed", err);
        }
        Err(err) => {
            return compat_session_access_error(compat_runtime::map_compat_sync_session_error(err));
        }
    };

    (StatusCode::OK, Json(json!(session)))
}

async fn get_session(auth: AuthUser, Path(session_id): Path<String>) -> (StatusCode, Json<Value>) {
    let session = match compat_runtime::get_session_for_auth(&auth, session_id.as_str()).await {
        Ok(session) => session,
        Err(err) => return compat_session_access_error(err),
    };
    (StatusCode::OK, Json(json!(session)))
}

async fn update_session(
    auth: AuthUser,
    Path(session_id): Path<String>,
    Json(req): Json<CompatPatchSessionRequest>,
) -> (StatusCode, Json<Value>) {
    match compat_scoped_result(
        compat_runtime::update_session_for_auth(
            &auth,
            session_id.as_str(),
            req.title,
            req.status,
            req.metadata,
        )
        .await,
        "update session failed",
    ) {
        Ok(Some(session)) => (StatusCode::OK, Json(json!(session))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "session not found"})),
        ),
        Err(err) => err,
    }
}

async fn delete_session(
    auth: AuthUser,
    Path(session_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    match compat_scoped_result(
        compat_runtime::delete_session_for_auth(&auth, session_id.as_str()).await,
        "delete session failed",
    ) {
        Ok(true) => (StatusCode::OK, Json(json!({"success": true}))),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "session not found"})),
        ),
        Err(err) => err,
    }
}

async fn list_messages(
    auth: AuthUser,
    Path(session_id): Path<String>,
    Query(query): Query<CompatListMessagesQuery>,
) -> (StatusCode, Json<Value>) {
    let asc = !matches!(query.order.as_deref(), Some("desc"));
    match compat_scoped_result(
        compat_runtime::list_messages_for_auth(
            &auth,
            session_id.as_str(),
            Some(query.limit.unwrap_or(100).max(1).min(2000)),
            query.offset.unwrap_or(0).max(0),
            asc,
        )
        .await,
        "list messages failed",
    ) {
        Ok(items) => (StatusCode::OK, Json(json!({ "items": items }))),
        Err(err) => err,
    }
}

async fn create_message(
    auth: AuthUser,
    Path(session_id): Path<String>,
    Json(req): Json<CompatCreateMessageRequest>,
) -> (StatusCode, Json<Value>) {
    match compat_scoped_result(
        compat_runtime::create_message_for_auth(
            &auth,
            session_id.as_str(),
            compat_message_input_from_create(req),
        )
        .await,
        "create message failed",
    ) {
        Ok(saved) => (StatusCode::OK, Json(json!(saved))),
        Err(err) => err,
    }
}

async fn sync_message(
    auth: AuthUser,
    Path((session_id, message_id)): Path<(String, String)>,
    Json(req): Json<CompatSyncMessageRequest>,
) -> (StatusCode, Json<Value>) {
    match compat_scoped_result(
        compat_runtime::sync_message_for_auth(
            &auth,
            session_id.as_str(),
            message_id,
            req.created_at.clone(),
            compat_message_input_from_sync(req),
        )
        .await,
        "sync message failed",
    ) {
        Ok(saved) => (StatusCode::OK, Json(json!(saved))),
        Err(err) => err,
    }
}

async fn batch_create_messages(
    auth: AuthUser,
    Path(session_id): Path<String>,
    Json(req): Json<CompatBatchCreateMessagesRequest>,
) -> (StatusCode, Json<Value>) {
    match compat_scoped_result(
        compat_runtime::batch_create_messages_for_auth(
            &auth,
            session_id.as_str(),
            req.messages
                .into_iter()
                .map(compat_message_input_from_create)
                .collect(),
        )
        .await,
        "batch create messages failed",
    ) {
        Ok(items) => (StatusCode::OK, Json(json!({ "items": items }))),
        Err(err) => err,
    }
}

async fn clear_session_messages(
    auth: AuthUser,
    Path(session_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    match compat_scoped_result(
        compat_runtime::clear_session_messages_for_auth(&auth, session_id.as_str()).await,
        "clear messages failed",
    ) {
        Ok(deleted) => (
            StatusCode::OK,
            Json(json!({"deleted": deleted, "success": true})),
        ),
        Err(err) => err,
    }
}

async fn get_message(auth: AuthUser, Path(message_id): Path<String>) -> (StatusCode, Json<Value>) {
    match compat_message_result(
        compat_runtime::get_message_for_auth(&auth, message_id.as_str()).await,
        "get message failed",
    ) {
        Ok(message) => (StatusCode::OK, Json(json!(message))),
        Err(err) => err,
    }
}

async fn delete_message(
    auth: AuthUser,
    Path(message_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    match compat_message_result(
        compat_runtime::delete_message_for_auth(&auth, message_id.as_str()).await,
        "delete message failed",
    ) {
        Ok(true) => (StatusCode::OK, Json(json!({"success": true}))),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "message not found"})),
        ),
        Err(err) => err,
    }
}

async fn list_summaries(
    auth: AuthUser,
    Path(session_id): Path<String>,
    Query(query): Query<CompatListSummariesQuery>,
) -> (StatusCode, Json<Value>) {
    match compat_scoped_result(
        compat_runtime::list_summaries_for_auth(
            &auth,
            session_id.as_str(),
            Some(query.limit.unwrap_or(100).max(1).min(1000)),
            query.offset.unwrap_or(0).max(0),
        )
        .await,
        "list summaries failed",
    ) {
        Ok(items) => (StatusCode::OK, Json(json!({ "items": items }))),
        Err(err) => err,
    }
}

async fn delete_summary(
    auth: AuthUser,
    Path((session_id, summary_id)): Path<(String, String)>,
) -> (StatusCode, Json<Value>) {
    match compat_scoped_result(
        compat_runtime::delete_summary_for_auth(&auth, session_id.as_str(), summary_id.as_str())
            .await,
        "delete summary failed",
    ) {
        Ok(result) if result.success => (
            StatusCode::OK,
            Json(json!({"success": true, "reset_messages": result.reset_messages})),
        ),
        Ok(_) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "summary not found"})),
        ),
        Err(err) => err,
    }
}

async fn sync_turn_runtime_snapshot(
    auth: AuthUser,
    Path((session_id, turn_id)): Path<(String, String)>,
    Json(req): Json<SyncTurnRuntimeSnapshotRequestDto>,
) -> (StatusCode, Json<Value>) {
    match compat_scoped_result(
        compat_runtime::sync_turn_runtime_snapshot_for_auth(
            &auth,
            session_id.as_str(),
            turn_id.as_str(),
            &req,
        )
        .await,
        "sync turn runtime snapshot failed",
    ) {
        Ok(snapshot) => (StatusCode::OK, Json(json!(snapshot))),
        Err(err) => err,
    }
}

async fn get_latest_turn_runtime_snapshot(
    auth: AuthUser,
    Path(session_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    match compat_scoped_result(
        compat_runtime::get_latest_turn_runtime_snapshot_for_auth(&auth, session_id.as_str()).await,
        "load latest runtime snapshot failed",
    ) {
        Ok(snapshot) => (StatusCode::OK, Json(json!(snapshot))),
        Err(err) => err,
    }
}

async fn get_turn_runtime_snapshot_by_turn(
    auth: AuthUser,
    Path((session_id, turn_id)): Path<(String, String)>,
) -> (StatusCode, Json<Value>) {
    match compat_scoped_result(
        compat_runtime::get_turn_runtime_snapshot_by_turn_for_auth(
            &auth,
            session_id.as_str(),
            turn_id.as_str(),
        )
        .await,
        "load runtime snapshot failed",
    ) {
        Ok(snapshot) => (StatusCode::OK, Json(json!(snapshot))),
        Err(err) => err,
    }
}

async fn compose_context(
    auth: AuthUser,
    Json(req): Json<CompatComposeContextRequest>,
) -> (StatusCode, Json<Value>) {
    let payload = match compat_scoped_result(
        compat_runtime::compose_context_for_auth(
            &auth,
            req.session_id.as_str(),
            req.include_raw_messages,
        )
        .await,
        "compose context failed",
    ) {
        Ok(value) => value,
        Err(err) => return err,
    };
    let _ignored_mode = req.mode;
    (StatusCode::OK, Json(json!(payload)))
}
