// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path as FsPath};
use std::sync::Mutex;
use std::time::Duration;

use axum::body::Body;
use axum::extract::{Path, Query};
use axum::http::header::{
    CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_SECURITY_POLICY, CONTENT_TYPE,
    REFERRER_POLICY,
};
use axum::http::{HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use chatos_plugin_management_sdk::{
    plugin_ui_snapshot_sha256, PluginArtifactCreateRequest, PluginArtifactDescriptor,
    PluginArtifactListRequest, PluginArtifactListResponse, PluginArtifactReadMode,
    PluginArtifactReadRequest, PluginArtifactReadResponse, PluginArtifactUiAccess,
    PluginArtifactUpdateRequest, PluginArtifactWriteOperation, PluginArtifactWriteResponse,
    PluginUiAssetKind, PluginUiAssetReadResponse, PluginUiReadyEventPayload, PluginUiSnapshot,
    PLUGIN_ARTIFACT_INLINE_READ_MAX_BYTES, PLUGIN_ARTIFACT_MAX_BYTES,
    PLUGIN_ARTIFACT_WRITE_MAX_BYTES, PLUGIN_UI_ASSET_MAX_BYTES,
    PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_CREATE, PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_DOWNLOAD,
    PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_LIST, PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_READ,
    PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_UPDATE, PLUGIN_UI_BRIDGE_CAPABILITY_HOST_CONTEXT_READ,
    PLUGIN_UI_BRIDGE_PROTOCOL_VERSION_V1, PLUGIN_UI_ENTRYPOINT_MAX_BYTES, PLUGIN_UI_HOST_CSP_V1,
    PLUGIN_UI_IFRAME_SANDBOX_V1, PLUGIN_UI_MAX_ARTIFACT_MIME_TYPES, PLUGIN_UI_MAX_ASSETS,
    PLUGIN_UI_MAX_BRIDGE_CAPABILITIES, PLUGIN_UI_READY_EVENT_VERSION_V1,
    PLUGIN_UI_SURFACE_ARTIFACT_VIEWER, PLUGIN_UI_SURFACE_DETAIL_PANEL,
    PLUGIN_UI_SURFACE_MESSAGE_PANEL, PLUGIN_UI_SURFACE_WORKBENCH, PLUGIN_UI_TOTAL_ASSET_MAX_BYTES,
};
use chatos_service_runtime::http_body::read_response_bytes_limited;
use once_cell::sync::Lazy;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::context::MessageTaskRunnerLookupQuery;
use super::resolve_message_task_runner_context;
use crate::config::Config;
use crate::core::auth::AuthUser;
use crate::services::task_runner_api_client;

const LOCAL_CONNECTOR_TOKEN_AUDIENCE: &str = "local-connector-service";
const PLUGIN_UI_READ_SCOPE: &str = "plugin.ui.read";
const PLUGIN_ARTIFACT_READ_SCOPE: &str = "plugin.artifact.read";
const PLUGIN_ARTIFACT_WRITE_SCOPE: &str = "plugin.artifact.write";
const PLUGIN_UI_RELAY_RESPONSE_LIMIT_BYTES: usize = 12 * 1024 * 1024;
const PLUGIN_ARTIFACT_LIST_RELAY_RESPONSE_LIMIT_BYTES: usize = 4 * 1024 * 1024;
const PLUGIN_ARTIFACT_READ_RELAY_RESPONSE_LIMIT_BYTES: usize = 88 * 1024 * 1024;
const PLUGIN_ARTIFACT_WRITE_RELAY_RESPONSE_LIMIT_BYTES: usize = 4 * 1024 * 1024;
const PLUGIN_ARTIFACT_INTERACTIVE_RELAY_TIMEOUT_MS: i64 = 5 * 60 * 1_000 + 15_000;
const PLUGIN_UI_WORKBENCH_SESSION_TTL_SECONDS: i64 = 300;
const PLUGIN_UI_WORKBENCH_MAX_SESSIONS: usize = 256;
const PLUGIN_UI_WORKBENCH_MAX_SESSIONS_PER_OWNER: usize = 16;

static PLUGIN_UI_WORKBENCH_SESSIONS: Lazy<Mutex<BTreeMap<String, PluginUiWorkbenchSession>>> =
    Lazy::new(|| Mutex::new(BTreeMap::new()));

type ApiError = (StatusCode, Json<Value>);

#[derive(Debug, Clone)]
pub struct PreparedPluginArtifactRelayRequest {
    pub url: String,
    pub workspace_id: String,
    pub owner_user_id: String,
    pub token: String,
    pub timeout: Duration,
}

pub(super) fn router() -> Router {
    Router::new().route(
        "/api/messages/{message_id}/task-runner/runs/{run_id}/plugin-ui/{event_id}/assets/{*asset_path}",
        get(get_plugin_ui_asset),
    )
    .route(
        "/api/messages/{message_id}/task-runner/runs/{run_id}/plugin-ui/{event_id}/workbench-sessions",
        post(create_plugin_ui_workbench_session),
    )
    .route(
        "/api/messages/{message_id}/task-runner/runs/{run_id}/plugin-ui/{event_id}/workbench-sessions/{session_id}",
        delete(revoke_plugin_ui_workbench_session),
    )
    .route(
        "/api/messages/{message_id}/task-runner/runs/{run_id}/plugin-ui/{event_id}/workbench-sessions/{session_id}/artifacts",
        get(list_plugin_ui_workbench_artifacts).post(create_plugin_ui_workbench_artifact),
    )
    .route(
        "/api/messages/{message_id}/task-runner/runs/{run_id}/plugin-ui/{event_id}/workbench-sessions/{session_id}/artifacts/{artifact_id}",
        get(read_plugin_ui_workbench_artifact).put(update_plugin_ui_workbench_artifact),
    )
}

pub(super) fn public_router() -> Router {
    Router::new()
        .route(
            "/api/plugin-ui/workbench/{session_id}/{*asset_path}",
            get(get_plugin_ui_workbench_asset),
        )
        .route(
            "/api/plugin-artifacts/workbench/{session_id}/{artifact_id}/download",
            get(download_plugin_ui_workbench_artifact),
        )
}

#[derive(Debug, Clone)]
struct PluginUiWorkbenchSession {
    owner_user_id: String,
    message_id: String,
    run_id: String,
    event_id: String,
    expires_at_epoch_seconds: i64,
    ready: PluginUiReadyEventPayload,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PluginUiWorkbenchArtifactCreateBody {
    display_name: String,
    media_type: String,
    body_base64: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PluginUiWorkbenchArtifactUpdateBody {
    expected_sha256: String,
    body_base64: String,
}

#[derive(Debug, Serialize)]
struct PluginUiWorkbenchHostContext {
    run_id: String,
    plugin_id: String,
    release_id: String,
    component_key: String,
    title: String,
    surface: String,
}

#[derive(Debug, Serialize)]
struct PluginUiWorkbenchSessionResponse {
    session_id: String,
    expires_in: i64,
    expires_at: String,
    iframe_path: String,
    bridge_protocol_version: u32,
    adapter_session_id: String,
    host_session_nonce: String,
    bridge_capabilities: Vec<String>,
    host_context: PluginUiWorkbenchHostContext,
}

async fn get_plugin_ui_asset(
    auth: AuthUser,
    Path((message_id, run_id, event_id, asset_path)): Path<(String, String, String, String)>,
    Query(query): Query<MessageTaskRunnerLookupQuery>,
) -> Result<Response, ApiError> {
    let relative_path = normalize_requested_asset_path(asset_path.as_str())?;
    let ready = resolve_ready_event_for_message(
        &auth,
        message_id.as_str(),
        run_id.as_str(),
        event_id.as_str(),
        &query,
    )
    .await?;
    let asset = request_local_connector_asset(&auth, &ready, relative_path.as_str()).await?;
    validate_asset_response(&auth, &ready, relative_path.as_str(), &asset)?;
    plugin_ui_asset_response(&ready.ui, asset, None)
}

async fn create_plugin_ui_workbench_session(
    auth: AuthUser,
    Path((message_id, run_id, event_id)): Path<(String, String, String)>,
    Query(query): Query<MessageTaskRunnerLookupQuery>,
) -> Result<Response, ApiError> {
    let ready = resolve_ready_event_for_message(
        &auth,
        message_id.as_str(),
        run_id.as_str(),
        event_id.as_str(),
        &query,
    )
    .await?;
    let config = Config::try_get().map_err(|_| service_unavailable("ChatOS 配置不可用"))?;
    let issued = issue_plugin_ui_workbench_session(
        &auth,
        message_id.as_str(),
        event_id.as_str(),
        ready,
        config.plugin_ui_resource_origin.as_deref(),
    )?;
    let mut response = Json(issued).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        HeaderName::from_static("x-content-type-options"),
        HeaderValue::from_static("nosniff"),
    );
    response
        .headers_mut()
        .insert(REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    Ok(response)
}

async fn revoke_plugin_ui_workbench_session(
    auth: AuthUser,
    Path((message_id, run_id, event_id, session_id)): Path<(String, String, String, String)>,
) -> Result<StatusCode, ApiError> {
    let session_id = normalize_workbench_session_id(session_id.as_str())?;
    let mut sessions = lock_workbench_sessions()?;
    prune_expired_workbench_sessions(&mut sessions);
    let Some(session) = sessions.get(session_id) else {
        return Err(not_found("Plugin UI Workbench session 不存在"));
    };
    if session.owner_user_id != auth.user_id
        || session.message_id != message_id
        || session.run_id != run_id
        || session.event_id != event_id
    {
        return Err(not_found("Plugin UI Workbench session 不存在"));
    }
    sessions.remove(session_id);
    Ok(StatusCode::NO_CONTENT)
}

async fn list_plugin_ui_workbench_artifacts(
    auth: AuthUser,
    Path((message_id, run_id, event_id, session_id)): Path<(String, String, String, String)>,
) -> Result<Response, ApiError> {
    let session = owned_plugin_ui_workbench_session(
        &auth,
        message_id.as_str(),
        run_id.as_str(),
        event_id.as_str(),
        session_id.as_str(),
    )?;
    let access = artifact_access_for_session(&session);
    let response =
        request_local_connector_artifact_list(&auth, &session.ready, access.clone()).await?;
    validate_artifact_list_response(&auth, &session.ready, &access, &response)?;
    let mut response = Json(response).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

async fn create_plugin_ui_workbench_artifact(
    auth: AuthUser,
    Path((message_id, run_id, event_id, session_id)): Path<(String, String, String, String)>,
    Json(body): Json<PluginUiWorkbenchArtifactCreateBody>,
) -> Result<Response, ApiError> {
    let session = owned_plugin_ui_workbench_session(
        &auth,
        message_id.as_str(),
        run_id.as_str(),
        event_id.as_str(),
        session_id.as_str(),
    )?;
    require_workbench_capability(&session, PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_CREATE)?;
    let bytes = decode_artifact_write_body(body.body_base64.as_str())?;
    let access = artifact_access_for_session(&session);
    let response = request_local_connector_artifact_create(
        &auth,
        &session.ready,
        access.clone(),
        body.display_name.as_str(),
        body.media_type.as_str(),
        body.body_base64,
    )
    .await?;
    validate_artifact_write_response(
        &auth,
        &session.ready,
        &access,
        PluginArtifactWriteOperation::Create,
        None,
        Some((body.display_name.as_str(), body.media_type.as_str())),
        bytes.as_slice(),
        &response,
    )?;
    let mut response = Json(response).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

async fn read_plugin_ui_workbench_artifact(
    auth: AuthUser,
    Path((message_id, run_id, event_id, session_id, artifact_id)): Path<(
        String,
        String,
        String,
        String,
        String,
    )>,
) -> Result<Response, ApiError> {
    let session = owned_plugin_ui_workbench_session(
        &auth,
        message_id.as_str(),
        run_id.as_str(),
        event_id.as_str(),
        session_id.as_str(),
    )?;
    let artifact_id = normalize_plugin_artifact_id(artifact_id.as_str())?;
    let access = artifact_access_for_session(&session);
    let response = request_local_connector_artifact_read(
        &auth,
        &session.ready,
        access.clone(),
        artifact_id,
        PluginArtifactReadMode::Inline,
    )
    .await?;
    validate_artifact_read_response(&auth, &session.ready, &access, artifact_id, &response)?;
    if response.artifact.size_bytes > PLUGIN_ARTIFACT_INLINE_READ_MAX_BYTES {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(json!({"error": "Plugin Artifact 过大，无法内联读取"})),
        ));
    }
    let mut response = Json(response).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

async fn update_plugin_ui_workbench_artifact(
    auth: AuthUser,
    Path((message_id, run_id, event_id, session_id, artifact_id)): Path<(
        String,
        String,
        String,
        String,
        String,
    )>,
    Json(body): Json<PluginUiWorkbenchArtifactUpdateBody>,
) -> Result<Response, ApiError> {
    let session = owned_plugin_ui_workbench_session(
        &auth,
        message_id.as_str(),
        run_id.as_str(),
        event_id.as_str(),
        session_id.as_str(),
    )?;
    require_workbench_capability(&session, PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_UPDATE)?;
    let artifact_id = normalize_plugin_artifact_id(artifact_id.as_str())?;
    if !is_lower_sha256(body.expected_sha256.as_str()) {
        return Err(bad_request("expected_sha256 无效"));
    }
    let bytes = decode_artifact_write_body(body.body_base64.as_str())?;
    let access = artifact_access_for_session(&session);
    let response = request_local_connector_artifact_update(
        &auth,
        &session.ready,
        access.clone(),
        artifact_id,
        body.expected_sha256.as_str(),
        body.body_base64,
    )
    .await?;
    validate_artifact_write_response(
        &auth,
        &session.ready,
        &access,
        PluginArtifactWriteOperation::Update,
        Some(artifact_id),
        None,
        bytes.as_slice(),
        &response,
    )?;
    let mut response = Json(response).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

async fn download_plugin_ui_workbench_artifact(
    Path((session_id, artifact_id)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    let session = get_plugin_ui_workbench_session(session_id.as_str())?;
    let artifact_id = normalize_plugin_artifact_id(artifact_id.as_str())?;
    let auth = AuthUser {
        user_id: session.owner_user_id.clone(),
        role: "user".to_string(),
    };
    let access = artifact_access_for_session(&session);
    let artifact = request_local_connector_artifact_read(
        &auth,
        &session.ready,
        access.clone(),
        artifact_id,
        PluginArtifactReadMode::Download,
    )
    .await?;
    validate_artifact_read_response(&auth, &session.ready, &access, artifact_id, &artifact)?;
    get_plugin_ui_workbench_session(session_id.as_str())?;
    plugin_artifact_download_response(artifact)
}

async fn get_plugin_ui_workbench_asset(
    Path((session_id, asset_path)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    let relative_path = normalize_requested_asset_path(asset_path.as_str())?;
    let session = get_plugin_ui_workbench_session(session_id.as_str())?;
    let auth = AuthUser {
        user_id: session.owner_user_id,
        role: "user".to_string(),
    };
    let asset =
        request_local_connector_asset(&auth, &session.ready, relative_path.as_str()).await?;
    validate_asset_response(&auth, &session.ready, relative_path.as_str(), &asset)?;
    get_plugin_ui_workbench_session(session_id.as_str())?;
    let config = Config::try_get().map_err(|_| service_unavailable("ChatOS 配置不可用"))?;
    plugin_ui_asset_response(
        &session.ready.ui,
        asset,
        config.plugin_ui_parent_origin.as_deref(),
    )
}

async fn resolve_ready_event_for_message(
    auth: &AuthUser,
    message_id: &str,
    run_id: &str,
    event_id: &str,
    query: &MessageTaskRunnerLookupQuery,
) -> Result<PluginUiReadyEventPayload, ApiError> {
    let context = resolve_message_task_runner_context(auth, message_id, query)
        .await?
        .ok_or_else(|| not_found("当前消息没有关联的任务来源"))?;
    let event = task_runner_api_client::get_message_run_event(
        context.base_url.as_str(),
        run_id,
        event_id,
        context.source_session_id.as_str(),
        context.source_user_message_id.as_deref(),
        context.source_turn_id.as_deref(),
    )
    .await
    .map_err(|_| bad_gateway("读取 Plugin UI 运行事件失败"))?;
    let ready = decode_ready_event(&event, run_id, event_id)?;
    validate_ready_payload(&ready)?;
    Ok(ready)
}

fn issue_plugin_ui_workbench_session(
    auth: &AuthUser,
    message_id: &str,
    event_id: &str,
    ready: PluginUiReadyEventPayload,
    resource_origin: Option<&str>,
) -> Result<PluginUiWorkbenchSessionResponse, ApiError> {
    let now = chrono::Utc::now().timestamp();
    let expires_at_epoch_seconds = now + PLUGIN_UI_WORKBENCH_SESSION_TTL_SECONDS;
    let mut sessions = lock_workbench_sessions()?;
    prune_expired_workbench_sessions(&mut sessions);
    sessions.retain(|_, session| {
        session.owner_user_id != auth.user_id
            || session.message_id != message_id
            || session.run_id != ready.run_id
            || session.event_id != event_id
            || session.ready.component_key != ready.component_key
    });
    if sessions.len() >= PLUGIN_UI_WORKBENCH_MAX_SESSIONS {
        return Err(service_unavailable(
            "Plugin UI Workbench session 已达到上限",
        ));
    }
    let owner_session_count = sessions
        .values()
        .filter(|session| session.owner_user_id == auth.user_id)
        .count();
    if owner_session_count >= PLUGIN_UI_WORKBENCH_MAX_SESSIONS_PER_OWNER {
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({ "error": "Plugin UI Workbench session 过多，请关闭旧面板后重试" })),
        ));
    }

    let session_id = format!("pui_{}", hex::encode(rand::random::<[u8; 32]>()));
    let host_session_nonce = format!("puih_{}", hex::encode(rand::random::<[u8; 32]>()));
    let entrypoint_path = encode_relative_asset_url_path(ready.ui.relative_source_path.as_str())?;
    let iframe_path = format!(
        "{}/api/plugin-ui/workbench/{session_id}/{entrypoint_path}#chatos_plugin_ui_v1&protocol_version={}&adapter_session_id={}&host_session_nonce={}",
        resource_origin.unwrap_or_default(),
        ready.ui.bridge_protocol_version,
        urlencoding::encode(ready.adapter_session_id.as_str()),
        urlencoding::encode(host_session_nonce.as_str()),
    );
    let response = PluginUiWorkbenchSessionResponse {
        session_id: session_id.clone(),
        expires_in: PLUGIN_UI_WORKBENCH_SESSION_TTL_SECONDS,
        expires_at: chrono::DateTime::<chrono::Utc>::from_timestamp(expires_at_epoch_seconds, 0)
            .map(|value| value.to_rfc3339())
            .unwrap_or_else(crate::core::time::now_rfc3339),
        iframe_path,
        bridge_protocol_version: ready.ui.bridge_protocol_version,
        adapter_session_id: ready.adapter_session_id.clone(),
        host_session_nonce,
        bridge_capabilities: ready.ui.bridge_capabilities.clone(),
        host_context: PluginUiWorkbenchHostContext {
            run_id: ready.run_id.clone(),
            plugin_id: ready.plugin_id.clone(),
            release_id: ready.release_id.clone(),
            component_key: ready.component_key.clone(),
            title: ready.ui.title.clone(),
            surface: ready.ui.surface.clone(),
        },
    };
    sessions.insert(
        session_id,
        PluginUiWorkbenchSession {
            owner_user_id: auth.user_id.clone(),
            message_id: message_id.to_string(),
            run_id: ready.run_id.clone(),
            event_id: event_id.to_string(),
            expires_at_epoch_seconds,
            ready,
        },
    );
    Ok(response)
}

fn get_plugin_ui_workbench_session(session_id: &str) -> Result<PluginUiWorkbenchSession, ApiError> {
    let session_id = normalize_workbench_session_id(session_id)?;
    let mut sessions = lock_workbench_sessions()?;
    prune_expired_workbench_sessions(&mut sessions);
    sessions
        .get(session_id)
        .cloned()
        .ok_or_else(|| not_found("Plugin UI Workbench session 不存在或已过期"))
}

fn owned_plugin_ui_workbench_session(
    auth: &AuthUser,
    message_id: &str,
    run_id: &str,
    event_id: &str,
    session_id: &str,
) -> Result<PluginUiWorkbenchSession, ApiError> {
    let session = get_plugin_ui_workbench_session(session_id)?;
    if session.owner_user_id != auth.user_id
        || session.message_id != message_id
        || session.run_id != run_id
        || session.event_id != event_id
    {
        return Err(not_found("Plugin UI Workbench session 不存在"));
    }
    Ok(session)
}

fn artifact_access_for_session(session: &PluginUiWorkbenchSession) -> PluginArtifactUiAccess {
    PluginArtifactUiAccess {
        run_id: session.ready.run_id.clone(),
        plugin_id: session.ready.plugin_id.clone(),
        release_id: session.ready.release_id.clone(),
        artifact_sha256: session.ready.artifact_sha256.clone(),
        component_key: session.ready.component_key.clone(),
        adapter_session_id: session.ready.adapter_session_id.clone(),
        ui_snapshot_sha256: session.ready.ui.snapshot_sha256.clone(),
    }
}

fn require_workbench_capability(
    session: &PluginUiWorkbenchSession,
    capability: &str,
) -> Result<(), ApiError> {
    if !session
        .ready
        .ui
        .bridge_capabilities
        .iter()
        .any(|value| value == capability)
    {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({"error": "Plugin UI 未声明所需 Artifact capability"})),
        ));
    }
    Ok(())
}

fn decode_artifact_write_body(body_base64: &str) -> Result<Vec<u8>, ApiError> {
    let encoded_limit = PLUGIN_ARTIFACT_WRITE_MAX_BYTES
        .div_ceil(3)
        .saturating_mul(4) as usize;
    if body_base64.len() > encoded_limit {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(json!({"error": "Plugin Artifact 写入内容过大"})),
        ));
    }
    let bytes = BASE64_STANDARD
        .decode(body_base64)
        .map_err(|_| bad_request("Plugin Artifact body_base64 无效"))?;
    if bytes.len() as u64 > PLUGIN_ARTIFACT_WRITE_MAX_BYTES
        || BASE64_STANDARD.encode(bytes.as_slice()) != body_base64
    {
        return Err(bad_request("Plugin Artifact body_base64 不是规范编码"));
    }
    Ok(bytes)
}

