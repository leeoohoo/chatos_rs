// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path as FsPath};
use std::sync::Mutex;
use std::time::Duration;

use axum::body::Body;
use axum::extract::{Path, Query};
use axum::http::header::{
    CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_TYPE, REFERRER_POLICY,
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
    PluginUiAssetReadResponse, PluginUiReadyEventPayload, PluginUiSnapshot,
    PLUGIN_ARTIFACT_INLINE_READ_MAX_BYTES, PLUGIN_ARTIFACT_MAX_BYTES,
    PLUGIN_ARTIFACT_WRITE_MAX_BYTES, PLUGIN_UI_ASSET_MAX_BYTES,
    PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_CREATE, PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_DOWNLOAD,
    PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_LIST, PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_READ,
    PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_UPDATE, PLUGIN_UI_BRIDGE_CAPABILITY_HOST_CONTEXT_READ,
    PLUGIN_UI_BRIDGE_PROTOCOL_VERSION_V1, PLUGIN_UI_HOST_CSP_V1, PLUGIN_UI_IFRAME_SANDBOX_V1,
    PLUGIN_UI_MAX_ARTIFACT_MIME_TYPES, PLUGIN_UI_MAX_ASSETS, PLUGIN_UI_MAX_BRIDGE_CAPABILITIES,
    PLUGIN_UI_READY_EVENT_VERSION_V1, PLUGIN_UI_SURFACE_ARTIFACT_VIEWER,
    PLUGIN_UI_SURFACE_DETAIL_PANEL, PLUGIN_UI_SURFACE_MESSAGE_PANEL, PLUGIN_UI_SURFACE_WORKBENCH,
    PLUGIN_UI_TOTAL_ASSET_MAX_BYTES,
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

mod asset_response;
mod ready_validation;
mod workbench_handlers;
mod workbench_session;

use asset_response::{
    bad_gateway, bad_request, expected_media_type, is_lower_sha256, is_safe_ui_path,
    map_relay_status, normalize_requested_asset_path, not_found, plugin_ui_asset_response,
    service_unavailable, validate_asset_response,
};
use ready_validation::{decode_ready_event, validate_ready_payload};
use workbench_handlers::{
    create_plugin_ui_workbench_artifact, create_plugin_ui_workbench_session,
    download_plugin_ui_workbench_artifact, get_plugin_ui_asset, get_plugin_ui_workbench_asset,
    list_plugin_ui_workbench_artifacts, read_plugin_ui_workbench_artifact,
    revoke_plugin_ui_workbench_session, update_plugin_ui_workbench_artifact,
};
use workbench_session::{
    artifact_access_for_session, decode_artifact_write_body, get_plugin_ui_workbench_session,
    issue_plugin_ui_workbench_session, lock_workbench_sessions, normalize_workbench_session_id,
    owned_plugin_ui_workbench_session, prune_expired_workbench_sessions,
    require_workbench_capability, resolve_ready_event_for_message,
};

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

#[cfg(test)]
mod tests;
