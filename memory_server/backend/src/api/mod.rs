use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{delete, get, patch, post, put};
use axum::{Json, Router};
use base64::Engine;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::ai::AiClient;
use crate::jobs;
use crate::models::{
    BatchCreateMessagesRequest, ComposeContextRequest, CreateMessageRequest, CreateSessionRequest,
    UpsertAiModelConfigRequest, UpsertSummaryJobConfigRequest,
    UpsertSummaryRollupJobConfigRequest, UpdateSessionRequest,
};
use crate::repositories::{
    auth as auth_repo, configs, jobs as job_repo, messages, sessions, summaries,
};
use crate::services::context;
use crate::state::AppState;

pub type SharedState = Arc<AppState>;

pub fn router(state: SharedState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/memory/v1/auth/login", post(login))
        .route("/api/memory/v1/auth/me", get(me))
        .route("/api/memory/v1/sessions", post(create_session).get(list_sessions))
        .route(
            "/api/memory/v1/sessions/:session_id/sync",
            put(sync_session),
        )
        .route(
            "/api/memory/v1/sessions/:session_id",
            get(get_session).patch(update_session).delete(delete_session),
        )
        .route(
            "/api/memory/v1/sessions/:session_id/messages",
            post(create_message).get(list_messages).delete(clear_session_messages),
        )
        .route(
            "/api/memory/v1/sessions/:session_id/messages/:message_id/sync",
            put(sync_message),
        )
        .route(
            "/api/memory/v1/sessions/:session_id/messages/batch",
            post(batch_create_messages),
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
            "/api/memory/v1/sessions/:session_id/summaries/levels",
            get(summary_levels),
        )
        .route(
            "/api/memory/v1/sessions/:session_id/summaries/graph",
            get(summary_graph),
        )
        .route(
            "/api/memory/v1/sessions/:session_id/summaries/:summary_id",
            delete(delete_summary),
        )
        .route(
            "/api/memory/v1/configs/models",
            get(list_model_configs).post(create_model_config),
        )
        .route(
            "/api/memory/v1/configs/models/:model_id",
            patch(update_model_config).delete(delete_model_config),
        )
        .route(
            "/api/memory/v1/configs/models/:model_id/test",
            post(test_model_config),
        )
        .route(
            "/api/memory/v1/configs/summary-job",
            get(get_summary_job_config).put(put_summary_job_config),
        )
        .route(
            "/api/memory/v1/configs/summary-rollup-job",
            get(get_summary_rollup_job_config).put(put_summary_rollup_job_config),
        )
        .route(
            "/api/memory/v1/jobs/summary/run-once",
            post(run_summary_once),
        )
        .route(
            "/api/memory/v1/jobs/summary-rollup/run-once",
            post(run_rollup_once),
        )
        .route("/api/memory/v1/jobs/runs", get(list_job_runs))
        .route("/api/memory/v1/jobs/stats", get(job_stats))
        .route("/api/memory/v1/context/compose", post(compose_context))
        .with_state(state)
}

async fn health() -> Json<Value> {
    Json(json!({"status": "ok", "service": "memory_server"}))
}

#[derive(Debug, Clone)]
struct AuthIdentity {
    user_id: String,
    role: String,
}

impl AuthIdentity {
    fn is_admin(&self) -> bool {
        self.role == auth_repo::ADMIN_ROLE || self.user_id == auth_repo::ADMIN_USER_ID
    }
}

#[derive(Debug, Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
}

