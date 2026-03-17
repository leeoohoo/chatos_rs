use std::collections::HashSet;
use std::fs;
use std::path::{Path as FsPath, PathBuf};
use std::process::Command;
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
    BatchCreateMessagesRequest, ComposeContextRequest, CreateContactRequest,
    CreateMemoryAgentRequest, CreateMessageRequest, CreateSessionRequest, MemoryAgentSkill,
    MemorySkill, MemorySkillPlugin, UpdateMemoryAgentRequest, UpdateSessionRequest,
    UpsertAgentMemoryJobConfigRequest, UpsertAiModelConfigRequest, UpsertSummaryJobConfigRequest,
    UpsertSummaryRollupJobConfigRequest,
};
use crate::repositories::{
    agents as agents_repo, auth as auth_repo, configs, contacts as contacts_repo, jobs as job_repo,
    memories as memories_repo, messages, sessions, skills as skills_repo, summaries,
};
use crate::services::context;
use crate::state::AppState;

pub type SharedState = Arc<AppState>;

pub fn router(state: SharedState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/memory/v1/auth/login", post(login))
        .route("/api/memory/v1/auth/me", get(me))
        .route(
            "/api/memory/v1/auth/users",
            get(list_users).post(create_user),
        )
        .route(
            "/api/memory/v1/auth/users/:username",
            patch(update_user).delete(delete_user),
        )
        .route(
            "/api/memory/v1/sessions",
            post(create_session).get(list_sessions),
        )
        .route(
            "/api/memory/v1/sessions/:session_id/sync",
            put(sync_session),
        )
        .route(
            "/api/memory/v1/sessions/:session_id",
            get(get_session)
                .patch(update_session)
                .delete(delete_session),
        )
        .route(
            "/api/memory/v1/sessions/:session_id/messages",
            post(create_message)
                .get(list_messages)
                .delete(clear_session_messages),
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
            "/api/memory/v1/contacts",
            get(list_contacts).post(create_contact),
        )
        .route(
            "/api/memory/v1/contacts/:contact_id",
            delete(delete_contact),
        )
        .route(
            "/api/memory/v1/contacts/:contact_id/project-memories",
            get(list_contact_project_memories),
        )
        .route(
            "/api/memory/v1/contacts/:contact_id/project-memories/:project_id",
            get(list_contact_project_memories_by_project),
        )
        .route(
            "/api/memory/v1/contacts/:contact_id/agent-recalls",
            get(list_contact_agent_recalls),
        )
        .route("/api/memory/v1/agents", get(list_agents).post(create_agent))
        .route("/api/memory/v1/skills", get(list_skills))
        .route("/api/memory/v1/skills/plugins", get(list_skill_plugins))
        .route(
            "/api/memory/v1/skills/import-git",
            post(import_skills_from_git),
        )
        .route(
            "/api/memory/v1/skills/plugins/install",
            post(install_skill_plugins),
        )
        .route("/api/memory/v1/agents/ai-create", post(ai_create_agent))
        .route(
            "/api/memory/v1/agents/:agent_id/runtime-context",
            get(get_agent_runtime_context),
        )
        .route(
            "/api/memory/v1/agents/:agent_id",
            get(get_agent).patch(update_agent).delete(delete_agent),
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
            "/api/memory/v1/configs/agent-memory-job",
            get(get_agent_memory_job_config).put(put_agent_memory_job_config),
        )
        .route(
            "/api/memory/v1/jobs/summary/run-once",
            post(run_summary_once),
        )
        .route(
            "/api/memory/v1/jobs/summary-rollup/run-once",
            post(run_rollup_once),
        )
        .route(
            "/api/memory/v1/jobs/agent-memory/run-once",
            post(run_agent_memory_once),
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

    let user =
        match auth_repo::verify_user_password(&state.pool, username.as_str(), password.as_str())
            .await
        {
            Ok(Some(user)) => user,
            Ok(None) => {
                return (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({"error": "invalid credentials"})),
                )
            }
            Err(err) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": "login failed", "detail": err})),
                )
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
            "username": user.user_id,
            "role": user.role
        })),
    )
}

async fn me(State(state): State<SharedState>, headers: HeaderMap) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };

    (
        StatusCode::OK,
        Json(json!({
            "username": auth.user_id,
            "role": auth.role
        })),
    )
}

#[derive(Debug, Deserialize)]
struct ListUsersQuery {
    limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct CreateUserRequest {
    username: String,
    password: String,
    role: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UpdateUserRequest {
    password: Option<String>,
    role: Option<String>,
}

fn auth_user_json(user: &auth_repo::AuthUser) -> Value {
    json!({
        "username": user.user_id,
        "role": user.role,
        "created_at": user.created_at,
        "updated_at": user.updated_at,
    })
}

async fn list_users(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(q): Query<ListUsersQuery>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };

    if auth.is_admin() {
        let limit = q.limit.unwrap_or(500).max(1);
        return match auth_repo::list_users(&state.pool, limit).await {
            Ok(items) => (
                StatusCode::OK,
                Json(json!({
                    "items": items
                        .into_iter()
                        .map(|u| auth_user_json(&u))
                        .collect::<Vec<Value>>()
                })),
            ),
            Err(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "list users failed", "detail": err})),
            ),
        };
    }

    match auth_repo::get_user_by_id(&state.pool, auth.user_id.as_str()).await {
        Ok(Some(user)) => (
            StatusCode::OK,
            Json(json!({ "items": [auth_user_json(&user)] })),
        ),
        Ok(None) => (StatusCode::OK, Json(json!({"items": []}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "load user failed", "detail": err})),
        ),
    }
}

fn normalize_role_input(role: Option<&str>) -> Result<String, String> {
    let role = role
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or(auth_repo::USER_ROLE)
        .to_lowercase();

    if role == auth_repo::ADMIN_ROLE || role == auth_repo::USER_ROLE {
        Ok(role)
    } else {
        Err("role only supports admin/user".to_string())
    }
}

async fn create_user(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<CreateUserRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if !auth.is_admin() {
        return (StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"})));
    }

    let username = req.username.trim().to_string();
    let password = req.password.trim().to_string();
    if username.is_empty() || password.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "username/password required"})),
        );
    }

    let mut role = match normalize_role_input(req.role.as_deref()) {
        Ok(v) => v,
        Err(err) => return (StatusCode::BAD_REQUEST, Json(json!({"error": err}))),
    };
    if username == auth_repo::ADMIN_USER_ID {
        role = auth_repo::ADMIN_ROLE.to_string();
    }

    match auth_repo::get_user_by_id(&state.pool, username.as_str()).await {
        Ok(Some(_)) => {
            return (
                StatusCode::CONFLICT,
                Json(json!({"error": "user already exists"})),
            )
        }
        Ok(None) => {}
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "load user failed", "detail": err})),
            )
        }
    }

    match auth_repo::create_user(
        &state.pool,
        username.as_str(),
        password.as_str(),
        role.as_str(),
    )
    .await
    {
        Ok(user) => (StatusCode::OK, Json(auth_user_json(&user))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "create user failed", "detail": err})),
        ),
    }
}