fn lock_workbench_sessions(
) -> Result<std::sync::MutexGuard<'static, BTreeMap<String, PluginUiWorkbenchSession>>, ApiError> {
    PLUGIN_UI_WORKBENCH_SESSIONS
        .lock()
        .map_err(|_| service_unavailable("Plugin UI Workbench session store 不可用"))
}

fn prune_expired_workbench_sessions(sessions: &mut BTreeMap<String, PluginUiWorkbenchSession>) {
    let now = chrono::Utc::now().timestamp();
    sessions.retain(|_, session| session.expires_at_epoch_seconds > now);
}

fn normalize_workbench_session_id(session_id: &str) -> Result<&str, ApiError> {
    if session_id.len() != 68
        || !session_id.starts_with("pui_")
        || !session_id[4..]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(not_found("Plugin UI Workbench session 不存在"));
    }
    Ok(session_id)
}

fn encode_relative_asset_url_path(relative_path: &str) -> Result<String, ApiError> {
    let path = relative_path
        .strip_prefix("./")
        .ok_or_else(|| bad_gateway("Plugin UI entrypoint 路径无效"))?;
    if path.is_empty() {
        return Err(bad_gateway("Plugin UI entrypoint 路径无效"));
    }
    Ok(path
        .split('/')
        .map(|segment| urlencoding::encode(segment).into_owned())
        .collect::<Vec<_>>()
        .join("/"))
}