async fn login(
    State(state): State<SharedState>,
    Json(req): Json<LoginRequest>,
) -> (StatusCode, Json<Value>) {
    let username = req.username.trim().to_string();
    let password = req.password.trim().to_string();

    if username.is_empty() || password.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "username/password required"})),
        );
    }

    let verified = match auth_repo::verify_user_password(&state.pool, username.as_str(), password.as_str()).await
    {
        Ok(Some(user)) => Some(user),
        Ok(None) => None,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "login failed", "detail": err})),
            )
        }
    };

    let user = if let Some(user) = verified {
        user
    } else {
        if username == auth_repo::ADMIN_USER_ID {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "invalid credentials"})),
            );
        }

        match auth_repo::create_user(&state.pool, username.as_str(), password.as_str(), auth_repo::USER_ROLE)
            .await
        {
            Ok(v) => v,
            Err(err) => {
                return (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({"error": "invalid credentials", "detail": err})),
                )
            }
        }
    };

    let token = build_auth_token(
        user.user_id.as_str(),
        user.role.as_str(),
        state.config.auth_secret.as_str(),
        state.config.auth_token_ttl_hours,
    );

    (
        StatusCode::OK,
        Json(json!({
            "token": token,
            "user_id": user.user_id,
            "role": user.role
        })),
    )
}

async fn me(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };

    (
        StatusCode::OK,
        Json(json!({
            "user_id": auth.user_id,
            "role": auth.role
        })),
    )
}

fn resolve_identity(
    headers: &HeaderMap,
    state: &AppState,
) -> Result<AuthIdentity, (StatusCode, Json<Value>)> {
    if is_valid_service_token(headers, state) {
        return Ok(AuthIdentity {
            user_id: auth_repo::ADMIN_USER_ID.to_string(),
            role: auth_repo::ADMIN_ROLE.to_string(),
        });
    }

    let token_from_header = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.trim().to_string())
        .and_then(|v| {
            v.strip_prefix("Bearer ")
                .or_else(|| v.strip_prefix("bearer "))
                .map(|s| s.trim().to_string())
        });

    let Some(token) = token_from_header else {
        return Err((StatusCode::UNAUTHORIZED, Json(json!({"error": "unauthorized"}))));
    };

    let parsed = parse_auth_token(token.as_str(), state.config.auth_secret.as_str()).ok_or_else(|| {
        (StatusCode::UNAUTHORIZED, Json(json!({"error": "unauthorized"})))
    })?;

    Ok(AuthIdentity {
        user_id: parsed.0,
        role: parsed.1,
    })
}

fn is_valid_service_token(headers: &HeaderMap, state: &AppState) -> bool {
    let Some(expected) = state.config.service_token.as_ref() else {
        return false;
    };

    headers
        .get("x-service-token")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .as_deref()
        == Some(expected.as_str())
}

fn resolve_scope_user_id(auth: &AuthIdentity, requested_user_id: Option<String>) -> String {
    if auth.is_admin() {
        requested_user_id
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| auth.user_id.clone())
    } else {
        auth.user_id.clone()
    }
}

async fn ensure_session_access(
    state: &AppState,
    auth: &AuthIdentity,
    session_id: &str,
) -> Result<(), (StatusCode, Json<Value>)> {
    if auth.is_admin() {
        return Ok(());
    }

    match sessions::get_session_by_id(&state.pool, session_id).await {
        Ok(Some(session)) => {
            if session.user_id == auth.user_id {
                Ok(())
            } else {
                Err((StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"}))))
            }
        }
        Ok(None) => Err((StatusCode::NOT_FOUND, Json(json!({"error": "session not found"})))),
        Err(err) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "load session failed", "detail": err})),
        )),
    }
}

fn build_auth_token(user_id: &str, role: &str, secret: &str, ttl_hours: i64) -> String {
    let exp = (chrono::Utc::now() + chrono::Duration::hours(ttl_hours.max(1))).timestamp();
    let payload = format!("{}|{}|{}", user_id, role, exp);
    let sig = sign(payload.as_str(), secret);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(format!("{}|{}", payload, sig))
}

fn parse_auth_token(token: &str, secret: &str) -> Option<(String, String, i64)> {
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(token.as_bytes())
        .ok()?;
    let decoded = String::from_utf8(decoded).ok()?;
    let mut parts = decoded.split('|');
    let user_id = parts.next()?.to_string();
    let role = parts.next()?.to_string();
    let exp = parts.next()?.parse::<i64>().ok()?;
    let sig = parts.next()?.to_string();
    if parts.next().is_some() {
        return None;
    }

    let payload = format!("{}|{}|{}", user_id, role, exp);
    if sign(payload.as_str(), secret) != sig {
        return None;
    }
    if chrono::Utc::now().timestamp() > exp {
        return None;
    }
    Some((user_id, role, exp))
}

