use axum::http::StatusCode;
use axum::{
    extract::{Path, Query},
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::core::auth::AuthUser;
use crate::core::user_scope::resolve_user_id;
use crate::models::memory_compat::{
    MemoryCompatComposeContextMeta, MemoryCompatComposeContextResponse,
};
use crate::models::memory_runtime_types::SyncTurnRuntimeSnapshotRequestDto;
use crate::models::message::Message;
use crate::models::session::Session;
use crate::services::{chatos_memory_engine, chatos_sessions};

#[derive(Debug, Deserialize)]
struct CompatSessionQuery {
    user_id: Option<String>,
    project_id: Option<String>,
    status: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct CompatListMessagesQuery {
    limit: Option<i64>,
    offset: Option<i64>,
    order: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CompatListSummariesQuery {
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct CompatComposeContextRequest {
    session_id: String,
    mode: Option<String>,
    summary_limit: Option<usize>,
    pending_limit: Option<usize>,
    include_raw_messages: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct CompatCreateSessionRequest {
    user_id: String,
    project_id: Option<String>,
    title: Option<String>,
    metadata: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct CompatPatchSessionRequest {
    title: Option<String>,
    status: Option<String>,
    metadata: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct CompatCreateMessageRequest {
    role: String,
    content: String,
    message_mode: Option<String>,
    message_source: Option<String>,
    tool_calls: Option<Value>,
    tool_call_id: Option<String>,
    reasoning: Option<String>,
    metadata: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct CompatBatchCreateMessagesRequest {
    messages: Vec<CompatCreateMessageRequest>,
}

#[derive(Debug, Deserialize)]
struct CompatSyncMessageRequest {
    role: String,
    content: String,
    message_mode: Option<String>,
    message_source: Option<String>,
    tool_calls: Option<Value>,
    tool_call_id: Option<String>,
    reasoning: Option<String>,
    metadata: Option<Value>,
    created_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CompatSyncSessionRequest {
    user_id: String,
    project_id: Option<String>,
    title: Option<String>,
    metadata: Option<Value>,
    status: Option<String>,
    created_at: Option<String>,
    updated_at: Option<String>,
}

pub fn router() -> Router {
    Router::new()
        .route(
            "/api/memory/v1/sessions",
            get(list_sessions).post(create_session),
        )
        .route(
            "/api/memory/v1/sessions/:session_id",
            get(get_session).patch(update_session).delete(delete_session),
        )
        .route(
            "/api/memory/v1/sessions/:session_id/sync",
            put(sync_session),
        )
        .route(
            "/api/memory/v1/sessions/:session_id/messages",
            get(list_messages).post(create_message).delete(clear_session_messages),
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
        .route(
            "/api/memory/v1/context/compose",
            post(compose_context),
        )
}

async fn list_sessions(
    auth: AuthUser,
    Query(query): Query<CompatSessionQuery>,
) -> (StatusCode, Json<Value>) {
    let user_id = match resolve_scope_user_id(query.user_id, &auth) {
        Ok(value) => value,
        Err(err) => return err,
    };
    let include_archived = matches!(query.status.as_deref(), Some("archived"));
    match chatos_sessions::list_sessions(
        Some(user_id.as_str()),
        query.project_id.as_deref(),
        Some(query.limit.unwrap_or(50).max(1).min(500)),
        query.offset.unwrap_or(0).max(0),
        include_archived,
        false,
    )
    .await
    {
        Ok(items) => (StatusCode::OK, Json(json!({ "items": items }))),
        Err(err) => compat_internal_error("list sessions failed", err),
    }
}

async fn create_session(
    auth: AuthUser,
    Json(mut req): Json<CompatCreateSessionRequest>,
) -> (StatusCode, Json<Value>) {
    let user_id = match resolve_scope_user_id(Some(req.user_id.clone()), &auth) {
        Ok(value) => value,
        Err(err) => return err,
    };
    let title = req
        .title
        .take()
        .unwrap_or_else(|| "Untitled".to_string())
        .trim()
        .to_string();
    if title.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "title is required"})),
        );
    }
    match chatos_sessions::create_session(user_id, title, req.project_id, req.metadata).await {
        Ok(session) => (StatusCode::OK, Json(json!(session))),
        Err(err) => compat_internal_error("create session failed", err),
    }
}

async fn sync_session(
    auth: AuthUser,
    Path(session_id): Path<String>,
    Json(req): Json<CompatSyncSessionRequest>,
) -> (StatusCode, Json<Value>) {
    let scope_user_id = match resolve_scope_user_id(Some(req.user_id.clone()), &auth) {
        Ok(value) => value,
        Err(err) => return err,
    };

    let existing = match chatos_sessions::get_session_by_id(session_id.as_str()).await {
        Ok(value) => value,
        Err(err) => return compat_internal_error("load session failed", err),
    };

    let session = if let Some(current) = existing {
        if current.user_id.as_deref() != Some(auth.user_id.as_str()) {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({"error": "forbidden"})),
            );
        }
        let title = req.title.or(Some(current.title.clone()));
        match chatos_memory_engine::update_chatos_session(
            session_id.as_str(),
            title,
            req.status,
            req.metadata.or(current.metadata),
        )
        .await
        {
            Ok(Some(updated)) => updated,
            Ok(None) => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(json!({"error": "session not found"})),
                )
            }
            Err(err) => return compat_internal_error("sync session failed", err),
        }
    } else {
        let title = req.title.unwrap_or_else(|| "Untitled".to_string());
        let mut created = match chatos_memory_engine::create_chatos_session(
            scope_user_id,
            title,
            req.project_id,
            req.metadata,
        )
        .await
        {
            Ok(session) => session,
            Err(err) => return compat_internal_error("sync session failed", err),
        };
        if created.id != session_id {
            created.id = session_id.clone();
            match chatos_memory_engine::sync_chatos_session(&created).await {
                Ok(()) => {}
                Err(err) => return compat_internal_error("sync session failed", err),
            }
        }
        if let Some(status) = req.status {
            match chatos_memory_engine::update_chatos_session(
                session_id.as_str(),
                None,
                Some(status),
                None,
            )
            .await
            {
                Ok(Some(updated)) => updated,
                Ok(None) => created,
                Err(err) => return compat_internal_error("sync session failed", err),
            }
        } else {
            created
        }
    };

    let _ = req.created_at;
    let _ = req.updated_at;
    (StatusCode::OK, Json(json!(session)))
}

async fn get_session(
    auth: AuthUser,
    Path(session_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let session = match ensure_session_read_access(session_id.as_str(), &auth).await {
        Ok(session) => session,
        Err(err) => return err,
    };
    (StatusCode::OK, Json(json!(session)))
}

async fn update_session(
    auth: AuthUser,
    Path(session_id): Path<String>,
    Json(req): Json<CompatPatchSessionRequest>,
) -> (StatusCode, Json<Value>) {
    let _session = match ensure_session_read_access(session_id.as_str(), &auth).await {
        Ok(session) => session,
        Err(err) => return err,
    };
    match chatos_sessions::update_session(
        session_id.as_str(),
        req.title,
        req.status,
        req.metadata,
    )
    .await
    {
        Ok(Some(session)) => (StatusCode::OK, Json(json!(session))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "session not found"})),
        ),
        Err(err) => compat_internal_error("update session failed", err),
    }
}