fn decode_ready_event(
    event: &Value,
    expected_run_id: &str,
    expected_event_id: &str,
) -> Result<PluginUiReadyEventPayload, ApiError> {
    if event.get("id").and_then(Value::as_str) != Some(expected_event_id)
        || event.get("run_id").and_then(Value::as_str) != Some(expected_run_id)
        || event.get("event_type").and_then(Value::as_str) != Some("plugin_ui_ready")
    {
        return Err(not_found("Plugin UI 运行事件不存在"));
    }
    let payload: PluginUiReadyEventPayload = serde_json::from_value(
        event
            .get("payload")
            .cloned()
            .ok_or_else(|| bad_gateway("Plugin UI 运行事件缺少安全描述符"))?,
    )
    .map_err(|_| bad_gateway("Plugin UI 运行事件格式无效"))?;
    if payload.run_id != expected_run_id {
        return Err(bad_gateway("Plugin UI 运行事件 Run identity 不匹配"));
    }
    Ok(payload)
}

fn validate_ready_payload(payload: &PluginUiReadyEventPayload) -> Result<(), ApiError> {
    if payload.event_schema_version != PLUGIN_UI_READY_EVENT_VERSION_V1
        || payload.run_id.trim().is_empty()
        || payload.device_id.trim().is_empty()
        || payload.plugin_id.trim().is_empty()
        || payload.release_id.trim().is_empty()
        || payload.component_key.trim().is_empty()
        || payload.adapter_session_id.trim().is_empty()
        || !is_lower_sha256(payload.artifact_sha256.as_str())
    {
        return Err(bad_gateway("Plugin UI session identity 无效"));
    }
    let ui = &payload.ui;
    if ui.plugin_id != payload.plugin_id
        || ui.release_id != payload.release_id
        || ui.artifact_sha256 != payload.artifact_sha256
        || ui.component_key != payload.component_key
        || ui.bridge_protocol_version != PLUGIN_UI_BRIDGE_PROTOCOL_VERSION_V1
        || ui.content_security_policy != PLUGIN_UI_HOST_CSP_V1
        || ui.iframe_sandbox != PLUGIN_UI_IFRAME_SANDBOX_V1
        || !is_safe_ui_path(ui.relative_source_path.as_str(), true)
        || !is_lower_sha256(ui.content_sha256.as_str())
        || ![
            PLUGIN_UI_SURFACE_DETAIL_PANEL,
            PLUGIN_UI_SURFACE_MESSAGE_PANEL,
            PLUGIN_UI_SURFACE_WORKBENCH,
            PLUGIN_UI_SURFACE_ARTIFACT_VIEWER,
        ]
        .contains(&ui.surface.as_str())
    {
        return Err(bad_gateway("Plugin UI immutable descriptor 无效"));
    }
    validate_bridge_capabilities(ui)?;
    validate_artifact_mime_types(ui)?;

    if ui.assets.len() > PLUGIN_UI_MAX_ASSETS {
        return Err(bad_gateway("Plugin UI asset allowlist 超出限制"));
    }
    let mut paths = BTreeSet::new();
    let mut total_bytes = 0_u64;
    for asset in &ui.assets {
        if !paths.insert(asset.relative_path.as_str())
            || asset.relative_path == ui.relative_source_path
            || !is_safe_ui_path(asset.relative_path.as_str(), false)
            || expected_media_type(asset.relative_path.as_str()) != Some(asset.media_type.as_str())
            || asset.size_bytes > PLUGIN_UI_ASSET_MAX_BYTES
            || !is_lower_sha256(asset.sha256.as_str())
        {
            return Err(bad_gateway("Plugin UI asset descriptor 无效"));
        }
        total_bytes = total_bytes
            .checked_add(asset.size_bytes)
            .ok_or_else(|| bad_gateway("Plugin UI asset 总大小溢出"))?;
    }
    if total_bytes > PLUGIN_UI_TOTAL_ASSET_MAX_BYTES {
        return Err(bad_gateway("Plugin UI asset 总大小超出限制"));
    }
    let expected_snapshot_sha256 = plugin_ui_snapshot_sha256(
        payload.plugin_id.as_str(),
        payload.release_id.as_str(),
        payload.component_key.as_str(),
        ui.title.as_str(),
        ui.surface.as_str(),
        ui.relative_source_path.as_str(),
        ui.content_sha256.as_str(),
        ui.assets.as_slice(),
        PLUGIN_UI_BRIDGE_PROTOCOL_VERSION_V1,
        ui.bridge_capabilities.as_slice(),
        ui.artifact_mime_types.as_slice(),
        PLUGIN_UI_HOST_CSP_V1,
        PLUGIN_UI_IFRAME_SANDBOX_V1,
    )
    .map_err(|_| bad_gateway("Plugin UI snapshot hash 计算失败"))?;
    if ui.snapshot_sha256 != expected_snapshot_sha256 {
        return Err(bad_gateway("Plugin UI snapshot hash 不匹配"));
    }
    Ok(())
}

