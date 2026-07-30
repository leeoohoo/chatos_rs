// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(super) fn decode_ready_event(
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

pub(super) fn validate_ready_payload(payload: &PluginUiReadyEventPayload) -> Result<(), ApiError> {
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