fn sign(payload: &str, secret: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(payload.as_bytes());
    hasher.update(b"|");
    hasher.update(secret.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[derive(Debug, Deserialize)]
struct ListSessionsQuery {
    user_id: Option<String>,
    project_id: Option<String>,
    status: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct SyncSessionRequest {
    user_id: String,
    project_id: Option<String>,
    title: Option<String>,
    status: Option<String>,
    created_at: Option<String>,
    updated_at: Option<String>,
}

async fn create_session(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(mut req): Json<CreateSessionRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if !auth.is_admin() {
        req.user_id = auth.user_id;
    } else if req.user_id.trim().is_empty() {
        req.user_id = auth.user_id;
    }

    match sessions::create_session(&state.pool, req).await {
        Ok(session) => (StatusCode::OK, Json(json!(session))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "create session failed", "detail": err})),
        ),
    }
}

async fn sync_session(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Json(req): Json<SyncSessionRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if !auth.is_admin() {
        return (StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"})));
    }

    match sessions::upsert_session_sync(
        &state.pool,
        session_id.as_str(),
        req.user_id.as_str(),
        req.project_id,
        req.title,
        req.status,
        req.created_at,
        req.updated_at,
    )
    .await
    {
        Ok(session) => (StatusCode::OK, Json(json!(session))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "sync session failed", "detail": err})),
        ),
    }
}

async fn list_sessions(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(q): Query<ListSessionsQuery>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };

    let limit = q.limit.unwrap_or(50);
    let offset = q.offset.unwrap_or(0);
    let scope_user_id = if auth.is_admin() {
        q.user_id.as_deref()
    } else {
        Some(auth.user_id.as_str())
    };
    match sessions::list_sessions(
        &state.pool,
        scope_user_id,
        q.project_id.as_deref(),
        q.status.as_deref(),
        limit,
        offset,
    )
    .await
    {
        Ok(items) => (StatusCode::OK, Json(json!({"items": items}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list sessions failed", "detail": err})),
        ),
    }
}

async fn delete_session(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if let Err(err) = ensure_session_access(state.as_ref(), &auth, session_id.as_str()).await {
        return err;
    }

    match sessions::delete_session(&state.pool, session_id.as_str()).await {
        Ok(true) => (StatusCode::OK, Json(json!({ "success": true }))),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "session not found"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "delete session failed", "detail": err})),
        ),
    }
}

async fn get_session(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if let Err(err) = ensure_session_access(state.as_ref(), &auth, session_id.as_str()).await {
        return err;
    }

    match sessions::get_session_by_id(&state.pool, session_id.as_str()).await {
        Ok(Some(session)) => (StatusCode::OK, Json(json!(session))),
        Ok(None) => (StatusCode::NOT_FOUND, Json(json!({"error": "session not found"}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "get session failed", "detail": err})),
        ),
    }
}

async fn update_session(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Json(req): Json<UpdateSessionRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if let Err(err) = ensure_session_access(state.as_ref(), &auth, session_id.as_str()).await {
        return err;
    }

    match sessions::update_session(&state.pool, session_id.as_str(), req).await {
        Ok(Some(session)) => (StatusCode::OK, Json(json!(session))),
        Ok(None) => (StatusCode::NOT_FOUND, Json(json!({"error": "session not found"}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "update session failed", "detail": err})),
        ),
    }
}

#[derive(Debug, Deserialize)]
struct ListMessagesQuery {
    limit: Option<i64>,
    offset: Option<i64>,
    order: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SyncMessageRequest {
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

async fn create_message(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Json(req): Json<CreateMessageRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if let Err(err) = ensure_session_access(state.as_ref(), &auth, session_id.as_str()).await {
        return err;
    }

    match messages::create_message(&state.pool, session_id.as_str(), req).await {
        Ok(message) => (StatusCode::OK, Json(json!(message))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "create message failed", "detail": err})),
        ),
    }
}