async fn delete_session(
    auth: AuthUser,
    Path(session_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let _session = match ensure_session_read_access(session_id.as_str(), &auth).await {
        Ok(session) => session,
        Err(err) => return err,
    };
    match chatos_sessions::delete_session(session_id.as_str()).await {
        Ok(true) => (StatusCode::OK, Json(json!({"success": true}))),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "session not found"})),
        ),
        Err(err) => compat_internal_error("delete session failed", err),
    }
}

async fn list_messages(
    auth: AuthUser,
    Path(session_id): Path<String>,
    Query(query): Query<CompatListMessagesQuery>,
) -> (StatusCode, Json<Value>) {
    let _session = match ensure_session_read_access(session_id.as_str(), &auth).await {
        Ok(session) => session,
        Err(err) => return err,
    };
    let asc = !matches!(query.order.as_deref(), Some("desc"));
    match chatos_sessions::list_messages(
        session_id.as_str(),
        Some(query.limit.unwrap_or(100).max(1).min(2000)),
        query.offset.unwrap_or(0).max(0),
        asc,
    )
    .await
    {
        Ok(items) => (StatusCode::OK, Json(json!({ "items": items }))),
        Err(err) => compat_internal_error("list messages failed", err),
    }
}

async fn create_message(
    auth: AuthUser,
    Path(session_id): Path<String>,
    Json(req): Json<CompatCreateMessageRequest>,
) -> (StatusCode, Json<Value>) {
    let _session = match ensure_session_read_access(session_id.as_str(), &auth).await {
        Ok(session) => session,
        Err(err) => return err,
    };
    let message = build_message_from_compat(session_id.as_str(), req, None);
    match chatos_sessions::upsert_message(&message).await {
        Ok(saved) => (StatusCode::OK, Json(json!(saved))),
        Err(err) => compat_internal_error("create message failed", err),
    }
}