fn validate_bridge_capabilities(ui: &PluginUiSnapshot) -> Result<(), ApiError> {
    if ui.bridge_capabilities.len() > PLUGIN_UI_MAX_BRIDGE_CAPABILITIES {
        return Err(bad_gateway("Plugin UI bridge capability 超出限制"));
    }
    let allowed = [
        PLUGIN_UI_BRIDGE_CAPABILITY_HOST_CONTEXT_READ,
        PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_LIST,
        PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_READ,
        PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_DOWNLOAD,
        PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_CREATE,
        PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_UPDATE,
    ];
    let mut seen = BTreeSet::new();
    if ui
        .bridge_capabilities
        .iter()
        .any(|value| !allowed.contains(&value.as_str()) || !seen.insert(value.as_str()))
    {
        return Err(bad_gateway("Plugin UI bridge capability 无效"));
    }
    Ok(())
}

fn validate_artifact_mime_types(ui: &PluginUiSnapshot) -> Result<(), ApiError> {
    if ui.artifact_mime_types.len() > PLUGIN_UI_MAX_ARTIFACT_MIME_TYPES {
        return Err(bad_gateway("Plugin UI Artifact MIME allowlist 超出限制"));
    }
    let mut seen = BTreeSet::new();
    for media_type in &ui.artifact_mime_types {
        let valid = media_type.len() <= 128
            && media_type.split_once('/').is_some_and(|(kind, subtype)| {
                !kind.is_empty()
                    && !subtype.is_empty()
                    && kind.bytes().all(is_mime_token_byte)
                    && subtype.bytes().all(is_mime_token_byte)
            });
        if !valid || !seen.insert(media_type.as_str()) {
            return Err(bad_gateway("Plugin UI Artifact MIME allowlist 无效"));
        }
    }
    Ok(())
}

fn is_mime_token_byte(byte: u8) -> bool {
    byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'+' | b'.')
}

async fn request_local_connector_asset(
    auth: &AuthUser,
    ready: &PluginUiReadyEventPayload,
    relative_path: &str,
) -> Result<PluginUiAssetReadResponse, ApiError> {
    let config = Config::try_get().map_err(|_| service_unavailable("ChatOS 配置不可用"))?;
    let secret = config
        .local_connector_internal_api_secret
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| service_unavailable("Plugin UI Local Connector relay 未配置"))?;
    let token = chatos_service_runtime::issue_internal_service_token(
        secret,
        "chatos-backend",
        LOCAL_CONNECTOR_TOKEN_AUDIENCE,
        PLUGIN_UI_READ_SCOPE,
        60,
    )
    .map_err(|_| service_unavailable("Plugin UI relay token 生成失败"))?;
    let url = format!(
        "{}/api/local-connectors/relay/{}/plugins/ui/assets",
        config
            .local_connector_service_base_url
            .trim()
            .trim_end_matches('/'),
        urlencoding::encode(ready.device_id.as_str())
    );
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_millis(
            config.local_connector_service_request_timeout_ms.max(300) as u64,
        ))
        .build()
        .map_err(|_| service_unavailable("Plugin UI relay client 创建失败"))?;
    let mut request = client
        .post(url)
        .header("x-local-connector-caller", "chatos-backend")
        .header("x-local-connector-internal-token", token)
        .header("x-local-connector-owner-user-id", auth.user_id.as_str())
        .json(&json!({
            "run_id": ready.run_id,
            "plugin_id": ready.plugin_id,
            "release_id": ready.release_id,
            "artifact_sha256": ready.artifact_sha256,
            "component_key": ready.component_key,
            "adapter_session_id": ready.adapter_session_id,
            "ui_snapshot_sha256": ready.ui.snapshot_sha256,
            "relative_path": relative_path,
        }));
    if let Some(workspace_id) = ready.workspace_id.as_deref() {
        request = request.query(&[("workspace_id", workspace_id)]);
    }
    let response = request
        .send()
        .await
        .map_err(|_| bad_gateway("Plugin UI asset relay 不可用"))?;
    let status = response.status();
    let bytes = read_response_bytes_limited(response, PLUGIN_UI_RELAY_RESPONSE_LIMIT_BYTES)
        .await
        .map_err(|_| bad_gateway("Plugin UI asset relay 响应超出限制"))?;
    if !status.is_success() {
        return Err((
            map_relay_status(status),
            Json(json!({
                "error": "Plugin UI asset 当前不可用"
            })),
        ));
    }
    serde_json::from_slice(bytes.as_slice())
        .map_err(|_| bad_gateway("Plugin UI asset relay 响应格式无效"))
}

async fn request_local_connector_artifact_list(
    auth: &AuthUser,
    ready: &PluginUiReadyEventPayload,
    access: PluginArtifactUiAccess,
) -> Result<PluginArtifactListResponse, ApiError> {
    request_local_connector_artifact(
        auth,
        ready,
        "list",
        &PluginArtifactListRequest { access },
        PLUGIN_ARTIFACT_LIST_RELAY_RESPONSE_LIMIT_BYTES,
    )
    .await
}

async fn request_local_connector_artifact_read(
    auth: &AuthUser,
    ready: &PluginUiReadyEventPayload,
    access: PluginArtifactUiAccess,
    artifact_id: &str,
    mode: PluginArtifactReadMode,
) -> Result<PluginArtifactReadResponse, ApiError> {
    request_local_connector_artifact(
        auth,
        ready,
        "read",
        &PluginArtifactReadRequest {
            access,
            artifact_id: artifact_id.to_string(),
            mode,
        },
        PLUGIN_ARTIFACT_READ_RELAY_RESPONSE_LIMIT_BYTES,
    )
    .await
}

async fn request_local_connector_artifact_create(
    auth: &AuthUser,
    ready: &PluginUiReadyEventPayload,
    access: PluginArtifactUiAccess,
    display_name: &str,
    media_type: &str,
    body_base64: String,
) -> Result<PluginArtifactWriteResponse, ApiError> {
    request_local_connector_artifact(
        auth,
        ready,
        "create",
        &PluginArtifactCreateRequest {
            access,
            display_name: display_name.to_string(),
            media_type: media_type.to_string(),
            body_base64,
        },
        PLUGIN_ARTIFACT_WRITE_RELAY_RESPONSE_LIMIT_BYTES,
    )
    .await
}

async fn request_local_connector_artifact_update(
    auth: &AuthUser,
    ready: &PluginUiReadyEventPayload,
    access: PluginArtifactUiAccess,
    artifact_id: &str,
    expected_sha256: &str,
    body_base64: String,
) -> Result<PluginArtifactWriteResponse, ApiError> {
    request_local_connector_artifact(
        auth,
        ready,
        "update",
        &PluginArtifactUpdateRequest {
            access,
            artifact_id: artifact_id.to_string(),
            expected_sha256: expected_sha256.to_string(),
            body_base64,
        },
        PLUGIN_ARTIFACT_WRITE_RELAY_RESPONSE_LIMIT_BYTES,
    )
    .await
}

async fn request_local_connector_artifact<T, R>(
    auth: &AuthUser,
    ready: &PluginUiReadyEventPayload,
    action: &str,
    body: &T,
    response_limit_bytes: usize,
) -> Result<R, ApiError>
where
    T: Serialize + ?Sized,
    R: DeserializeOwned,
{
    let config = Config::try_get().map_err(|_| service_unavailable("ChatOS 配置不可用"))?;
    let secret = config
        .local_connector_internal_api_secret
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| service_unavailable("Plugin Artifact Local Connector relay 未配置"))?;
    let prepared = prepare_plugin_artifact_relay_request(
        auth.user_id.as_str(),
        ready,
        action,
        config.local_connector_service_base_url.as_str(),
        secret,
        config.local_connector_service_request_timeout_ms,
    );
    let prepared = prepared?;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(prepared.timeout)
        .build()
        .map_err(|_| service_unavailable("Plugin Artifact relay client 创建失败"))?;
    let response = client
        .post(prepared.url)
        .query(&[("workspace_id", prepared.workspace_id.as_str())])
        .header("x-local-connector-caller", "chatos-backend")
        .header("x-local-connector-internal-token", prepared.token)
        .header("x-local-connector-owner-user-id", prepared.owner_user_id)
        .json(body)
        .send()
        .await
        .map_err(|_| bad_gateway("Plugin Artifact relay 不可用"))?;
    let status = response.status();
    let bytes = read_response_bytes_limited(response, response_limit_bytes)
        .await
        .map_err(|_| bad_gateway("Plugin Artifact relay 响应超出限制"))?;
    if !status.is_success() {
        return Err((
            map_relay_status(status),
            Json(json!({"error": "Plugin Artifact 当前不可用"})),
        ));
    }
    serde_json::from_slice(bytes.as_slice())
        .map_err(|_| bad_gateway("Plugin Artifact relay 响应格式无效"))
}