async fn sync_message(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((session_id, message_id)): Path<(String, String)>,
    Json(req): Json<SyncMessageRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if let Err(err) = ensure_session_access(state.as_ref(), &auth, session_id.as_str()).await {
        return err;
    }

    let created_at = req
        .created_at
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
    let input = messages::SyncMessageInput {
        message_id,
        role: req.role,
        content: req.content,
        message_mode: req.message_mode,
        message_source: req.message_source,
        tool_calls_json: req.tool_calls.map(|v| v.to_string()),
        tool_call_id: req.tool_call_id,
        reasoning: req.reasoning,
        metadata_json: req.metadata.map(|v| v.to_string()),
        created_at,
    };

    match messages::upsert_message_sync(&state.pool, session_id.as_str(), input).await {
        Ok(message) => (StatusCode::OK, Json(json!(message))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "sync message failed", "detail": err})),
        ),
    }
}

async fn batch_create_messages(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Json(req): Json<BatchCreateMessagesRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if let Err(err) = ensure_session_access(state.as_ref(), &auth, session_id.as_str()).await {
        return err;
    }

    match messages::batch_create_messages(&state.pool, session_id.as_str(), req.messages).await {
        Ok(items) => (StatusCode::OK, Json(json!({"items": items}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "batch create messages failed", "detail": err})),
        ),
    }
}