async fn update_user(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(username): Path<String>,
    Json(req): Json<UpdateUserRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if !auth.is_admin() {
        return (StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"})));
    }

    let target_username = username.trim().to_string();
    if target_username.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "username required"})),
        );
    }

    let password = req
        .password
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let mut role = match req.role.as_ref() {
        Some(v) => match normalize_role_input(Some(v.as_str())) {
            Ok(role) => Some(role),
            Err(err) => return (StatusCode::BAD_REQUEST, Json(json!({"error": err}))),
        },
        None => None,
    };

    if password.is_none() && role.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "nothing to update"})),
        );
    }

    if target_username == auth_repo::ADMIN_USER_ID
        && role.as_deref() != Some(auth_repo::ADMIN_ROLE)
        && role.is_some()
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "admin role cannot be changed"})),
        );
    }
    if target_username == auth_repo::ADMIN_USER_ID {
        role = Some(auth_repo::ADMIN_ROLE.to_string());
    }

    match auth_repo::update_user(
        &state.pool,
        target_username.as_str(),
        password.as_deref(),
        role.as_deref(),
    )
    .await
    {
        Ok(Some(user)) => (StatusCode::OK, Json(auth_user_json(&user))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "user not found"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "update user failed", "detail": err})),
        ),
    }
}

async fn delete_user(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(username): Path<String>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if !auth.is_admin() {
        return (StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"})));
    }

    let target_username = username.trim().to_string();
    if target_username.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "username required"})),
        );
    }
    if target_username == auth_repo::ADMIN_USER_ID {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "admin user cannot be deleted"})),
        );
    }
    if target_username == auth.user_id {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "cannot delete current login user"})),
        );
    }

    if let Err(err) = configs::delete_user_configs(&state.pool, target_username.as_str()).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "delete user configs failed", "detail": err})),
        );
    }

    match auth_repo::delete_user(&state.pool, target_username.as_str()).await {
        Ok(true) => (StatusCode::OK, Json(json!({"success": true}))),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "user not found"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "delete user failed", "detail": err})),
        ),
    }
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
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "unauthorized"})),
        ));
    };

    let parsed =
        parse_auth_token(token.as_str(), state.config.auth_secret.as_str()).ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "unauthorized"})),
            )
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

fn resolve_visible_user_ids(scope_user_id: &str) -> Vec<String> {
    let normalized = scope_user_id.trim();
    if normalized.is_empty() || normalized == auth_repo::ADMIN_USER_ID {
        return vec![auth_repo::ADMIN_USER_ID.to_string()];
    }
    vec![normalized.to_string(), auth_repo::ADMIN_USER_ID.to_string()]
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
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "session not found"})),
        )),
        Err(err) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "load session failed", "detail": err})),
        )),
    }
}

async fn ensure_contact_access(
    state: &AppState,
    auth: &AuthIdentity,
    contact_id: &str,
) -> Result<crate::models::Contact, (StatusCode, Json<Value>)> {
    match contacts_repo::get_contact_by_id(&state.pool, contact_id).await {
        Ok(Some(contact)) => {
            if auth.is_admin() || contact.user_id == auth.user_id {
                Ok(contact)
            } else {
                Err((StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"}))))
            }
        }
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "contact not found"})),
        )),
        Err(err) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "load contact failed", "detail": err})),
        )),
    }
}

async fn ensure_agent_read_access(
    state: &AppState,
    auth: &AuthIdentity,
    agent_id: &str,
) -> Result<crate::models::MemoryAgent, (StatusCode, Json<Value>)> {
    match agents_repo::get_agent_by_id(&state.pool, agent_id).await {
        Ok(Some(agent)) => {
            if auth.is_admin()
                || agent.user_id == auth.user_id
                || agent.user_id == auth_repo::ADMIN_USER_ID
            {
                Ok(agent)
            } else {
                Err((StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"}))))
            }
        }
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "agent not found"})),
        )),
        Err(err) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "load agent failed", "detail": err})),
        )),
    }
}

async fn ensure_agent_manage_access(
    state: &AppState,
    auth: &AuthIdentity,
    agent_id: &str,
) -> Result<crate::models::MemoryAgent, (StatusCode, Json<Value>)> {
    match agents_repo::get_agent_by_id(&state.pool, agent_id).await {
        Ok(Some(agent)) => {
            if auth.is_admin() || agent.user_id == auth.user_id {
                Ok(agent)
            } else {
                Err((StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"}))))
            }
        }
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "agent not found"})),
        )),
        Err(err) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "load agent failed", "detail": err})),
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
    metadata: Option<Value>,
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
        req.metadata,
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
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "session not found"})),
        ),
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
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "session not found"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "update session failed", "detail": err})),
        ),
    }
}

#[derive(Debug, Deserialize)]
struct ListContactsQuery {
    user_id: Option<String>,
    status: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct CreateContactPayload {
    user_id: Option<String>,
    agent_id: String,
    agent_name_snapshot: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ListContactMemoriesQuery {
    project_id: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct ListAgentsQuery {
    user_id: Option<String>,
    enabled: Option<bool>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct CreateAgentRequest {
    user_id: Option<String>,
    name: String,
    description: Option<String>,
    category: Option<String>,
    role_definition: String,
    skills: Option<Vec<MemoryAgentSkill>>,
    skill_ids: Option<Vec<String>>,
    default_skill_ids: Option<Vec<String>>,
    mcp_policy: Option<Value>,
    project_policy: Option<Value>,
    enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct ListSkillsQuery {
    user_id: Option<String>,
    plugin_source: Option<String>,
    query: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct ListSkillPluginsQuery {
    user_id: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct ImportSkillsFromGitRequest {
    user_id: Option<String>,
    repository: String,
    branch: Option<String>,
    marketplace_path: Option<String>,
    plugins_path: Option<String>,
    auto_install: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct InstallSkillPluginsRequest {
    user_id: Option<String>,
    source: Option<String>,
    install_all: Option<bool>,
}

#[derive(Debug, Clone)]
struct SkillPluginCandidate {
    source: String,
    name: String,
    category: Option<String>,
    description: Option<String>,
    version: Option<String>,
}

async fn list_contacts(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(q): Query<ListContactsQuery>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };

    let scope_user_id = resolve_scope_user_id(&auth, q.user_id);
    let limit = q.limit.unwrap_or(200);
    let offset = q.offset.unwrap_or(0);
    let status = q
        .status
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("active");

    match contacts_repo::list_contacts(
        &state.pool,
        scope_user_id.as_str(),
        Some(status),
        limit,
        offset,
    )
    .await
    {
        Ok(items) => (StatusCode::OK, Json(json!({"items": items}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list contacts failed", "detail": err})),
        ),
    }
}

async fn create_contact(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<CreateContactPayload>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };

    let scope_user_id = resolve_scope_user_id(&auth, req.user_id);
    let agent_id = req.agent_id.trim().to_string();
    if agent_id.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "agent_id is required"})),
        );
    }

    let agent = match ensure_agent_read_access(state.as_ref(), &auth, agent_id.as_str()).await {
        Ok(agent) => agent,
        Err(err) => return err,
    };
    if !agent.enabled {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "agent is disabled"})),
        );
    }

    let create_req = CreateContactRequest {
        user_id: scope_user_id,
        agent_id,
        agent_name_snapshot: req
            .agent_name_snapshot
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| Some(agent.name)),
    };

    match contacts_repo::create_contact_idempotent(&state.pool, create_req).await {
        Ok((contact, created)) => {
            let status = if created {
                StatusCode::CREATED
            } else {
                StatusCode::OK
            };
            (
                status,
                Json(json!({"created": created, "contact": contact})),
            )
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "create contact failed", "detail": err})),
        ),
    }
}