async fn sync_message(
    auth: AuthUser,
    Path((session_id, message_id)): Path<(String, String)>,
    Json(req): Json<CompatSyncMessageRequest>,
) -> (StatusCode, Json<Value>) {
    let _session = match ensure_session_read_access(session_id.as_str(), &auth).await {
        Ok(session) => session,
        Err(err) => return err,
    };
    let message = build_message_from_compat(
        session_id.as_str(),
        CompatCreateMessageRequest {
            role: req.role,
            content: req.content,
            message_mode: req.message_mode,
            message_source: req.message_source,
            tool_calls: req.tool_calls,
            tool_call_id: req.tool_call_id,
            reasoning: req.reasoning,
            metadata: req.metadata,
        },
        Some((message_id, req.created_at)),
    );
    match chatos_sessions::upsert_message(&message).await {
        Ok(saved) => (StatusCode::OK, Json(json!(saved))),
        Err(err) => compat_internal_error("sync message failed", err),
    }
}

async fn batch_create_messages(
    auth: AuthUser,
    Path(session_id): Path<String>,
    Json(req): Json<CompatBatchCreateMessagesRequest>,
) -> (StatusCode, Json<Value>) {
    let _session = match ensure_session_read_access(session_id.as_str(), &auth).await {
        Ok(session) => session,
        Err(err) => return err,
    };
    let mut out = Vec::with_capacity(req.messages.len());
    for item in req.messages {
        let message = build_message_from_compat(session_id.as_str(), item, None);
        match chatos_sessions::upsert_message(&message).await {
            Ok(saved) => out.push(saved),
            Err(err) => return compat_internal_error("batch create messages failed", err),
        }
    }
    (StatusCode::OK, Json(json!({ "items": out })))
}

async fn clear_session_messages(
    auth: AuthUser,
    Path(session_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let _session = match ensure_session_read_access(session_id.as_str(), &auth).await {
        Ok(session) => session,
        Err(err) => return err,
    };
    match chatos_sessions::delete_messages_by_session(session_id.as_str()).await {
        Ok(deleted) => (StatusCode::OK, Json(json!({"deleted": deleted, "success": true}))),
        Err(err) => compat_internal_error("clear messages failed", err),
    }
}

async fn get_message(
    auth: AuthUser,
    Path(message_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    match chatos_sessions::get_message_by_id(message_id.as_str()).await {
        Ok(Some(message)) => {
            if let Err(err) = ensure_session_read_access(message.session_id.as_str(), &auth).await {
                return err;
            }
            (StatusCode::OK, Json(json!(message)))
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "message not found"})),
        ),
        Err(err) => compat_internal_error("get message failed", err),
    }
}

async fn delete_message(
    auth: AuthUser,
    Path(message_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let message = match chatos_sessions::get_message_by_id(message_id.as_str()).await {
        Ok(Some(message)) => message,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "message not found"})),
            )
        }
        Err(err) => return compat_internal_error("delete message failed", err),
    };
    if let Err(err) = ensure_session_read_access(message.session_id.as_str(), &auth).await {
        return err;
    }
    match chatos_sessions::delete_message(message_id.as_str()).await {
        Ok(true) => (StatusCode::OK, Json(json!({"success": true}))),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "message not found"})),
        ),
        Err(err) => compat_internal_error("delete message failed", err),
    }
}

async fn list_summaries(
    auth: AuthUser,
    Path(session_id): Path<String>,
    Query(query): Query<CompatListSummariesQuery>,
) -> (StatusCode, Json<Value>) {
    let _session = match ensure_session_read_access(session_id.as_str(), &auth).await {
        Ok(session) => session,
        Err(err) => return err,
    };
    match chatos_sessions::list_summaries(
        session_id.as_str(),
        Some(query.limit.unwrap_or(100).max(1).min(1000)),
        query.offset.unwrap_or(0).max(0),
    )
    .await
    {
        Ok(items) => (StatusCode::OK, Json(json!({ "items": items }))),
        Err(err) => compat_internal_error("list summaries failed", err),
    }
}

async fn delete_summary(
    auth: AuthUser,
    Path((session_id, summary_id)): Path<(String, String)>,
) -> (StatusCode, Json<Value>) {
    let _session = match ensure_session_read_access(session_id.as_str(), &auth).await {
        Ok(session) => session,
        Err(err) => return err,
    };
    match chatos_sessions::delete_summary(session_id.as_str(), summary_id.as_str()).await {
        Ok(result) if result.success => (
            StatusCode::OK,
            Json(json!({"success": true, "reset_messages": result.reset_messages})),
        ),
        Ok(_) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "summary not found"})),
        ),
        Err(err) => compat_internal_error("delete summary failed", err),
    }
}