fn prepare_plugin_artifact_relay_request(
    owner_user_id: &str,
    ready: &PluginUiReadyEventPayload,
    action: &str,
    service_base_url: &str,
    secret: &str,
    configured_timeout_ms: i64,
) -> Result<PreparedPluginArtifactRelayRequest, ApiError> {
    let (scope, minimum_timeout_ms) = match action {
        "list" | "read" => (PLUGIN_ARTIFACT_READ_SCOPE, 300),
        "create" | "update" => (
            PLUGIN_ARTIFACT_WRITE_SCOPE,
            PLUGIN_ARTIFACT_INTERACTIVE_RELAY_TIMEOUT_MS,
        ),
        _ => return Err(bad_gateway("Plugin Artifact relay action 无效")),
    };
    let workspace_id = ready
        .workspace_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| bad_gateway("Plugin Artifact workspace identity 缺失"))?
        .to_string();
    let owner_user_id = owner_user_id.trim();
    if owner_user_id.is_empty() {
        return Err(bad_gateway("Plugin Artifact owner identity 缺失"));
    }
    let secret = secret.trim();
    if secret.is_empty() {
        return Err(service_unavailable(
            "Plugin Artifact Local Connector relay 未配置",
        ));
    }
    let base_url = service_base_url.trim().trim_end_matches('/');
    if base_url.is_empty() {
        return Err(service_unavailable(
            "Plugin Artifact Local Connector relay 未配置",
        ));
    }
    let token = chatos_service_runtime::issue_internal_service_token(
        secret,
        "chatos-backend",
        LOCAL_CONNECTOR_TOKEN_AUDIENCE,
        scope,
        60,
    )
    .map_err(|_| service_unavailable("Plugin Artifact relay token 生成失败"))?;
    Ok(PreparedPluginArtifactRelayRequest {
        url: format!(
            "{base_url}/api/local-connectors/relay/{}/plugins/artifacts/{action}",
            urlencoding::encode(ready.device_id.as_str())
        ),
        workspace_id,
        owner_user_id: owner_user_id.to_string(),
        token,
        timeout: Duration::from_millis(configured_timeout_ms.max(minimum_timeout_ms) as u64),
    })
}

#[cfg(feature = "test-support")]
pub fn prepare_plugin_artifact_relay_request_for_test(
    owner_user_id: &str,
    ready: &PluginUiReadyEventPayload,
    action: &str,
    service_base_url: &str,
    secret: &str,
    configured_timeout_ms: i64,
) -> Result<PreparedPluginArtifactRelayRequest, String> {
    prepare_plugin_artifact_relay_request(
        owner_user_id,
        ready,
        action,
        service_base_url,
        secret,
        configured_timeout_ms,
    )
    .map_err(|(_, Json(body))| {
        body.get("error")
            .and_then(Value::as_str)
            .unwrap_or("prepare Plugin Artifact relay request failed")
            .to_string()
    })
}

#[cfg(feature = "test-support")]
pub fn validate_plugin_artifact_list_response_for_test(
    owner_user_id: &str,
    ready: &PluginUiReadyEventPayload,
    access: &PluginArtifactUiAccess,
    response: &PluginArtifactListResponse,
) -> Result<(), String> {
    let auth = AuthUser {
        user_id: owner_user_id.to_string(),
        role: "user".to_string(),
    };
    validate_artifact_list_response(&auth, ready, access, response)
        .map_err(plugin_artifact_test_error)
}

#[cfg(feature = "test-support")]
pub fn validate_plugin_artifact_read_response_for_test(
    owner_user_id: &str,
    ready: &PluginUiReadyEventPayload,
    access: &PluginArtifactUiAccess,
    artifact_id: &str,
    response: &PluginArtifactReadResponse,
) -> Result<(), String> {
    let auth = AuthUser {
        user_id: owner_user_id.to_string(),
        role: "user".to_string(),
    };
    validate_artifact_read_response(&auth, ready, access, artifact_id, response)
        .map_err(plugin_artifact_test_error)
}

#[cfg(feature = "test-support")]
#[allow(clippy::too_many_arguments)]
pub fn validate_plugin_artifact_write_response_for_test(
    owner_user_id: &str,
    ready: &PluginUiReadyEventPayload,
    access: &PluginArtifactUiAccess,
    operation: PluginArtifactWriteOperation,
    expected_artifact_id: Option<&str>,
    expected_create_metadata: Option<(&str, &str)>,
    expected_body: &[u8],
    response: &PluginArtifactWriteResponse,
) -> Result<(), String> {
    let auth = AuthUser {
        user_id: owner_user_id.to_string(),
        role: "user".to_string(),
    };
    validate_artifact_write_response(
        &auth,
        ready,
        access,
        operation,
        expected_artifact_id,
        expected_create_metadata,
        expected_body,
        response,
    )
    .map_err(plugin_artifact_test_error)
}

#[cfg(feature = "test-support")]
fn plugin_artifact_test_error((_, Json(body)): ApiError) -> String {
    body.get("error")
        .and_then(Value::as_str)
        .unwrap_or("Plugin Artifact response validation failed")
        .to_string()
}

fn validate_artifact_list_response(
    auth: &AuthUser,
    ready: &PluginUiReadyEventPayload,
    access: &PluginArtifactUiAccess,
    response: &PluginArtifactListResponse,
) -> Result<(), ApiError> {
    if &response.access != access || response.artifacts.len() > 1_024 {
        return Err(bad_gateway("Plugin Artifact list identity 不匹配"));
    }
    let mut artifact_ids = BTreeSet::new();
    for artifact in &response.artifacts {
        if !artifact_ids.insert(artifact.artifact_id.as_str()) {
            return Err(bad_gateway("Plugin Artifact list 包含重复项目"));
        }
        validate_artifact_descriptor(auth, ready, artifact)?;
    }
    Ok(())
}

fn validate_artifact_read_response(
    auth: &AuthUser,
    ready: &PluginUiReadyEventPayload,
    access: &PluginArtifactUiAccess,
    artifact_id: &str,
    response: &PluginArtifactReadResponse,
) -> Result<(), ApiError> {
    if &response.access != access || response.artifact.artifact_id != artifact_id {
        return Err(bad_gateway("Plugin Artifact read identity 不匹配"));
    }
    validate_artifact_descriptor(auth, ready, &response.artifact)?;
    let max_base64_bytes =
        ((response.artifact.size_bytes as usize).saturating_add(2) / 3).saturating_mul(4);
    if response.body_base64.len() > max_base64_bytes {
        return Err(bad_gateway("Plugin Artifact body 超出声明大小"));
    }
    let bytes = BASE64_STANDARD
        .decode(response.body_base64.as_bytes())
        .map_err(|_| bad_gateway("Plugin Artifact body 编码无效"))?;
    if bytes.len() as u64 != response.artifact.size_bytes
        || hex::encode(Sha256::digest(bytes.as_slice())) != response.artifact.sha256
    {
        return Err(bad_gateway("Plugin Artifact body checksum 不匹配"));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_artifact_write_response(
    auth: &AuthUser,
    ready: &PluginUiReadyEventPayload,
    access: &PluginArtifactUiAccess,
    operation: PluginArtifactWriteOperation,
    expected_artifact_id: Option<&str>,
    expected_create_metadata: Option<(&str, &str)>,
    expected_body: &[u8],
    response: &PluginArtifactWriteResponse,
) -> Result<(), ApiError> {
    if &response.access != access || response.operation != operation {
        return Err(bad_gateway("Plugin Artifact write identity 不匹配"));
    }
    validate_artifact_descriptor(auth, ready, &response.artifact)?;
    let artifact = &response.artifact;
    if !artifact.mutable
        || artifact.owner.component_key != ready.component_key
        || artifact.owner.adapter_session_id != ready.adapter_session_id
        || artifact.size_bytes != expected_body.len() as u64
        || artifact.sha256 != hex::encode(Sha256::digest(expected_body))
    {
        return Err(bad_gateway("Plugin Artifact write descriptor 无效"));
    }
    match operation {
        PluginArtifactWriteOperation::Create => {
            let Some((display_name, media_type)) = expected_create_metadata else {
                return Err(bad_gateway(
                    "Plugin Artifact create validation metadata 缺失",
                ));
            };
            if expected_artifact_id.is_some()
                || artifact.display_name != display_name
                || artifact.media_type != media_type
                || artifact.producer_tool_name != PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_CREATE
            {
                return Err(bad_gateway("Plugin Artifact create response 不匹配"));
            }
        }
        PluginArtifactWriteOperation::Update => {
            if expected_create_metadata.is_some()
                || expected_artifact_id != Some(artifact.artifact_id.as_str())
                || artifact.producer_tool_name != PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_UPDATE
            {
                return Err(bad_gateway("Plugin Artifact update response 不匹配"));
            }
        }
    }
    Ok(())
}

fn validate_artifact_descriptor(
    auth: &AuthUser,
    ready: &PluginUiReadyEventPayload,
    artifact: &PluginArtifactDescriptor,
) -> Result<(), ApiError> {
    let owner = &artifact.owner;
    let workspace_id = ready.workspace_id.as_deref().unwrap_or_default();
    let path = FsPath::new(artifact.workspace_relative_path.as_str());
    if owner.owner_user_id != auth.user_id
        || owner.run_id != ready.run_id
        || owner.device_id != ready.device_id
        || owner.workspace_id != workspace_id
        || owner.plugin_id != ready.plugin_id
        || owner.release_id != ready.release_id
        || owner.artifact_sha256 != ready.artifact_sha256
        || owner.component_key.trim().is_empty()
        || owner.adapter_session_id.trim().is_empty()
        || normalize_plugin_artifact_id(artifact.artifact_id.as_str()).is_err()
        || artifact.workspace_relative_path.len() > 4_096
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        || path.file_name().and_then(|value| value.to_str()) != Some(artifact.display_name.as_str())
        || !ready
            .ui
            .artifact_mime_types
            .iter()
            .any(|media_type| media_type == &artifact.media_type)
        || artifact.size_bytes > PLUGIN_ARTIFACT_MAX_BYTES
        || !is_lower_sha256(artifact.sha256.as_str())
        || artifact.producer_tool_name.trim().is_empty()
        || chrono::DateTime::parse_from_rfc3339(artifact.created_at.as_str()).is_err()
        || !artifact.downloadable
    {
        return Err(bad_gateway("Plugin Artifact descriptor 无效"));
    }
    if artifact.mutable
        && (owner.component_key != ready.component_key
            || owner.adapter_session_id != ready.adapter_session_id
            || artifact.size_bytes > PLUGIN_ARTIFACT_WRITE_MAX_BYTES
            || !matches!(
                artifact.producer_tool_name.as_str(),
                PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_CREATE
                    | PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_UPDATE
            ))
    {
        return Err(bad_gateway("Plugin Artifact mutable descriptor 无效"));
    }
    Ok(())
}

fn plugin_artifact_download_response(
    artifact: PluginArtifactReadResponse,
) -> Result<Response, ApiError> {
    let bytes = BASE64_STANDARD
        .decode(artifact.body_base64.as_bytes())
        .map_err(|_| bad_gateway("Plugin Artifact body 编码无效"))?;
    let mut response = Response::new(Body::from(bytes));
    *response.status_mut() = StatusCode::OK;
    let headers = response.headers_mut();
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_str(artifact.artifact.media_type.as_str())
            .map_err(|_| bad_gateway("Plugin Artifact Content-Type 无效"))?,
    );
    headers.insert(
        CONTENT_LENGTH,
        HeaderValue::from_str(artifact.artifact.size_bytes.to_string().as_str())
            .map_err(|_| bad_gateway("Plugin Artifact Content-Length 无效"))?,
    );
    headers.insert(
        CONTENT_DISPOSITION,
        safe_content_disposition(artifact.artifact.display_name.as_str())?,
    );
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    headers.insert(
        HeaderName::from_static("x-content-type-options"),
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        HeaderName::from_static("cross-origin-resource-policy"),
        HeaderValue::from_static("same-origin"),
    );
    Ok(response)
}

fn safe_content_disposition(filename: &str) -> Result<HeaderValue, ApiError> {
    let ascii = filename
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_' | ' ') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    let ascii = if ascii.trim().is_empty() {
        "artifact".to_string()
    } else {
        ascii
    };
    let value = format!(
        "attachment; filename=\"{}\"; filename*=UTF-8''{}",
        ascii.replace('"', "_"),
        urlencoding::encode(filename)
    );
    HeaderValue::from_str(value.as_str())
        .map_err(|_| bad_gateway("Plugin Artifact Content-Disposition 无效"))
}