async fn delete_contact(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(contact_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };

    let contact = match ensure_contact_access(state.as_ref(), &auth, contact_id.as_str()).await {
        Ok(contact) => contact,
        Err(err) => return err,
    };

    if let Err(err) = sessions::archive_sessions_by_contact(
        &state.pool,
        contact.user_id.as_str(),
        contact.id.as_str(),
        contact.agent_id.as_str(),
    )
    .await
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "archive contact sessions failed", "detail": err})),
        );
    }

    match contacts_repo::delete_contact_by_id(&state.pool, contact_id.as_str()).await {
        Ok(true) => (StatusCode::OK, Json(json!({"success": true}))),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "contact not found"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "delete contact failed", "detail": err})),
        ),
    }
}

async fn list_contact_project_memories(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(contact_id): Path<String>,
    Query(q): Query<ListContactMemoriesQuery>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let contact = match ensure_contact_access(state.as_ref(), &auth, contact_id.as_str()).await {
        Ok(contact) => contact,
        Err(err) => return err,
    };

    let limit = q.limit.unwrap_or(200);
    let offset = q.offset.unwrap_or(0);
    let target_project_id = q
        .project_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    let list_result = match target_project_id {
        Some(project_id) => {
            memories_repo::list_project_memories(
                &state.pool,
                contact.user_id.as_str(),
                contact.id.as_str(),
                project_id.as_str(),
                limit,
                offset,
            )
            .await
        }
        None => {
            memories_repo::list_project_memories_by_contact(
                &state.pool,
                contact.user_id.as_str(),
                contact.id.as_str(),
                limit,
                offset,
            )
            .await
        }
    };

    match list_result {
        Ok(items) => (StatusCode::OK, Json(json!({"items": items}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list project memories failed", "detail": err})),
        ),
    }
}

async fn list_contact_project_memories_by_project(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((contact_id, project_id)): Path<(String, String)>,
    Query(q): Query<ListContactMemoriesQuery>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let contact = match ensure_contact_access(state.as_ref(), &auth, contact_id.as_str()).await {
        Ok(contact) => contact,
        Err(err) => return err,
    };

    let project_id = project_id.trim().to_string();
    if project_id.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "project_id is required"})),
        );
    }

    let limit = q.limit.unwrap_or(200);
    let offset = q.offset.unwrap_or(0);
    match memories_repo::list_project_memories(
        &state.pool,
        contact.user_id.as_str(),
        contact.id.as_str(),
        project_id.as_str(),
        limit,
        offset,
    )
    .await
    {
        Ok(items) => (StatusCode::OK, Json(json!({"items": items}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list project memories failed", "detail": err})),
        ),
    }
}

async fn list_contact_agent_recalls(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(contact_id): Path<String>,
    Query(q): Query<ListContactMemoriesQuery>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let contact = match ensure_contact_access(state.as_ref(), &auth, contact_id.as_str()).await {
        Ok(contact) => contact,
        Err(err) => return err,
    };

    let limit = q.limit.unwrap_or(200);
    let offset = q.offset.unwrap_or(0);
    match memories_repo::list_agent_recalls(
        &state.pool,
        contact.user_id.as_str(),
        contact.agent_id.as_str(),
        limit,
        offset,
    )
    .await
    {
        Ok(items) => (StatusCode::OK, Json(json!({"items": items}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list agent recalls failed", "detail": err})),
        ),
    }
}

async fn list_skills(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(q): Query<ListSkillsQuery>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };

    let scope_user_id = resolve_scope_user_id(&auth, q.user_id);
    let visible_user_ids = resolve_visible_user_ids(scope_user_id.as_str());
    let limit = q.limit.unwrap_or(200);
    let offset = q.offset.unwrap_or(0);
    let plugin_source = q
        .plugin_source
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let query = q
        .query
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    match skills_repo::list_skills(
        &state.pool,
        visible_user_ids.as_slice(),
        plugin_source,
        query,
        limit,
        offset,
    )
    .await
    {
        Ok(items) => (StatusCode::OK, Json(json!({"items": items}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list skills failed", "detail": err})),
        ),
    }
}

async fn list_skill_plugins(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(q): Query<ListSkillPluginsQuery>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };

    let scope_user_id = resolve_scope_user_id(&auth, q.user_id);
    let limit = q.limit.unwrap_or(200);
    let offset = q.offset.unwrap_or(0);

    match skills_repo::list_plugins(&state.pool, scope_user_id.as_str(), limit, offset).await {
        Ok(items) => (StatusCode::OK, Json(json!({"items": items}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list skill plugins failed", "detail": err})),
        ),
    }
}

async fn import_skills_from_git(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<ImportSkillsFromGitRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };

    let repository = req.repository.trim().to_string();
    if repository.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "repository is required"})),
        );
    }

    let scope_user_id = resolve_scope_user_id(&auth, req.user_id);
    let state_root = resolve_skill_state_root(scope_user_id.as_str());
    let plugins_root = state_root.join("plugins");
    let git_cache_root = state_root.join("git-cache");

    if let Err(err) = ensure_dir(plugins_root.as_path()) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "prepare plugin cache failed", "detail": err})),
        );
    }
    if let Err(err) = ensure_dir(git_cache_root.as_path()) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "prepare git cache failed", "detail": err})),
        );
    }

    let branch = req
        .branch
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    let repo_root = match ensure_git_repo(
        repository.as_str(),
        branch.as_deref(),
        git_cache_root.as_path(),
    ) {
        Ok(path) => path,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "git import failed", "detail": err})),
            )
        }
    };

    let candidates = match load_plugin_candidates_from_repo(
        repo_root.as_path(),
        req.marketplace_path.as_deref(),
        req.plugins_path.as_deref(),
    ) {
        Ok(items) if !items.is_empty() => items,
        Ok(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "no plugins discovered from repository"})),
            )
        }
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "parse plugin definitions failed", "detail": err})),
            )
        }
    };

    let sources = candidates
        .iter()
        .map(|item| item.source.clone())
        .collect::<Vec<_>>();
    let existing =
        skills_repo::get_plugins_by_sources(&state.pool, scope_user_id.as_str(), &sources)
            .await
            .unwrap_or_default();
    let existing_by_source = existing
        .into_iter()
        .map(|item| (item.source.clone(), item))
        .collect::<std::collections::HashMap<_, _>>();

    let mut imported_sources = Vec::new();
    let mut details = Vec::new();
    for candidate in candidates {
        let cache_rel = match copy_plugin_source_from_repo(
            repo_root.as_path(),
            plugins_root.as_path(),
            candidate.source.as_str(),
        ) {
            Ok(value) => value,
            Err(err) => {
                details.push(json!({
                    "source": candidate.source,
                    "ok": false,
                    "error": err
                }));
                continue;
            }
        };

        let plugin_root = plugins_root.join(cache_rel.as_str());
        let discoverable_skills = discover_skill_entries(plugin_root.as_path()).len() as i64;
        let previous = existing_by_source.get(candidate.source.as_str());
        let plugin = MemorySkillPlugin {
            id: previous.map(|item| item.id.clone()).unwrap_or_else(|| {
                hash_id(&["plugin", scope_user_id.as_str(), candidate.source.as_str()])
            }),
            user_id: scope_user_id.clone(),
            source: candidate.source.clone(),
            name: candidate.name.clone(),
            category: candidate.category.clone(),
            description: candidate.description.clone(),
            version: candidate.version.clone(),
            repository: Some(repository.clone()),
            branch: branch.clone(),
            cache_path: Some(cache_rel.clone()),
            installed: previous.map(|item| item.installed).unwrap_or(false),
            discoverable_skills,
            installed_skill_count: previous.map(|item| item.installed_skill_count).unwrap_or(0),
            updated_at: crate::repositories::now_rfc3339(),
        };

        match skills_repo::upsert_plugin(&state.pool, plugin).await {
            Ok(saved) => {
                imported_sources.push(saved.source.clone());
                details.push(json!({
                    "source": saved.source,
                    "name": saved.name,
                    "discoverable_skills": saved.discoverable_skills,
                    "installed": saved.installed,
                    "cache_path": saved.cache_path,
                    "ok": true
                }));
            }
            Err(err) => {
                details.push(json!({
                    "source": candidate.source,
                    "ok": false,
                    "error": err
                }));
            }
        }
    }

    if imported_sources.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "no plugin imported", "details": details})),
        );
    }

    let auto_install = req.auto_install.unwrap_or(false);
    let install_result = if auto_install {
        match install_skill_plugins_internal(
            state.as_ref(),
            scope_user_id.as_str(),
            imported_sources.as_slice(),
        )
        .await
        {
            Ok(value) => Some(value),
            Err(err) => Some(json!({"ok": false, "error": err})),
        }
    } else {
        None
    };

    (
        StatusCode::OK,
        Json(json!({
            "ok": true,
            "repository": repository,
            "branch": branch,
            "imported_sources": imported_sources,
            "details": details,
            "auto_install": auto_install,
            "install_result": install_result
        })),
    )
}

