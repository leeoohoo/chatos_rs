// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(super) async fn resolve_ready_event_for_message(
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

pub(super) fn issue_plugin_ui_workbench_session(
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

pub(super) fn get_plugin_ui_workbench_session(
    session_id: &str,
) -> Result<PluginUiWorkbenchSession, ApiError> {
    let session_id = normalize_workbench_session_id(session_id)?;
    let mut sessions = lock_workbench_sessions()?;
    prune_expired_workbench_sessions(&mut sessions);
    sessions
        .get(session_id)
        .cloned()
        .ok_or_else(|| not_found("Plugin UI Workbench session 不存在或已过期"))
}

pub(super) fn owned_plugin_ui_workbench_session(
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

pub(super) fn artifact_access_for_session(
    session: &PluginUiWorkbenchSession,
) -> PluginArtifactUiAccess {
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

pub(super) fn require_workbench_capability(
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

pub(super) fn decode_artifact_write_body(body_base64: &str) -> Result<Vec<u8>, ApiError> {
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

pub(super) fn lock_workbench_sessions(
) -> Result<std::sync::MutexGuard<'static, BTreeMap<String, PluginUiWorkbenchSession>>, ApiError> {
    PLUGIN_UI_WORKBENCH_SESSIONS
        .lock()
        .map_err(|_| service_unavailable("Plugin UI Workbench session store 不可用"))
}

pub(super) fn prune_expired_workbench_sessions(
    sessions: &mut BTreeMap<String, PluginUiWorkbenchSession>,
) {
    let now = chrono::Utc::now().timestamp();
    sessions.retain(|_, session| session.expires_at_epoch_seconds > now);
}

pub(super) fn normalize_workbench_session_id(session_id: &str) -> Result<&str, ApiError> {
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