async fn list_messages(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Query(q): Query<ListMessagesQuery>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if let Err(err) = ensure_session_access(state.as_ref(), &auth, session_id.as_str()).await {
        return err;
    }

    let asc = !matches!(q.order.as_deref(), Some("desc"));
    let limit = q.limit.unwrap_or(100);
    let offset = q.offset.unwrap_or(0);

    match messages::list_messages_by_session(&state.pool, session_id.as_str(), limit, offset, asc).await {
        Ok(items) => (StatusCode::OK, Json(json!({"items": items}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list messages failed", "detail": err})),
        ),
    }
}

async fn clear_session_messages(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if let Err(err) = ensure_session_access(state.as_ref(), &auth, session_id.as_str()).await {
        return err;
    }

    match messages::delete_messages_by_session(&state.pool, session_id.as_str()).await {
        Ok(deleted) => (StatusCode::OK, Json(json!({"deleted": deleted, "success": true}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "clear messages failed", "detail": err})),
        ),
    }
}

async fn get_message(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(message_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if !auth.is_admin() {
        return (StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"})));
    }

    match messages::get_message_by_id(&state.pool, message_id.as_str()).await {
        Ok(Some(item)) => (StatusCode::OK, Json(json!(item))),
        Ok(None) => (StatusCode::NOT_FOUND, Json(json!({"error": "message not found"}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "get message failed", "detail": err})),
        ),
    }
}

async fn delete_message(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(message_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if !auth.is_admin() {
        return (StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"})));
    }

    match messages::delete_message_by_id(&state.pool, message_id.as_str()).await {
        Ok(true) => (StatusCode::OK, Json(json!({"success": true}))),
        Ok(false) => (StatusCode::NOT_FOUND, Json(json!({"error": "message not found"}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "delete message failed", "detail": err})),
        ),
    }
}

#[derive(Debug, Deserialize)]
struct ListSummariesQuery {
    level: Option<i64>,
    status: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

async fn list_summaries(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Query(q): Query<ListSummariesQuery>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if let Err(err) = ensure_session_access(state.as_ref(), &auth, session_id.as_str()).await {
        return err;
    }

    match summaries::list_summaries(
        &state.pool,
        session_id.as_str(),
        q.level,
        q.status.as_deref().or(Some("pending")),
        q.limit.unwrap_or(100),
        q.offset.unwrap_or(0),
    )
    .await
    {
        Ok(items) => (StatusCode::OK, Json(json!({"items": items}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list summaries failed", "detail": err})),
        ),
    }
}

async fn summary_levels(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if let Err(err) = ensure_session_access(state.as_ref(), &auth, session_id.as_str()).await {
        return err;
    }

    match summaries::list_summary_level_stats(&state.pool, session_id.as_str()).await {
        Ok(levels) => {
            let items: Vec<Value> = levels
                .into_iter()
                .map(|(level, total, pending)| {
                    json!({
                        "level": level,
                        "total": total,
                        "pending": pending,
                        "summarized": total.saturating_sub(pending),
                    })
                })
                .collect();
            (StatusCode::OK, Json(json!({"items": items})))
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list summary levels failed", "detail": err})),
        ),
    }
}

async fn summary_graph(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if let Err(err) = ensure_session_access(state.as_ref(), &auth, session_id.as_str()).await {
        return err;
    }

    match summaries::list_all_summaries_by_session(&state.pool, session_id.as_str()).await {
        Ok(items) => {
            let nodes: Vec<Value> = items
                .iter()
                .map(|s| {
                    let excerpt: String = s.summary_text.chars().take(120).collect();
                    json!({
                        "id": s.id,
                        "level": s.level,
                        "status": s.status,
                        "rollup_summary_id": s.rollup_summary_id,
                        "created_at": s.created_at,
                        "summary_excerpt": excerpt,
                    })
                })
                .collect();

            let edges: Vec<Value> = items
                .iter()
                .filter_map(|s| {
                    s.rollup_summary_id.as_ref().map(|target| {
                        json!({
                            "from": s.id,
                            "to": target,
                        })
                    })
                })
                .collect();

            (
                StatusCode::OK,
                Json(json!({
                    "session_id": session_id,
                    "nodes": nodes,
                    "edges": edges
                })),
            )
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "summary graph failed", "detail": err})),
        ),
    }
}

async fn delete_summary(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((session_id, summary_id)): Path<(String, String)>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if let Err(err) = ensure_session_access(state.as_ref(), &auth, session_id.as_str()).await {
        return err;
    }

    match summaries::delete_summary(&state.pool, session_id.as_str(), summary_id.as_str()).await {
        Ok(true) => (StatusCode::OK, Json(json!({"success": true}))),
        Ok(false) => (StatusCode::NOT_FOUND, Json(json!({"error": "summary not found"}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "delete summary failed", "detail": err})),
        ),
    }
}

#[derive(Debug, Deserialize)]
struct UserIdQuery {
    user_id: Option<String>,
}

async fn list_model_configs(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(q): Query<UserIdQuery>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let user_id = resolve_scope_user_id(&auth, q.user_id);

    match configs::list_model_configs(&state.pool, user_id.as_str()).await {
        Ok(items) => (StatusCode::OK, Json(json!({"items": items}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list model configs failed", "detail": err})),
        ),
    }
}

async fn create_model_config(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(mut req): Json<UpsertAiModelConfigRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };
    req.user_id = resolve_scope_user_id(&auth, Some(req.user_id.clone()));

    let req = match normalize_model_config_request(req) {
        Ok(v) => v,
        Err(err) => return (StatusCode::BAD_REQUEST, Json(json!({"error": err}))),
    };

    match configs::create_model_config(&state.pool, req).await {
        Ok(item) => (StatusCode::OK, Json(json!(item))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "create model config failed", "detail": err})),
        ),
    }
}

async fn update_model_config(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(model_id): Path<String>,
    Json(mut req): Json<UpsertAiModelConfigRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let existing = match configs::get_model_config_by_id(&state.pool, model_id.as_str()).await {
        Ok(Some(cfg)) => cfg,
        Ok(None) => return (StatusCode::NOT_FOUND, Json(json!({"error": "model config not found"}))),
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "load model config failed", "detail": err})),
            )
        }
    };
    if !auth.is_admin() && existing.user_id != auth.user_id {
        return (StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"})));
    }
    req.user_id = existing.user_id;

    let req = match normalize_model_config_request(req) {
        Ok(v) => v,
        Err(err) => return (StatusCode::BAD_REQUEST, Json(json!({"error": err}))),
    };

    match configs::update_model_config(&state.pool, model_id.as_str(), req).await {
        Ok(Some(item)) => (StatusCode::OK, Json(json!(item))),
        Ok(None) => (StatusCode::NOT_FOUND, Json(json!({"error": "model config not found"}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "update model config failed", "detail": err})),
        ),
    }
}