async fn install_skill_plugins(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<InstallSkillPluginsRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };

    let scope_user_id = resolve_scope_user_id(&auth, req.user_id);
    let install_all = req.install_all.unwrap_or(false);
    let source = req
        .source
        .as_deref()
        .map(normalize_plugin_source)
        .filter(|value| !value.is_empty());

    let target_sources = if install_all {
        match skills_repo::list_plugins(&state.pool, scope_user_id.as_str(), 500, 0).await {
            Ok(items) => items
                .into_iter()
                .map(|item| item.source)
                .collect::<Vec<_>>(),
            Err(err) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": "load plugins failed", "detail": err})),
                )
            }
        }
    } else if let Some(value) = source {
        vec![value]
    } else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "source is required when install_all=false"})),
        );
    };

    match install_skill_plugins_internal(state.as_ref(), scope_user_id.as_str(), &target_sources)
        .await
    {
        Ok(value) => (StatusCode::OK, Json(value)),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "install plugins failed", "detail": err})),
        ),
    }
}

async fn install_skill_plugins_internal(
    state: &AppState,
    user_id: &str,
    sources: &[String],
) -> Result<Value, String> {
    let normalized_sources = unique_strings(
        sources
            .iter()
            .map(|item| normalize_plugin_source(item.as_str()))
            .filter(|item| !item.is_empty())
            .collect::<Vec<_>>(),
    );
    if normalized_sources.is_empty() {
        return Err("no plugin sources specified".to_string());
    }

    let plugins =
        skills_repo::get_plugins_by_sources(&state.pool, user_id, &normalized_sources).await?;
    if plugins.is_empty() {
        return Err("plugins not found".to_string());
    }

    let state_root = resolve_skill_state_root(user_id);
    let plugins_root = state_root.join("plugins");

    let mut installed = 0usize;
    let mut skipped = 0usize;
    let mut details = Vec::new();

    for plugin in plugins {
        let Some(plugin_root) = resolve_plugin_root_from_cache(
            plugins_root.as_path(),
            plugin.cache_path.as_deref(),
            plugin.source.as_str(),
        ) else {
            skipped += 1;
            details.push(json!({
                "source": plugin.source,
                "ok": false,
                "reason": "cached plugin path not found"
            }));
            continue;
        };

        let entries = discover_skill_entries(plugin_root.as_path());
        if entries.is_empty() {
            let _ = skills_repo::replace_skills_for_plugin(
                &state.pool,
                user_id,
                plugin.source.as_str(),
                Vec::new(),
            )
            .await;
            let _ = skills_repo::update_plugin_install_state(
                &state.pool,
                user_id,
                plugin.source.as_str(),
                0,
                0,
            )
            .await;
            skipped += 1;
            details.push(json!({
                "source": plugin.source,
                "ok": false,
                "reason": "no skills discovered in plugin"
            }));
            continue;
        }

        let mut skills = Vec::new();
        for entry in entries {
            let Some(file_path) =
                normalize_skill_entry_to_file(plugin_root.as_path(), entry.as_str())
            else {
                continue;
            };
            let raw = match fs::read_to_string(file_path.as_path()) {
                Ok(value) => value,
                Err(_) => continue,
            };
            let content = raw.trim().to_string();
            if content.is_empty() {
                continue;
            }
            let id = hash_id(&["skill", user_id, plugin.source.as_str(), entry.as_str()]);
            let skill = MemorySkill {
                id,
                user_id: user_id.to_string(),
                plugin_source: plugin.source.clone(),
                name: build_skill_name_from_entry(entry.as_str()),
                description: None,
                content,
                source_path: entry,
                version: plugin.version.clone(),
                updated_at: crate::repositories::now_rfc3339(),
            };
            skills.push(skill);
        }

        let installed_count = skills_repo::replace_skills_for_plugin(
            &state.pool,
            user_id,
            plugin.source.as_str(),
            skills,
        )
        .await?;
        let _ = skills_repo::update_plugin_install_state(
            &state.pool,
            user_id,
            plugin.source.as_str(),
            installed_count as i64,
            entries_len_to_i64(&discover_skill_entries(plugin_root.as_path())),
        )
        .await?;

        installed += 1;
        details.push(json!({
            "source": plugin.source,
            "ok": true,
            "installed_skills": installed_count
        }));
    }

    Ok(json!({
        "ok": true,
        "installed_plugins": installed,
        "skipped_plugins": skipped,
        "details": details
    }))
}