fn normalize_plugin_artifact_id(artifact_id: &str) -> Result<&str, ApiError> {
    if artifact_id.len() != 35
        || !artifact_id.starts_with("pa_")
        || !artifact_id[3..]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(not_found("Plugin Artifact 不存在"));
    }
    Ok(artifact_id)
}

fn validate_asset_response(
    auth: &AuthUser,
    ready: &PluginUiReadyEventPayload,
    relative_path: &str,
    response: &PluginUiAssetReadResponse,
) -> Result<(), ApiError> {
    if response.run_id != ready.run_id
        || response.owner_user_id != auth.user_id
        || response.plugin_id != ready.plugin_id
        || response.release_id != ready.release_id
        || response.artifact_sha256 != ready.artifact_sha256
        || response.component_key != ready.component_key
        || response.adapter_session_id != ready.adapter_session_id
        || response.ui_snapshot_sha256 != ready.ui.snapshot_sha256
        || response.relative_path != relative_path
    {
        return Err(bad_gateway("Plugin UI asset session identity 不匹配"));
    }
    let (expected_kind, expected_media_type, expected_size, expected_sha256, max_bytes) =
        if relative_path == ready.ui.relative_source_path {
            (
                PluginUiAssetKind::Entrypoint,
                "text/html; charset=utf-8",
                None,
                ready.ui.content_sha256.as_str(),
                PLUGIN_UI_ENTRYPOINT_MAX_BYTES,
            )
        } else {
            let asset = ready
                .ui
                .assets
                .iter()
                .find(|asset| asset.relative_path == relative_path)
                .ok_or_else(|| not_found("Plugin UI asset 未在 Run snapshot 中声明"))?;
            (
                PluginUiAssetKind::StaticAsset,
                asset.media_type.as_str(),
                Some(asset.size_bytes),
                asset.sha256.as_str(),
                PLUGIN_UI_ASSET_MAX_BYTES,
            )
        };
    if response.kind != expected_kind
        || response.media_type != expected_media_type
        || response.sha256 != expected_sha256
        || expected_size.is_some_and(|size| size != response.size_bytes)
        || response.size_bytes > max_bytes
    {
        return Err(bad_gateway("Plugin UI asset metadata 不匹配"));
    }
    let max_base64_bytes = ((max_bytes as usize).saturating_add(2) / 3).saturating_mul(4);
    if response.body_base64.len() > max_base64_bytes {
        return Err(bad_gateway("Plugin UI asset body 超出限制"));
    }
    let bytes = BASE64_STANDARD
        .decode(response.body_base64.as_bytes())
        .map_err(|_| bad_gateway("Plugin UI asset body 编码无效"))?;
    if bytes.len() as u64 != response.size_bytes
        || hex::encode(Sha256::digest(bytes.as_slice())) != response.sha256
    {
        return Err(bad_gateway("Plugin UI asset body checksum 不匹配"));
    }
    Ok(())
}

fn plugin_ui_asset_response(
    ui: &PluginUiSnapshot,
    asset: PluginUiAssetReadResponse,
    parent_origin: Option<&str>,
) -> Result<Response, ApiError> {
    let bytes = BASE64_STANDARD
        .decode(asset.body_base64.as_bytes())
        .map_err(|_| bad_gateway("Plugin UI asset body 编码无效"))?;
    let mut response = Response::new(Body::from(bytes));
    *response.status_mut() = StatusCode::OK;
    let headers = response.headers_mut();
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_str(asset.media_type.as_str())
            .map_err(|_| bad_gateway("Plugin UI asset Content-Type 无效"))?,
    );
    headers.insert(
        CONTENT_LENGTH,
        HeaderValue::from_str(asset.size_bytes.to_string().as_str())
            .map_err(|_| bad_gateway("Plugin UI asset Content-Length 无效"))?,
    );
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    headers.insert(
        HeaderName::from_static("cross-origin-resource-policy"),
        HeaderValue::from_static("same-origin"),
    );
    headers.insert(
        HeaderName::from_static("x-content-type-options"),
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        HeaderName::from_static("permissions-policy"),
        HeaderValue::from_static(
            "camera=(), microphone=(), geolocation=(), display-capture=(), clipboard-read=(), clipboard-write=()",
        ),
    );
    if asset.kind == PluginUiAssetKind::Entrypoint {
        let content_security_policy = plugin_ui_response_content_security_policy(
            ui.content_security_policy.as_str(),
            parent_origin,
        )?;
        headers.insert(
            CONTENT_SECURITY_POLICY,
            HeaderValue::from_str(content_security_policy.as_str())
                .map_err(|_| bad_gateway("Plugin UI CSP 无效"))?,
        );
        headers.insert(
            HeaderName::from_static("origin-agent-cluster"),
            HeaderValue::from_static("?1"),
        );
    }
    Ok(response)
}

fn plugin_ui_response_content_security_policy(
    immutable_csp: &str,
    parent_origin: Option<&str>,
) -> Result<String, ApiError> {
    if immutable_csp != PLUGIN_UI_HOST_CSP_V1 {
        return Err(bad_gateway("Plugin UI immutable CSP 无效"));
    }
    let Some(parent_origin) = parent_origin else {
        return Ok(immutable_csp.to_string());
    };
    if parent_origin.is_empty()
        || parent_origin.bytes().any(|byte| byte.is_ascii_whitespace())
        || parent_origin.contains(';')
        || parent_origin.contains('\'')
        || parent_origin.contains('"')
    {
        return Err(service_unavailable("Plugin UI parent origin 配置无效"));
    }
    let marker = "frame-ancestors 'self'";
    if immutable_csp.matches(marker).count() != 1 {
        return Err(bad_gateway("Plugin UI frame ancestor policy 无效"));
    }
    Ok(immutable_csp.replace(marker, format!("frame-ancestors {parent_origin}").as_str()))
}

fn normalize_requested_asset_path(path: &str) -> Result<String, ApiError> {
    let path = path.trim().trim_start_matches('/');
    if path.is_empty()
        || path.len() > 1024
        || path.contains('\0')
        || path.contains('\\')
        || path.split('/').any(|segment| {
            segment.is_empty() || segment == "." || segment == ".." || segment.len() > 255
        })
    {
        return Err(not_found("Plugin UI asset 路径无效"));
    }
    let relative_path = format!("./{path}");
    if !is_safe_ui_path(relative_path.as_str(), path.ends_with(".html")) {
        return Err(not_found("Plugin UI asset 路径无效"));
    }
    Ok(relative_path)
}

fn is_safe_ui_path(path: &str, html: bool) -> bool {
    if !path.starts_with("./ui/")
        || path.len() > 1024
        || path.contains('\0')
        || path.contains('\\')
        || path
            .trim_start_matches("./")
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return false;
    }
    if html {
        path.to_ascii_lowercase().ends_with(".html")
    } else {
        expected_media_type(path).is_some()
    }
}

fn expected_media_type(path: &str) -> Option<&'static str> {
    let extension = path.rsplit_once('.')?.1.to_ascii_lowercase();
    match extension.as_str() {
        "js" | "mjs" => Some("text/javascript"),
        "css" => Some("text/css"),
        "json" => Some("application/json"),
        "svg" => Some("image/svg+xml"),
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "webp" => Some("image/webp"),
        "gif" => Some("image/gif"),
        "woff" => Some("font/woff"),
        "woff2" => Some("font/woff2"),
        _ => None,
    }
}

fn is_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn map_relay_status(status: reqwest::StatusCode) -> StatusCode {
    StatusCode::from_u16(status.as_u16())
        .ok()
        .filter(|status| {
            matches!(
                *status,
                StatusCode::BAD_REQUEST
                    | StatusCode::FORBIDDEN
                    | StatusCode::NOT_FOUND
                    | StatusCode::PAYLOAD_TOO_LARGE
                    | StatusCode::CONFLICT
                    | StatusCode::GONE
                    | StatusCode::SERVICE_UNAVAILABLE
            )
        })
        .unwrap_or(StatusCode::BAD_GATEWAY)
}