fn normalize_model_config_request(
    mut req: UpsertAiModelConfigRequest,
) -> Result<UpsertAiModelConfigRequest, String> {
    req.provider = normalize_provider_input(req.provider.as_str())?;
    if req.model.trim().is_empty() {
        return Err("model is required".to_string());
    }
    if req.name.trim().is_empty() {
        return Err("name is required".to_string());
    }

    req.thinking_level =
        normalize_thinking_level_input(req.provider.as_str(), req.thinking_level.as_deref())?;

    if let Some(v) = req.temperature {
        req.temperature = Some(v.clamp(0.0, 2.0));
    }

    Ok(req)
}

fn normalize_provider_input(raw: &str) -> Result<String, String> {
    let normalized = raw.trim().to_lowercase();
    let provider = if normalized == "openai" {
        "gpt".to_string()
    } else {
        normalized
    };

    match provider.as_str() {
        "gpt" | "deepseek" | "kimik2" => Ok(provider),
        _ => Err("provider only supports gpt/deepseek/kimik2".to_string()),
    }
}

fn normalize_thinking_level_input(
    provider: &str,
    level: Option<&str>,
) -> Result<Option<String>, String> {
    let level = level
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|v| v.to_lowercase());

    if level.is_none() {
        return Ok(None);
    }

    if provider != "gpt" {
        return Err("thinking_level only works with gpt provider".to_string());
    }

    let level = level.expect("checked");
    let allowed = ["none", "minimal", "low", "medium", "high", "xhigh"];
    if !allowed.contains(&level.as_str()) {
        return Err("thinking_level only supports none/minimal/low/medium/high/xhigh".to_string());
    }

    Ok(Some(level))
}

async fn delete_model_config(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(model_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let existing = match configs::get_model_config_by_id(&state.pool, model_id.as_str()).await {
        Ok(Some(cfg)) => cfg,
        Ok(None) => return (StatusCode::NOT_FOUND, Json(json!({"error": "model config not found"}))),
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "load model config failed", "detail": err})),
            )
        }
    };
    if !auth.is_admin() && existing.user_id != auth.user_id {
        return (StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"})));
    }

    match configs::delete_model_config(&state.pool, model_id.as_str()).await {
        Ok(true) => (StatusCode::OK, Json(json!({"success": true}))),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "model config not found"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "delete model config failed", "detail": err})),
        ),
    }
}

async fn test_model_config(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(model_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };

    let cfg = match configs::get_model_config_by_id(&state.pool, model_id.as_str()).await {
        Ok(Some(cfg)) => cfg,
        Ok(None) => return (StatusCode::NOT_FOUND, Json(json!({"error": "model config not found"}))),
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "load model config failed", "detail": err})),
            )
        }
    };
    if !auth.is_admin() && cfg.user_id != auth.user_id {
        return (StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"})));
    }

    let ai = match AiClient::new(state.config.ai_request_timeout_secs, &state.config) {
        Ok(client) => client,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "init ai client failed", "detail": err})),
            )
        }
    };

    match ai
        .summarize(
            Some(&cfg),
            128,
            "模型连通性测试",
            &["这是一段连通性测试文本，请返回简短摘要。".to_string()],
        )
        .await
    {
        Ok(output) => (StatusCode::OK, Json(json!({"ok": true, "output": output}))),
        Err(err) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({"ok": false, "error": err})),
        ),
    }
}

async fn get_summary_job_config(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(q): Query<UserIdQuery>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let user_id = resolve_scope_user_id(&auth, q.user_id);
    match configs::get_summary_job_config(&state.pool, user_id.as_str()).await {
        Ok(cfg) => (StatusCode::OK, Json(json!(cfg))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "get summary job config failed", "detail": err})),
        ),
    }
}