fn entries_len_to_i64(entries: &[String]) -> i64 {
    entries.len().min(i64::MAX as usize) as i64
}

async fn list_agents(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(q): Query<ListAgentsQuery>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };

    let scope_user_id = resolve_scope_user_id(&auth, q.user_id);
    let visible_user_ids = resolve_visible_user_ids(scope_user_id.as_str());
    let limit = q.limit.unwrap_or(100);
    let offset = q.offset.unwrap_or(0);

    match agents_repo::list_agents(
        &state.pool,
        visible_user_ids.as_slice(),
        q.enabled,
        limit,
        offset,
    )
    .await
    {
        Ok(items) => (StatusCode::OK, Json(json!({"items": items}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list agents failed", "detail": err})),
        ),
    }
}

async fn create_agent(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<CreateAgentRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };

    let name = req.name.trim().to_string();
    if name.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "name is required"})),
        );
    }

    let role_definition = req.role_definition.trim().to_string();
    if role_definition.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "role_definition is required"})),
        );
    }

    let scope_user_id = resolve_scope_user_id(&auth, req.user_id);
    let create_req = CreateMemoryAgentRequest {
        user_id: scope_user_id,
        name,
        description: req.description,
        category: req.category,
        role_definition,
        skills: req.skills,
        skill_ids: req.skill_ids,
        default_skill_ids: req.default_skill_ids,
        mcp_policy: req.mcp_policy,
        project_policy: req.project_policy,
        enabled: req.enabled,
    };

    match agents_repo::create_agent(&state.pool, create_req).await {
        Ok(agent) => (StatusCode::OK, Json(json!(agent))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "create agent failed", "detail": err})),
        ),
    }
}

async fn get_agent(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };

    match ensure_agent_read_access(state.as_ref(), &auth, agent_id.as_str()).await {
        Ok(agent) => (StatusCode::OK, Json(json!(agent))),
        Err(err) => err,
    }
}

async fn update_agent(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
    Json(req): Json<UpdateMemoryAgentRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if let Err(err) = ensure_agent_manage_access(state.as_ref(), &auth, agent_id.as_str()).await {
        return err;
    }

    match agents_repo::update_agent(&state.pool, agent_id.as_str(), req).await {
        Ok(Some(agent)) => (StatusCode::OK, Json(json!(agent))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "agent not found"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "update agent failed", "detail": err})),
        ),
    }
}

async fn delete_agent(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if let Err(err) = ensure_agent_manage_access(state.as_ref(), &auth, agent_id.as_str()).await {
        return err;
    }

    match agents_repo::delete_agent(&state.pool, agent_id.as_str()).await {
        Ok(true) => (StatusCode::OK, Json(json!({"success": true}))),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "agent not found"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "delete agent failed", "detail": err})),
        ),
    }
}

async fn get_agent_runtime_context(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if let Err(err) = ensure_agent_read_access(state.as_ref(), &auth, agent_id.as_str()).await {
        return err;
    }

    match agents_repo::get_runtime_context(&state.pool, agent_id.as_str()).await {
        Ok(Some(context)) => (StatusCode::OK, Json(json!(context))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "agent not found"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "load runtime context failed", "detail": err})),
        ),
    }
}

async fn ai_create_agent(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<Value>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };

    let requested_user_id = req
        .get("user_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let scope_user_id = resolve_scope_user_id(&auth, requested_user_id);

    let requirement = req
        .get("requirement")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let Some(requirement) = requirement else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "requirement is required"})),
        );
    };

    let name = req
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| default_agent_name(&requirement));
    let category = req
        .get("category")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| Some(infer_agent_category(&requirement).to_string()));
    let description = req
        .get("description")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            Some(format!(
                "根据需求“{}”生成的智能体。",
                truncate_text(&requirement, 120)
            ))
        });
    let role_definition = req
        .get("role_definition")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| default_role_definition(name.as_str(), requirement.as_str()));

    let skill_ids = req
        .get("skill_ids")
        .and_then(parse_string_array)
        .unwrap_or_else(|| default_skill_ids(&requirement));
    let default_skill_ids = req
        .get("default_skill_ids")
        .and_then(parse_string_array)
        .unwrap_or_else(|| skill_ids.clone());
    let skills = parse_skill_prompts(req.get("skill_prompts"))
        .or_else(|| req.get("skills").and_then(parse_skill_objects));
    let enabled = req.get("enabled").and_then(Value::as_bool).unwrap_or(true);

    let mcp_enabled = req
        .get("mcp_enabled")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let enabled_mcp_ids = req
        .get("enabled_mcp_ids")
        .and_then(parse_string_array)
        .unwrap_or_default();
    let project_id = req
        .get("project_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let project_root = req
        .get("project_root")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let mcp_policy = Some(json!({
        "enabled": mcp_enabled,
        "enabled_mcp_ids": enabled_mcp_ids,
    }));
    let project_policy = if project_id.is_some() || project_root.is_some() {
        Some(json!({
            "project_id": project_id,
            "project_root": project_root,
        }))
    } else {
        None
    };

    let create_req = CreateMemoryAgentRequest {
        user_id: scope_user_id,
        name,
        description,
        category,
        role_definition,
        skills,
        skill_ids: Some(skill_ids),
        default_skill_ids: Some(default_skill_ids),
        mcp_policy,
        project_policy,
        enabled: Some(enabled),
    };

    match agents_repo::create_agent(&state.pool, create_req).await {
        Ok(agent) => (
            StatusCode::OK,
            Json(json!({
                "created": true,
                "agent": agent,
                "source": "rule_based_builder"
            })),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "ai-create failed", "detail": err})),
        ),
    }
}