async fn sync_turn_runtime_snapshot(
    auth: AuthUser,
    Path((session_id, turn_id)): Path<(String, String)>,
    Json(req): Json<SyncTurnRuntimeSnapshotRequestDto>,
) -> (StatusCode, Json<Value>) {
    let _session = match ensure_session_read_access(session_id.as_str(), &auth).await {
        Ok(session) => session,
        Err(err) => return err,
    };
    match chatos_sessions::sync_turn_runtime_snapshot(session_id.as_str(), turn_id.as_str(), &req).await {
        Ok(snapshot) => (StatusCode::OK, Json(json!(snapshot))),
        Err(err) => compat_internal_error("sync turn runtime snapshot failed", err),
    }
}

async fn get_latest_turn_runtime_snapshot(
    auth: AuthUser,
    Path(session_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let _session = match ensure_session_read_access(session_id.as_str(), &auth).await {
        Ok(session) => session,
        Err(err) => return err,
    };
    match chatos_sessions::get_latest_turn_runtime_snapshot(session_id.as_str()).await {
        Ok(snapshot) => (StatusCode::OK, Json(json!(snapshot))),
        Err(err) => compat_internal_error("load latest runtime snapshot failed", err),
    }
}

async fn get_turn_runtime_snapshot_by_turn(
    auth: AuthUser,
    Path((session_id, turn_id)): Path<(String, String)>,
) -> (StatusCode, Json<Value>) {
    let _session = match ensure_session_read_access(session_id.as_str(), &auth).await {
        Ok(session) => session,
        Err(err) => return err,
    };
    match chatos_sessions::get_turn_runtime_snapshot_by_turn(session_id.as_str(), turn_id.as_str()).await {
        Ok(snapshot) => (StatusCode::OK, Json(json!(snapshot))),
        Err(err) => compat_internal_error("load runtime snapshot failed", err),
    }
}

async fn compose_context(
    auth: AuthUser,
    Json(req): Json<CompatComposeContextRequest>,
) -> (StatusCode, Json<Value>) {
    let session = match ensure_session_read_access(req.session_id.as_str(), &auth).await {
        Ok(session) => session,
        Err(err) => return err,
    };
    let payload = match chatos_memory_engine::compose_chatos_context(
        &session,
        req.summary_limit.or(req.pending_limit).unwrap_or(20).max(1),
        req.include_raw_messages.unwrap_or(true),
    )
    .await
    {
        Ok(value) => value,
        Err(err) => return compat_internal_error("compose context failed", err),
    };
    let _ = req.mode;
    (
        StatusCode::OK,
        Json(json!(MemoryCompatComposeContextResponse {
            session_id: session.id,
            merged_summary: payload.merged_summary,
            summary_count: payload.summary_count,
            messages: payload.messages,
            meta: MemoryCompatComposeContextMeta {
                used_levels: Vec::new(),
                filtered_rollup_count: 0,
                kept_raw_level0_count: 0,
            },
        })),
    )
}

fn build_message_from_compat(
    session_id: &str,
    req: CompatCreateMessageRequest,
    sync_hint: Option<(String, Option<String>)>,
) -> Message {
    let mut message = Message::new(session_id.to_string(), req.role, req.content);
    if let Some((message_id, created_at)) = sync_hint {
        message.id = message_id;
        if let Some(created_at) = created_at {
            message.created_at = created_at;
        }
    }
    message.message_mode = req.message_mode;
    message.message_source = req.message_source;
    message.tool_calls = req.tool_calls;
    message.tool_call_id = req.tool_call_id;
    message.reasoning = req.reasoning;
    message.metadata = req.metadata;
    message
}

async fn ensure_session_read_access(
    session_id: &str,
    auth: &AuthUser,
) -> Result<Session, (StatusCode, Json<Value>)> {
    match chatos_sessions::get_session_by_id(session_id).await {
        Ok(Some(session)) => {
            let owner_user_id = session.user_id.clone().unwrap_or_default();
            if owner_user_id == auth.user_id {
                Ok(session)
            } else {
                Err((StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"}))))
            }
        }
        Ok(None) => Err((StatusCode::NOT_FOUND, Json(json!({"error": "session not found"})))),
        Err(err) => Err(compat_internal_error("load session failed", err)),
    }
}

fn resolve_scope_user_id(
    requested_user_id: Option<String>,
    auth: &AuthUser,
) -> Result<String, (StatusCode, Json<Value>)> {
    resolve_user_id(requested_user_id, auth)
}

fn compat_internal_error(context: &str, detail: String) -> (StatusCode, Json<Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({
            "error": context,
            "detail": detail,
        })),
    )
}