async fn put_summary_job_config(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(mut req): Json<UpsertSummaryJobConfigRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };
    req.user_id = resolve_scope_user_id(&auth, Some(req.user_id.clone()));

    match configs::upsert_summary_job_config(&state.pool, req).await {
        Ok(cfg) => (StatusCode::OK, Json(json!(cfg))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "save summary job config failed", "detail": err})),
        ),
    }
}

async fn get_summary_rollup_job_config(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(q): Query<UserIdQuery>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let user_id = resolve_scope_user_id(&auth, q.user_id);
    match configs::get_summary_rollup_job_config(&state.pool, user_id.as_str()).await {
        Ok(cfg) => (StatusCode::OK, Json(json!(cfg))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "get summary rollup job config failed", "detail": err})),
        ),
    }
}

async fn put_summary_rollup_job_config(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(mut req): Json<UpsertSummaryRollupJobConfigRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };
    req.user_id = resolve_scope_user_id(&auth, Some(req.user_id.clone()));

    match configs::upsert_summary_rollup_job_config(&state.pool, req).await {
        Ok(cfg) => (StatusCode::OK, Json(json!(cfg))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "save summary rollup job config failed", "detail": err})),
        ),
    }
}

#[derive(Debug, Deserialize)]
struct RunJobRequest {
    user_id: Option<String>,
    session_id: Option<String>,
}

async fn run_summary_once(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<RunJobRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let scope_user_id = resolve_scope_user_id(&auth, req.user_id);
    let ai = match AiClient::new(state.config.ai_request_timeout_secs, &state.config) {
        Ok(client) => client,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "init ai client failed", "detail": err})),
            )
        }
    };

    let result = if let Some(session_id) = req.session_id.as_deref() {
        if let Err(err) = ensure_session_access(state.as_ref(), &auth, session_id).await {
            return err;
        }
        jobs::summary::run_once_for_session(&state.pool, &ai, scope_user_id.as_str(), session_id)
            .await
            .map(|_| json!({"session_id": session_id, "done": true}))
    } else {
        jobs::summary::run_once(&state.pool, &ai, scope_user_id.as_str())
            .await
            .map(|r| json!(r))
    };

    match result {
        Ok(data) => (StatusCode::OK, Json(json!({"ok": true, "data": data}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"ok": false, "error": err})),
        ),
    }
}

async fn run_rollup_once(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<RunJobRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let scope_user_id = resolve_scope_user_id(&auth, req.user_id);
    let ai = match AiClient::new(state.config.ai_request_timeout_secs, &state.config) {
        Ok(client) => client,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "init ai client failed", "detail": err})),
            )
        }
    };

    match jobs::rollup::run_once(&state.pool, &ai, scope_user_id.as_str()).await {
        Ok(data) => (StatusCode::OK, Json(json!({"ok": true, "data": data}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"ok": false, "error": err})),
        ),
    }
}

#[derive(Debug, Deserialize)]
struct JobRunsQuery {
    job_type: Option<String>,
    session_id: Option<String>,
    status: Option<String>,
    limit: Option<i64>,
}

async fn list_job_runs(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(q): Query<JobRunsQuery>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if !auth.is_admin() {
        return (StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"})));
    }

    match job_repo::list_job_runs(
        &state.pool,
        q.job_type.as_deref(),
        q.session_id.as_deref(),
        q.status.as_deref(),
        q.limit.unwrap_or(100),
    )
    .await
    {
        Ok(items) => (StatusCode::OK, Json(json!({"items": items}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list job runs failed", "detail": err})),
        ),
    }
}

async fn job_stats(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if !auth.is_admin() {
        return (StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"})));
    }

    match job_repo::job_stats(&state.pool).await {
        Ok(stats) => (StatusCode::OK, Json(json!({"stats": stats}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "job stats failed", "detail": err})),
        ),
    }
}

async fn compose_context(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<ComposeContextRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if let Err(err) = ensure_session_access(state.as_ref(), &auth, req.session_id.as_str()).await {
        return err;
    }

    match context::compose_context(&state.pool, req).await {
        Ok(ctx) => (StatusCode::OK, Json(json!(ctx))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "compose context failed", "detail": err})),
        ),
    }
}