fn parse_string_array(value: &Value) -> Option<Vec<String>> {
    let items = value.as_array()?;
    let mut out = Vec::new();
    for item in items {
        let Some(raw) = item.as_str() else {
            continue;
        };
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        if out.iter().any(|existing: &String| existing == trimmed) {
            continue;
        }
        out.push(trimmed.to_string());
    }
    Some(out)
}

fn parse_skill_objects(value: &Value) -> Option<Vec<MemoryAgentSkill>> {
    let items = value.as_array()?;
    let mut out = Vec::new();
    for item in items {
        let Some(obj) = item.as_object() else {
            continue;
        };
        let id = obj
            .get("id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let name = obj
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let content = obj
            .get("content")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let (Some(id), Some(name), Some(content)) = (id, name, content) else {
            continue;
        };
        out.push(MemoryAgentSkill { id, name, content });
    }
    Some(out)
}

fn parse_skill_prompts(value: Option<&Value>) -> Option<Vec<MemoryAgentSkill>> {
    let prompts = value?.as_array()?;
    let mut out = Vec::new();
    for (index, item) in prompts.iter().enumerate() {
        let Some(prompt) = item.as_str() else {
            continue;
        };
        let trimmed = prompt.trim();
        if trimmed.is_empty() {
            continue;
        }
        let skill_id = format!("skill_{}", index + 1);
        out.push(MemoryAgentSkill {
            id: skill_id.clone(),
            name: format!("Skill {}", index + 1),
            content: trimmed.to_string(),
        });
    }
    Some(out)
}

fn contains_any(text: &str, patterns: &[&str]) -> bool {
    let lowered = text.to_ascii_lowercase();
    patterns
        .iter()
        .any(|pattern| lowered.contains(pattern.to_ascii_lowercase().as_str()))
}

fn infer_agent_category(requirement: &str) -> &'static str {
    if contains_any(requirement, &["代码", "开发", "编程", "debug", "code"]) {
        "engineering"
    } else if contains_any(requirement, &["产品", "需求", "roadmap", "prd"]) {
        "product"
    } else if contains_any(requirement, &["运营", "增长", "营销", "campaign"]) {
        "growth"
    } else {
        "general"
    }
}

fn default_agent_name(requirement: &str) -> String {
    let category = infer_agent_category(requirement);
    match category {
        "engineering" => "研发协作助手".to_string(),
        "product" => "产品分析助手".to_string(),
        "growth" => "增长运营助手".to_string(),
        _ => "通用业务助手".to_string(),
    }
}

fn default_role_definition(name: &str, requirement: &str) -> String {
    format!(
        "你是{name}。你的目标是围绕“{}”为用户提供清晰、可执行、可验证的行动建议，并在信息不足时优先澄清约束。",
        truncate_text(requirement, 180)
    )
}

fn default_skill_ids(requirement: &str) -> Vec<String> {
    match infer_agent_category(requirement) {
        "engineering" => vec![
            "code_review".to_string(),
            "bug_fix".to_string(),
            "test_design".to_string(),
        ],
        "product" => vec![
            "requirement_analysis".to_string(),
            "roadmap_planning".to_string(),
            "prd_writing".to_string(),
        ],
        "growth" => vec![
            "campaign_planning".to_string(),
            "funnel_analysis".to_string(),
            "copywriting".to_string(),
        ],
        _ => vec![
            "task_planning".to_string(),
            "knowledge_summary".to_string(),
            "decision_support".to_string(),
        ],
    }
}

fn truncate_text(raw: &str, max_chars: usize) -> String {
    if raw.chars().count() <= max_chars {
        return raw.to_string();
    }
    let mut out: String = raw.chars().take(max_chars).collect();
    out.push_str("...");
    out
}

fn resolve_skill_state_root(user_id: &str) -> PathBuf {
    let user_segment = sanitize_user_segment(user_id);
    if let Ok(raw) = std::env::var("MEMORY_SKILL_STATE_ROOT") {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed).join(user_segment);
        }
    }

    let home = std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    home.join(".chatos")
        .join("memory_skill_center")
        .join(user_segment)
}

fn sanitize_user_segment(value: &str) -> String {
    let mut output = String::new();
    let mut last_dash = false;
    for ch in value.trim().chars() {
        let normalized = if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            ch
        } else {
            '-'
        };
        if normalized == '-' {
            if last_dash {
                continue;
            }
            last_dash = true;
        } else {
            last_dash = false;
        }
        output.push(normalized);
    }
    let trimmed = output.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "default".to_string()
    } else {
        trimmed
    }
}

fn ensure_dir(path: &FsPath) -> Result<(), String> {
    if path.as_os_str().is_empty() {
        return Ok(());
    }
    fs::create_dir_all(path).map_err(|err| err.to_string())
}

fn ensure_git_repo(
    repo_url: &str,
    branch: Option<&str>,
    cache_root: &FsPath,
) -> Result<PathBuf, String> {
    ensure_dir(cache_root)?;
    let safe_name = sanitize_repo_name(repo_url);
    let repo_path = cache_root.join(safe_name);

    if repo_path.exists() {
        fs::remove_dir_all(repo_path.as_path()).map_err(|err| {
            format!(
                "remove old repo failed ({}): {}",
                repo_path.to_string_lossy(),
                err
            )
        })?;
    }

    let mut args = vec!["clone".to_string(), "--depth".to_string(), "1".to_string()];
    if let Some(value) = branch {
        args.push("--branch".to_string());
        args.push(value.to_string());
    }
    args.push(repo_url.to_string());
    args.push(repo_path.to_string_lossy().to_string());
    run_git(args.as_slice())?;
    Ok(repo_path)
}

fn run_git(args: &[String]) -> Result<(), String> {
    let output = Command::new("git")
        .args(args)
        .output()
        .map_err(|err| format!("git execution failed: {}", err))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let detail = if !stderr.is_empty() { stderr } else { stdout };
    Err(format!(
        "git command failed (exit={}): {}",
        output.status.code().unwrap_or(-1),
        detail
    ))
}

fn sanitize_repo_name(value: &str) -> String {
    let mut raw = value.trim().to_string();
    if let Some(stripped) = raw.strip_prefix("https://") {
        raw = stripped.to_string();
    } else if let Some(stripped) = raw.strip_prefix("http://") {
        raw = stripped.to_string();
    }
    if let Some(stripped) = raw.strip_prefix("git@") {
        raw = stripped.to_string();
    }

    raw = raw.replace([':', '/'], "-");
    if raw.ends_with(".git") {
        raw.truncate(raw.len().saturating_sub(4));
    }

    let mut cleaned = String::new();
    let mut last_dash = false;
    for ch in raw.chars() {
        let valid = ch.is_ascii_alphanumeric() || ch == '_' || ch == '-';
        if valid {
            cleaned.push(ch);
            last_dash = false;
        } else if !last_dash {
            cleaned.push('-');
            last_dash = true;
        }
    }

    let trimmed = cleaned.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "repo".to_string()
    } else {
        trimmed
    }
}