fn not_found(message: &str) -> ApiError {
    (StatusCode::NOT_FOUND, Json(json!({ "error": message })))
}

fn bad_request(message: &str) -> ApiError {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": message })))
}

fn bad_gateway(message: &str) -> ApiError {
    (StatusCode::BAD_GATEWAY, Json(json!({ "error": message })))
}

fn service_unavailable(message: &str) -> ApiError {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({ "error": message })),
    )
}

#[cfg(test)]
mod tests {
    use super::{
        get_plugin_ui_workbench_session, issue_plugin_ui_workbench_session,
        lock_workbench_sessions, normalize_requested_asset_path, plugin_artifact_download_response,
        plugin_ui_asset_response, plugin_ui_response_content_security_policy,
        prepare_plugin_artifact_relay_request, require_workbench_capability,
        validate_artifact_read_response, validate_artifact_write_response, validate_asset_response,
        validate_ready_payload,
    };
    use crate::core::auth::AuthUser;
    use axum::http::header::{
        CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_SECURITY_POLICY, REFERRER_POLICY,
    };
    use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
    use base64::Engine;
    use chatos_plugin_management_sdk::{
        plugin_ui_snapshot_sha256, PluginArtifactDescriptor, PluginArtifactOwner,
        PluginArtifactReadResponse, PluginArtifactUiAccess, PluginArtifactWriteOperation,
        PluginArtifactWriteResponse, PluginUiAssetKind, PluginUiAssetReadResponse,
        PluginUiAssetSnapshot, PluginUiReadyEventPayload, PluginUiSnapshot,
        PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_CREATE, PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_UPDATE,
        PLUGIN_UI_BRIDGE_PROTOCOL_VERSION_V1, PLUGIN_UI_HOST_CSP_V1, PLUGIN_UI_IFRAME_SANDBOX_V1,
        PLUGIN_UI_READY_EVENT_VERSION_V1,
    };
    use sha2::{Digest, Sha256};

    fn ready_payload() -> PluginUiReadyEventPayload {
        let html = b"<!doctype html><script src=\"app.js\"></script>";
        let script = b"window.parent.postMessage({type:'ready'}, '*');";
        let content_sha256 = hex::encode(Sha256::digest(html));
        let assets = vec![PluginUiAssetSnapshot {
            relative_path: "./ui/app.js".to_string(),
            media_type: "text/javascript".to_string(),
            size_bytes: script.len() as u64,
            sha256: hex::encode(Sha256::digest(script)),
        }];
        let snapshot_sha256 = plugin_ui_snapshot_sha256(
            "plugin-1",
            "release-1",
            "workbench",
            "Workbench",
            "workbench",
            "./ui/index.html",
            content_sha256.as_str(),
            assets.as_slice(),
            PLUGIN_UI_BRIDGE_PROTOCOL_VERSION_V1,
            &[
                "host.context.read".to_string(),
                "artifact.list".to_string(),
                "artifact.read".to_string(),
                "artifact.download".to_string(),
                PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_CREATE.to_string(),
                PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_UPDATE.to_string(),
            ],
            &["application/pdf".to_string()],
            PLUGIN_UI_HOST_CSP_V1,
            PLUGIN_UI_IFRAME_SANDBOX_V1,
        )
        .expect("snapshot hash");
        PluginUiReadyEventPayload {
            event_schema_version: PLUGIN_UI_READY_EVENT_VERSION_V1,
            run_id: "run-1".to_string(),
            device_id: "device-1".to_string(),
            workspace_id: Some("workspace-1".to_string()),
            plugin_id: "plugin-1".to_string(),
            release_id: "release-1".to_string(),
            artifact_sha256: "b".repeat(64),
            component_key: "workbench".to_string(),
            adapter_session_id: "adapter-1".to_string(),
            ui: PluginUiSnapshot {
                plugin_id: "plugin-1".to_string(),
                release_id: "release-1".to_string(),
                version: "1.0.0".to_string(),
                artifact_sha256: "b".repeat(64),
                component_key: "workbench".to_string(),
                title: "Workbench".to_string(),
                surface: "workbench".to_string(),
                relative_source_path: "./ui/index.html".to_string(),
                content_sha256,
                assets,
                bridge_protocol_version: PLUGIN_UI_BRIDGE_PROTOCOL_VERSION_V1,
                bridge_capabilities: vec![
                    "host.context.read".to_string(),
                    "artifact.list".to_string(),
                    "artifact.read".to_string(),
                    "artifact.download".to_string(),
                    PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_CREATE.to_string(),
                    PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_UPDATE.to_string(),
                ],
                artifact_mime_types: vec!["application/pdf".to_string()],
                content_security_policy: PLUGIN_UI_HOST_CSP_V1.to_string(),
                iframe_sandbox: PLUGIN_UI_IFRAME_SANDBOX_V1.to_string(),
                snapshot_sha256,
            },
        }
    }

    #[test]
    fn ready_event_and_asset_response_are_revalidated_end_to_end() {
        let ready = ready_payload();
        validate_ready_payload(&ready).expect("valid ready payload");
        let body = b"window.parent.postMessage({type:'ready'}, '*');";
        let response = PluginUiAssetReadResponse {
            run_id: ready.run_id.clone(),
            owner_user_id: "user-1".to_string(),
            plugin_id: ready.plugin_id.clone(),
            release_id: ready.release_id.clone(),
            artifact_sha256: ready.artifact_sha256.clone(),
            component_key: ready.component_key.clone(),
            adapter_session_id: ready.adapter_session_id.clone(),
            ui_snapshot_sha256: ready.ui.snapshot_sha256.clone(),
            kind: PluginUiAssetKind::StaticAsset,
            relative_path: "./ui/app.js".to_string(),
            media_type: "text/javascript".to_string(),
            size_bytes: body.len() as u64,
            sha256: hex::encode(Sha256::digest(body)),
            body_base64: BASE64_STANDARD.encode(body),
        };
        validate_asset_response(
            &AuthUser {
                user_id: "user-1".to_string(),
                role: "user".to_string(),
            },
            &ready,
            "./ui/app.js",
            &response,
        )
        .expect("valid asset response");

        let mut tampered = response;
        tampered.body_base64 = BASE64_STANDARD.encode(b"tampered");
        assert!(validate_asset_response(
            &AuthUser {
                user_id: "user-1".to_string(),
                role: "user".to_string(),
            },
            &ready,
            "./ui/app.js",
            &tampered,
        )
        .is_err());
    }

    #[test]
    fn asset_url_path_is_canonical_and_traversal_safe() {
        assert_eq!(
            normalize_requested_asset_path("ui/app.js").expect("valid path"),
            "./ui/app.js"
        );
        for path in ["", "../secret", "ui/../secret", "ui\\app.js", "ui/app.exe"] {
            assert!(normalize_requested_asset_path(path).is_err(), "{path}");
        }
    }

    #[test]
    fn workbench_session_is_short_lived_snapshot_bound_and_revocable() {
        let auth = AuthUser {
            user_id: format!("user-{}", uuid::Uuid::new_v4()),
            role: "user".to_string(),
        };
        let ready = ready_payload();
        validate_ready_payload(&ready).expect("valid ready payload");
        let response =
            issue_plugin_ui_workbench_session(&auth, "message-1", "event-1", ready.clone(), None)
                .expect("issue workbench session");
        assert_eq!(response.expires_in, 300);
        assert!(response
            .iframe_path
            .starts_with("/api/plugin-ui/workbench/pui_"));
        assert!(response.iframe_path.contains("/ui/index.html#"));
        assert!(!response.iframe_path.contains("<!doctype"));
        assert_eq!(response.host_context.run_id, ready.run_id);

        let stored = get_plugin_ui_workbench_session(response.session_id.as_str())
            .expect("read workbench session");
        assert_eq!(stored.owner_user_id, auth.user_id);
        assert_eq!(stored.ready.ui.snapshot_sha256, ready.ui.snapshot_sha256);

        lock_workbench_sessions()
            .expect("lock workbench sessions")
            .remove(response.session_id.as_str());
        assert!(get_plugin_ui_workbench_session(response.session_id.as_str()).is_err());
    }

    #[test]
    fn entrypoint_response_enforces_opaque_workbench_security_headers() {
        let ready = ready_payload();
        let body = b"<!doctype html><script src=\"app.js\"></script>";
        let response = plugin_ui_asset_response(
            &ready.ui,
            PluginUiAssetReadResponse {
                run_id: ready.run_id,
                owner_user_id: "user-1".to_string(),
                plugin_id: ready.plugin_id,
                release_id: ready.release_id,
                artifact_sha256: ready.artifact_sha256,
                component_key: ready.component_key,
                adapter_session_id: ready.adapter_session_id,
                ui_snapshot_sha256: ready.ui.snapshot_sha256.clone(),
                kind: PluginUiAssetKind::Entrypoint,
                relative_path: ready.ui.relative_source_path.clone(),
                media_type: "text/html; charset=utf-8".to_string(),
                size_bytes: body.len() as u64,
                sha256: ready.ui.content_sha256.clone(),
                body_base64: BASE64_STANDARD.encode(body),
            },
            None,
        )
        .expect("entrypoint response");
        assert_eq!(response.headers()[CACHE_CONTROL], "no-store");
        assert_eq!(response.headers()[REFERRER_POLICY], "no-referrer");
        assert_eq!(
            response.headers()["cross-origin-resource-policy"],
            "same-origin"
        );
        assert_eq!(
            response.headers()[CONTENT_SECURITY_POLICY],
            ready.ui.content_security_policy.as_str()
        );
        assert!(!ready.ui.iframe_sandbox.contains("allow-same-origin"));
    }