fn load_plugin_candidates_from_repo(
    repo_root: &FsPath,
    marketplace_path: Option<&str>,
    plugins_path: Option<&str>,
) -> Result<Vec<SkillPluginCandidate>, String> {
    if let Some(path) = marketplace_path
        .map(normalize_repo_relative_path)
        .filter(|value| !value.is_empty())
    {
        let file = repo_root.join(path.as_str());
        if !file.exists() || !file.is_file() {
            return Err(format!(
                "marketplace path not found: {}",
                file.to_string_lossy()
            ));
        }
        let raw = fs::read_to_string(file.as_path()).map_err(|err| err.to_string())?;
        let parsed = parse_marketplace_candidates(raw.as_str())?;
        if !parsed.is_empty() {
            return Ok(parsed);
        }
    } else if let Some(file) = find_default_file_recursively(repo_root, &["marketplace.json"]) {
        if let Ok(raw) = fs::read_to_string(file.as_path()) {
            let parsed = parse_marketplace_candidates(raw.as_str())?;
            if !parsed.is_empty() {
                return Ok(parsed);
            }
        }
    }

    Ok(fallback_plugin_candidates(repo_root, plugins_path))
}

fn parse_marketplace_candidates(raw: &str) -> Result<Vec<SkillPluginCandidate>, String> {
    let value = serde_json::from_str::<Value>(raw)
        .map_err(|err| format!("marketplace json parse failed: {}", err))?;
    let plugins = value
        .get("plugins")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut out = Vec::new();
    for item in plugins {
        let source = item
            .get("source")
            .and_then(Value::as_str)
            .map(normalize_plugin_source)
            .unwrap_or_default();
        if source.is_empty() || has_parent_path_component(source.as_str()) {
            continue;
        }
        let name = item
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| source.clone());
        let category = item
            .get("category")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let description = item
            .get("description")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let version = item
            .get("version")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        out.push(SkillPluginCandidate {
            source,
            name,
            category,
            description,
            version,
        });
    }

    Ok(unique_plugin_candidates(out))
}

fn fallback_plugin_candidates(
    repo_root: &FsPath,
    plugins_path: Option<&str>,
) -> Vec<SkillPluginCandidate> {
    let root = plugins_path
        .map(normalize_repo_relative_path)
        .filter(|value| !value.is_empty())
        .map(|value| repo_root.join(value))
        .unwrap_or_else(|| repo_root.join("plugins"));
    if !root.exists() || !root.is_dir() {
        return Vec::new();
    }

    let entries = match fs::read_dir(root.as_path()) {
        Ok(value) => value,
        Err(_) => return Vec::new(),
    };

    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let rel = path_to_unix_relative(repo_root, path.as_path());
        let Some(rel) = rel else {
            continue;
        };
        let source = normalize_plugin_source(rel.as_str());
        if source.is_empty() || has_parent_path_component(source.as_str()) {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| source.clone());
        out.push(SkillPluginCandidate {
            source,
            name,
            category: None,
            description: None,
            version: None,
        });
    }

    unique_plugin_candidates(out)
}

fn unique_plugin_candidates(items: Vec<SkillPluginCandidate>) -> Vec<SkillPluginCandidate> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for item in items {
        if seen.insert(item.source.clone()) {
            out.push(item);
        }
    }
    out
}

fn copy_plugin_source_from_repo(
    repo_root: &FsPath,
    plugins_root: &FsPath,
    source: &str,
) -> Result<String, String> {
    let normalized = normalize_plugin_source(source);
    if normalized.is_empty() {
        return Err("plugin source is empty".to_string());
    }
    if has_parent_path_component(normalized.as_str()) {
        return Err("plugin source cannot contain ..".to_string());
    }

    let src = repo_root.join(normalized.as_str());
    if !src.exists() {
        return Err(format!(
            "plugin source not found in repository: {}",
            normalized
        ));
    }

    let dest_rel = plugin_install_destination(normalized.as_str());
    if dest_rel.is_empty() {
        return Err("plugin source normalization failed".to_string());
    }

    let dest = plugins_root.join(dest_rel.as_str());
    copy_path(src.as_path(), dest.as_path())?;
    Ok(dest_rel)
}

fn copy_path(src: &FsPath, dest: &FsPath) -> Result<(), String> {
    if !src.exists() {
        return Err(format!("source not found: {}", src.to_string_lossy()));
    }

    if dest.exists() {
        if dest.is_dir() {
            fs::remove_dir_all(dest).map_err(|err| err.to_string())?;
        } else {
            fs::remove_file(dest).map_err(|err| err.to_string())?;
        }
    }

    if src.is_file() {
        if let Some(parent) = dest.parent() {
            ensure_dir(parent)?;
        }
        fs::copy(src, dest).map_err(|err| err.to_string())?;
        return Ok(());
    }

    ensure_dir(dest)?;
    for entry in fs::read_dir(src).map_err(|err| err.to_string())? {
        let entry = entry.map_err(|err| err.to_string())?;
        let path = entry.path();
        let next = dest.join(entry.file_name());
        let file_type = entry.file_type().map_err(|err| err.to_string())?;
        if file_type.is_dir() {
            copy_path(path.as_path(), next.as_path())?;
        } else if file_type.is_file() {
            if let Some(parent) = next.parent() {
                ensure_dir(parent)?;
            }
            fs::copy(path.as_path(), next.as_path()).map_err(|err| err.to_string())?;
        }
    }

    Ok(())
}

fn normalize_plugin_source(value: &str) -> String {
    let mut normalized = value.trim().replace('\\', "/");
    while normalized.starts_with("./") {
        normalized = normalized[2..].to_string();
    }
    normalized = normalized.trim_start_matches('/').to_string();
    normalized.trim_matches('/').to_string()
}

fn plugin_install_destination(source: &str) -> String {
    let normalized = normalize_plugin_source(source);
    if let Some(stripped) = normalized.strip_prefix("plugins/") {
        stripped.trim_matches('/').to_string()
    } else {
        normalized
    }
}

fn has_parent_path_component(path: &str) -> bool {
    FsPath::new(path)
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
}

fn find_default_file_recursively(root: &FsPath, names: &[&str]) -> Option<PathBuf> {
    let mut candidate_names = HashSet::new();
    for name in names {
        candidate_names.insert((*name).to_ascii_lowercase());
    }

    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(dir.as_path()) {
            Ok(value) => value,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                let file_name = path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if candidate_names.contains(file_name.as_str()) {
                    return Some(path);
                }
                continue;
            }
            if path.is_dir() && !is_skipped_repo_dir(path.as_path()) {
                stack.push(path);
            }
        }
    }
    None
}

fn is_skipped_repo_dir(path: &FsPath) -> bool {
    let Some(name) = path.file_name().and_then(|v| v.to_str()) else {
        return false;
    };

    matches!(name, ".git" | "node_modules" | "target" | ".next")
}

fn normalize_repo_relative_path(value: &str) -> String {
    let mut normalized = value.trim().replace('\\', "/");
    while normalized.starts_with("./") {
        normalized = normalized[2..].to_string();
    }
    normalized = normalized.trim_start_matches('/').to_string();
    normalized.trim_matches('/').to_string()
}

fn resolve_plugin_root_from_cache(
    plugins_root: &FsPath,
    cache_path: Option<&str>,
    source: &str,
) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(value) = cache_path
        .map(normalize_repo_relative_path)
        .filter(|value| !value.is_empty())
    {
        candidates.push(value);
    }
    let normalized = normalize_plugin_source(source);
    if !normalized.is_empty() {
        candidates.push(normalized.clone());
        if let Some(stripped) = normalized.strip_prefix("plugins/") {
            candidates.push(stripped.to_string());
        } else {
            candidates.push(format!("plugins/{}", normalized));
        }
    }
    for rel in unique_strings(candidates) {
        let path = plugins_root.join(rel.as_str());
        if path.exists() && path.is_dir() {
            return Some(path);
        }
    }
    None
}

fn discover_skill_entries(plugin_root: &FsPath) -> Vec<String> {
    let root = plugin_root.join("skills");
    if !root.exists() || !root.is_dir() {
        return Vec::new();
    }

    let mut seen = HashSet::new();
    for path in collect_markdown_entries(root.as_path()) {
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");

        if file_name.eq_ignore_ascii_case("README.md") {
            continue;
        }

        if file_name.eq_ignore_ascii_case("SKILL.md") || file_name.eq_ignore_ascii_case("index.md")
        {
            let parent = path.parent().unwrap_or_else(|| root.as_path());
            if let Some(rel) = path_to_unix_relative(plugin_root, parent) {
                if !rel.trim().is_empty() {
                    seen.insert(rel);
                }
            }
            continue;
        }

        if contains_path_component(path.as_path(), "references") {
            continue;
        }

        if let Some(rel) = path_to_unix_relative(plugin_root, path.as_path()) {
            seen.insert(rel);
        }
    }

    let mut items = seen.into_iter().collect::<Vec<_>>();
    items.sort();
    items
}

fn collect_markdown_entries(root: &FsPath) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if !root.exists() || !root.is_dir() {
        return out;
    }

    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(dir.as_path()) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let file_type = match entry.file_type() {
                Ok(value) => value,
                Err(_) => continue,
            };

            if file_type.is_dir() {
                if !is_skipped_repo_dir(path.as_path()) {
                    stack.push(path);
                }
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            let is_markdown = path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("md"))
                .unwrap_or(false);
            if is_markdown {
                out.push(path);
            }
        }
    }

    out
}

fn path_to_unix_relative(base: &FsPath, path: &FsPath) -> Option<String> {
    let rel = path.strip_prefix(base).ok()?;
    let rendered = rel.to_string_lossy().replace('\\', "/");
    let trimmed = rendered.trim_matches('/').to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn contains_path_component(path: &FsPath, target: &str) -> bool {
    path.components().any(|comp| {
        comp.as_os_str()
            .to_str()
            .map(|name| name.eq_ignore_ascii_case(target))
            .unwrap_or(false)
    })
}

fn normalize_skill_entry_to_file(plugin_root: &FsPath, entry: &str) -> Option<PathBuf> {
    let normalized = normalize_repo_relative_path(entry);
    if normalized.is_empty() {
        return None;
    }
    let path = plugin_root.join(normalized.as_str());
    if path.is_file() {
        return Some(path);
    }
    if path.is_dir() {
        let skill_md = path.join("SKILL.md");
        if skill_md.exists() && skill_md.is_file() {
            return Some(skill_md);
        }
        let index_md = path.join("index.md");
        if index_md.exists() && index_md.is_file() {
            return Some(index_md);
        }
    }
    None
}

fn build_skill_name_from_entry(entry: &str) -> String {
    let normalized = normalize_repo_relative_path(entry);
    if normalized.is_empty() {
        return "Skill".to_string();
    }

    let parts = normalized
        .split('/')
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>();
    if parts.is_empty() {
        return "Skill".to_string();
    }

    let last = parts.last().copied().unwrap_or("");
    if last.eq_ignore_ascii_case("SKILL.md") || last.eq_ignore_ascii_case("index.md") {
        return parts
            .iter()
            .rev()
            .nth(1)
            .map(|value| (*value).to_string())
            .unwrap_or_else(|| "Skill".to_string());
    }
    if let Some(stem) = last.strip_suffix(".md") {
        return stem.to_string();
    }
    last.to_string()
}

fn hash_id(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update([0u8]);
    }
    let digest = hasher.finalize();
    let mut out = String::new();
    for byte in digest {
        out.push_str(format!("{:02x}", byte).as_str());
    }
    out
}

fn unique_strings(values: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for item in values {
        let trimmed = item.trim().to_string();
        if trimmed.is_empty() {
            continue;
        }
        if seen.insert(trimmed.clone()) {
            out.push(trimmed);
        }
    }
    out
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

    match messages::list_messages_by_session(&state.pool, session_id.as_str(), limit, offset, asc)
        .await
    {
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
        Ok(deleted) => (
            StatusCode::OK,
            Json(json!({"deleted": deleted, "success": true})),
        ),
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
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "message not found"})),
        ),
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
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "message not found"})),
        ),
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
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "summary not found"})),
        ),
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
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "model config not found"})),
            )
        }
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
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "model config not found"})),
        ),
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
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "model config not found"})),
            )
        }
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
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "model config not found"})),
            )
        }
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

async fn get_agent_memory_job_config(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(q): Query<UserIdQuery>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let user_id = resolve_scope_user_id(&auth, q.user_id);
    match configs::get_agent_memory_job_config(&state.pool, user_id.as_str()).await {
        Ok(cfg) => (StatusCode::OK, Json(json!(cfg))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "get agent memory job config failed", "detail": err})),
        ),
    }
}

async fn put_agent_memory_job_config(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(mut req): Json<UpsertAgentMemoryJobConfigRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match resolve_identity(&headers, state.as_ref()) {
        Ok(v) => v,
        Err(err) => return err,
    };
    req.user_id = resolve_scope_user_id(&auth, Some(req.user_id.clone()));

    match configs::upsert_agent_memory_job_config(&state.pool, req).await {
        Ok(cfg) => (StatusCode::OK, Json(json!(cfg))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "save agent memory job config failed", "detail": err})),
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

async fn run_agent_memory_once(
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

    match jobs::agent_memory::run_once(&state.pool, &ai, scope_user_id.as_str()).await {
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