    #[test]
    fn workbench_resource_origin_is_absolute_and_csp_is_parent_exact() {
        let auth = AuthUser {
            user_id: format!("user-{}", uuid::Uuid::new_v4()),
            role: "user".to_string(),
        };
        let response = issue_plugin_ui_workbench_session(
            &auth,
            "message-origin",
            "event-origin",
            ready_payload(),
            Some("https://plugin-ui.example.com"),
        )
        .expect("issue cross-origin workbench session");
        assert!(response.iframe_path.starts_with(&format!(
            "https://plugin-ui.example.com/api/plugin-ui/workbench/{}/",
            response.session_id
        )));
        let csp = plugin_ui_response_content_security_policy(
            PLUGIN_UI_HOST_CSP_V1,
            Some("https://app.example.com"),
        )
        .expect("derive exact parent CSP");
        assert!(csp.contains("frame-ancestors https://app.example.com"));
        assert!(!csp.contains("frame-ancestors 'self'"));
        assert!(plugin_ui_response_content_security_policy(
            PLUGIN_UI_HOST_CSP_V1,
            Some("https://app.example.com; frame-src *"),
        )
        .is_err());

        lock_workbench_sessions()
            .expect("lock workbench sessions")
            .remove(response.session_id.as_str());
    }

    #[test]
    fn artifact_read_is_owner_bound_and_download_headers_are_safe() {
        let ready = ready_payload();
        let auth = AuthUser {
            user_id: "user-1".to_string(),
            role: "user".to_string(),
        };
        let access = PluginArtifactUiAccess {
            run_id: ready.run_id.clone(),
            plugin_id: ready.plugin_id.clone(),
            release_id: ready.release_id.clone(),
            artifact_sha256: ready.artifact_sha256.clone(),
            component_key: ready.component_key.clone(),
            adapter_session_id: ready.adapter_session_id.clone(),
            ui_snapshot_sha256: ready.ui.snapshot_sha256.clone(),
        };
        let body = b"%PDF-fixture";
        let artifact = PluginArtifactDescriptor {
            artifact_id: format!("pa_{}", "c".repeat(32)),
            owner: PluginArtifactOwner {
                owner_user_id: auth.user_id.clone(),
                run_id: ready.run_id.clone(),
                device_id: ready.device_id.clone(),
                workspace_id: ready.workspace_id.clone().expect("workspace"),
                plugin_id: ready.plugin_id.clone(),
                release_id: ready.release_id.clone(),
                artifact_sha256: ready.artifact_sha256.clone(),
                component_key: "documents".to_string(),
                adapter_session_id: "producer-session".to_string(),
            },
            workspace_relative_path: "artifacts/report.pdf".to_string(),
            display_name: "报告 \"final\".pdf".to_string(),
            media_type: "application/pdf".to_string(),
            size_bytes: body.len() as u64,
            sha256: hex::encode(Sha256::digest(body)),
            created_at: "2026-07-26T00:00:00Z".to_string(),
            producer_tool_name: "create_text_pdf".to_string(),
            downloadable: true,
            mutable: false,
        };
        let response = PluginArtifactReadResponse {
            access: access.clone(),
            artifact: artifact.clone(),
            body_base64: BASE64_STANDARD.encode(body),
        };
        assert!(validate_artifact_read_response(
            &auth,
            &ready,
            &access,
            artifact.artifact_id.as_str(),
            &response,
        )
        .is_err());

        let mut safe = response;
        safe.artifact.display_name = "report.pdf".to_string();
        validate_artifact_read_response(
            &auth,
            &ready,
            &access,
            artifact.artifact_id.as_str(),
            &safe,
        )
        .expect("valid Artifact response");
        let download = plugin_artifact_download_response(safe).expect("download response");
        assert_eq!(download.headers()[CACHE_CONTROL], "no-store");
        assert!(download.headers()[CONTENT_DISPOSITION]
            .to_str()
            .expect("Content-Disposition")
            .starts_with("attachment;"));
    }

    #[test]
    fn artifact_write_is_capability_exact_ui_owner_and_body_bound() {
        let ready = ready_payload();
        let auth = AuthUser {
            user_id: "user-1".to_string(),
            role: "user".to_string(),
        };
        let issued = issue_plugin_ui_workbench_session(
            &auth,
            "message-write",
            "event-write",
            ready.clone(),
            None,
        )
        .expect("issue writable workbench session");
        let session = get_plugin_ui_workbench_session(issued.session_id.as_str())
            .expect("read writable workbench session");
        require_workbench_capability(&session, PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_CREATE)
            .expect("create capability");

        let access = PluginArtifactUiAccess {
            run_id: ready.run_id.clone(),
            plugin_id: ready.plugin_id.clone(),
            release_id: ready.release_id.clone(),
            artifact_sha256: ready.artifact_sha256.clone(),
            component_key: ready.component_key.clone(),
            adapter_session_id: ready.adapter_session_id.clone(),
            ui_snapshot_sha256: ready.ui.snapshot_sha256.clone(),
        };
        let body = b"%PDF-plugin-draft";
        let artifact_id = format!("pa_{}", "e".repeat(32));
        let artifact = PluginArtifactDescriptor {
            artifact_id: artifact_id.clone(),
            owner: PluginArtifactOwner {
                owner_user_id: auth.user_id.clone(),
                run_id: ready.run_id.clone(),
                device_id: ready.device_id.clone(),
                workspace_id: ready.workspace_id.clone().expect("workspace"),
                plugin_id: ready.plugin_id.clone(),
                release_id: ready.release_id.clone(),
                artifact_sha256: ready.artifact_sha256.clone(),
                component_key: ready.component_key.clone(),
                adapter_session_id: ready.adapter_session_id.clone(),
            },
            workspace_relative_path: format!(
                "chatos-plugin-artifacts/opaque/{artifact_id}/draft.pdf"
            ),
            display_name: "draft.pdf".to_string(),
            media_type: "application/pdf".to_string(),
            size_bytes: body.len() as u64,
            sha256: hex::encode(Sha256::digest(body)),
            created_at: "2026-07-26T00:00:00Z".to_string(),
            producer_tool_name: PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_CREATE.to_string(),
            downloadable: true,
            mutable: true,
        };
        let response = PluginArtifactWriteResponse {
            access: access.clone(),
            operation: PluginArtifactWriteOperation::Create,
            artifact: artifact.clone(),
        };
        validate_artifact_write_response(
            &auth,
            &ready,
            &access,
            PluginArtifactWriteOperation::Create,
            None,
            Some(("draft.pdf", "application/pdf")),
            body,
            &response,
        )
        .expect("valid mutable Artifact create response");

        let mut wrong_owner = response.clone();
        wrong_owner.artifact.owner.adapter_session_id = "other-session".to_string();
        assert!(validate_artifact_write_response(
            &auth,
            &ready,
            &access,
            PluginArtifactWriteOperation::Create,
            None,
            Some(("draft.pdf", "application/pdf")),
            body,
            &wrong_owner,
        )
        .is_err());

        let updated_body = b"%PDF-plugin-draft-v2";
        let mut updated = artifact;
        updated.size_bytes = updated_body.len() as u64;
        updated.sha256 = hex::encode(Sha256::digest(updated_body));
        updated.producer_tool_name = PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_UPDATE.to_string();
        validate_artifact_write_response(
            &auth,
            &ready,
            &access,
            PluginArtifactWriteOperation::Update,
            Some(artifact_id.as_str()),
            None,
            updated_body,
            &PluginArtifactWriteResponse {
                access: access.clone(),
                operation: PluginArtifactWriteOperation::Update,
                artifact: updated,
            },
        )
        .expect("valid mutable Artifact update response");

        lock_workbench_sessions()
            .expect("lock workbench sessions")
            .remove(issued.session_id.as_str());
    }

    #[test]
    fn artifact_relay_request_is_signed_routed_and_timeout_bound() {
        let auth = AuthUser {
            user_id: "user-1".to_string(),
            role: "user".to_string(),
        };
        let mut ready = ready_payload();
        ready.device_id = "device /一".to_string();
        let secret = "a-long-chatos-local-connector-secret";

        let read = prepare_plugin_artifact_relay_request(
            auth.user_id.as_str(),
            &ready,
            "read",
            " https://connector.example.test/ ",
            secret,
            100,
        )
        .expect("prepare read relay request");
        assert_eq!(
            read.url,
            "https://connector.example.test/api/local-connectors/relay/device%20%2F%E4%B8%80/plugins/artifacts/read"
        );
        assert_eq!(read.workspace_id, "workspace-1");
        assert_eq!(read.owner_user_id, "user-1");
        assert_eq!(read.timeout, std::time::Duration::from_millis(300));
        chatos_service_runtime::verify_internal_service_token(
            read.token.as_str(),
            secret,
            "chatos-backend",
            "local-connector-service",
            "plugin.artifact.read",
        )
        .expect("verify read-scoped service token");

        let write = prepare_plugin_artifact_relay_request(
            auth.user_id.as_str(),
            &ready,
            "update",
            "https://connector.example.test",
            secret,
            1_000,
        )
        .expect("prepare write relay request");
        assert_eq!(write.timeout, std::time::Duration::from_millis(315_000));
        chatos_service_runtime::verify_internal_service_token(
            write.token.as_str(),
            secret,
            "chatos-backend",
            "local-connector-service",
            "plugin.artifact.write",
        )
        .expect("verify write-scoped service token");
        assert!(chatos_service_runtime::verify_internal_service_token(
            write.token.as_str(),
            secret,
            "chatos-backend",
            "local-connector-service",
            "plugin.artifact.read",
        )
        .is_err());
    }
}
