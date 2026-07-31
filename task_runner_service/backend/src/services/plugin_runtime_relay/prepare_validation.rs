// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};

use chatos_plugin_management_sdk::{
    normalized_plugin_hook_set_sha256, parse_plugin_hook_set, plugin_hook_snapshot_sha256,
    plugin_ui_snapshot_sha256, PluginHookSet, PluginUiSnapshot, RunPluginComponentSnapshot,
    RunPluginSnapshot, PLUGIN_UI_ASSET_MAX_BYTES, PLUGIN_UI_BRIDGE_PROTOCOL_VERSION_V1,
    PLUGIN_UI_HOST_CSP_V1, PLUGIN_UI_IFRAME_SANDBOX_V1, PLUGIN_UI_SURFACE_DETAIL_PANEL,
    PLUGIN_UI_TOTAL_ASSET_MAX_BYTES,
};
use serde_json::Value;

pub(super) fn validate_prepare_response(
    plugin: &RunPluginSnapshot,
    component: &RunPluginComponentSnapshot,
    response: &Value,
) -> Result<(), String> {
    for (field, expected) in [
        ("plugin_id", plugin.plugin_id.as_str()),
        ("release_id", plugin.release_id.as_str()),
        ("version", plugin.version.as_str()),
        ("artifact_sha256", plugin.artifact_sha256.as_str()),
        ("component_key", component.component_key.as_str()),
    ] {
        if response.get(field).and_then(Value::as_str) != Some(expected) {
            return Err(format!(
                "Plugin prepare response {field} does not match the immutable Run snapshot"
            ));
        }
    }
    let actual_permissions = response
        .get("permission_snapshot")
        .and_then(Value::as_array)
        .ok_or_else(|| "Plugin prepare response is missing permission_snapshot".to_string())?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .ok_or_else(|| {
                    "Plugin prepare response contains an invalid permission snapshot".to_string()
                })
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    let expected_permissions = plugin
        .permission_snapshot
        .iter()
        .map(|permission| permission.trim().to_string())
        .collect::<BTreeSet<_>>();
    if actual_permissions != expected_permissions {
        return Err(
            "Plugin prepare response permission_snapshot does not match the immutable Run snapshot"
                .to_string(),
        );
    }
    Ok(())
}

pub(super) fn validate_hook_response(
    plugin: &RunPluginSnapshot,
    component: &RunPluginComponentSnapshot,
    response: &Value,
) -> Result<String, String> {
    let hooks = response
        .get("hooks")
        .and_then(Value::as_array)
        .ok_or_else(|| "Plugin Hook prepare response is missing the Hook snapshot".to_string())?;
    if hooks.len() != 1 {
        return Err("Plugin Hook prepare response must contain exactly one Hook set".to_string());
    }
    let hook = &hooks[0];
    for (field, expected) in [
        ("plugin_id", plugin.plugin_id.as_str()),
        ("release_id", plugin.release_id.as_str()),
        ("version", plugin.version.as_str()),
        ("artifact_sha256", plugin.artifact_sha256.as_str()),
        ("component_key", component.component_key.as_str()),
        ("content_sha256", component.content_sha256.as_str()),
    ] {
        if hook.get(field).and_then(Value::as_str) != Some(expected) {
            return Err(format!(
                "Plugin Hook prepare response {field} does not match the immutable Run snapshot"
            ));
        }
    }
    let expected_entrypoint = component
        .runtime
        .get("entrypoint")
        .and_then(Value::as_str)
        .ok_or_else(|| "Plugin Hook Run snapshot is missing entrypoint".to_string())?;
    if hook.get("relative_source_path").and_then(Value::as_str) != Some(expected_entrypoint) {
        return Err(
            "Plugin Hook source does not match the immutable component entrypoint".to_string(),
        );
    }
    let hook_set: PluginHookSet = serde_json::from_value(
        hook.get("hook_set")
            .cloned()
            .ok_or_else(|| "Plugin Hook prepare response is missing hook_set".to_string())?,
    )
    .map_err(|error| format!("Plugin Hook prepare response hook_set is invalid: {error}"))?;
    let canonical_hook_set = parse_plugin_hook_set(
        serde_json::to_string(&hook_set)
            .map_err(|error| format!("encode Plugin Hook set failed: {error}"))?
            .as_str(),
    )
    .map_err(|error| format!("Plugin Hook set validation failed: {error}"))?;
    if canonical_hook_set != hook_set {
        return Err("Plugin Hook prepare response is not canonically normalized".to_string());
    }
    let hook_set_sha256 = normalized_plugin_hook_set_sha256(&hook_set)
        .map_err(|error| format!("hash Plugin Hook set failed: {error}"))?;
    if hook.get("hook_set_sha256").and_then(Value::as_str) != Some(hook_set_sha256.as_str()) {
        return Err("Plugin Hook set hash does not match its normalized snapshot".to_string());
    }
    let command_sha256_by_hook = serde_json::from_value::<BTreeMap<String, String>>(
        hook.get("command_sha256_by_hook").cloned().ok_or_else(|| {
            "Plugin Hook prepare response is missing command_sha256_by_hook".to_string()
        })?,
    )
    .map_err(|error| format!("Plugin Hook command snapshot is invalid: {error}"))?;
    if command_sha256_by_hook.len() != hook_set.hooks.len()
        || command_sha256_by_hook
            .values()
            .any(|value| !is_lower_sha256(value))
        || hook_set
            .hooks
            .iter()
            .any(|definition| !command_sha256_by_hook.contains_key(definition.id.as_str()))
    {
        return Err("Plugin Hook command hashes do not cover the normalized Hook set".to_string());
    }
    let expected_snapshot_sha256 = plugin_hook_snapshot_sha256(
        plugin.plugin_id.as_str(),
        plugin.release_id.as_str(),
        component.component_key.as_str(),
        expected_entrypoint,
        component.content_sha256.as_str(),
        hook_set_sha256.as_str(),
        &command_sha256_by_hook,
    )
    .map_err(|error| format!("hash Plugin Hook snapshot failed: {error}"))?;
    if hook.get("snapshot_sha256").and_then(Value::as_str)
        != Some(expected_snapshot_sha256.as_str())
    {
        return Err(
            "Plugin Hook snapshot hash does not match the immutable Run snapshot".to_string(),
        );
    }
    let operations = response
        .get("operations")
        .and_then(Value::as_array)
        .ok_or_else(|| "Plugin Hook prepare response is missing operations".to_string())?;
    if !operations
        .iter()
        .any(|operation| operation.as_str() == Some("dispatch_hook_event"))
    {
        return Err("Plugin Hook prepare response did not publish dispatch_hook_event".to_string());
    }
    Ok(expected_snapshot_sha256)
}

pub(super) fn validate_ui_response(
    plugin: &RunPluginSnapshot,
    component: &RunPluginComponentSnapshot,
    response: &Value,
) -> Result<PluginUiSnapshot, String> {
    let values = response
        .get("ui")
        .and_then(Value::as_array)
        .ok_or_else(|| "Plugin UI prepare response is missing the UI snapshot".to_string())?;
    if values.len() != 1 {
        return Err("Plugin UI prepare response must contain exactly one UI snapshot".to_string());
    }
    let snapshot: PluginUiSnapshot = serde_json::from_value(values[0].clone())
        .map_err(|error| format!("Plugin UI prepare response is invalid: {error}"))?;
    for (field, actual, expected) in [
        (
            "plugin_id",
            snapshot.plugin_id.as_str(),
            plugin.plugin_id.as_str(),
        ),
        (
            "release_id",
            snapshot.release_id.as_str(),
            plugin.release_id.as_str(),
        ),
        (
            "version",
            snapshot.version.as_str(),
            plugin.version.as_str(),
        ),
        (
            "artifact_sha256",
            snapshot.artifact_sha256.as_str(),
            plugin.artifact_sha256.as_str(),
        ),
        (
            "component_key",
            snapshot.component_key.as_str(),
            component.component_key.as_str(),
        ),
        (
            "content_sha256",
            snapshot.content_sha256.as_str(),
            component.content_sha256.as_str(),
        ),
    ] {
        if actual != expected {
            return Err(format!(
                "Plugin UI prepare response {field} does not match the immutable Run snapshot"
            ));
        }
    }
    let expected_entrypoint = component
        .runtime
        .get("entrypoint")
        .and_then(Value::as_str)
        .ok_or_else(|| "Plugin UI Run snapshot is missing entrypoint".to_string())?;
    if snapshot.relative_source_path != expected_entrypoint {
        return Err(
            "Plugin UI source does not match the immutable component entrypoint".to_string(),
        );
    }
    let metadata = component.runtime.get("metadata").and_then(Value::as_object);
    let expected_title = metadata
        .and_then(|metadata| metadata.get("title"))
        .and_then(Value::as_str)
        .unwrap_or(component.component_key.as_str());
    let expected_surface = metadata
        .and_then(|metadata| metadata.get("surface"))
        .and_then(Value::as_str)
        .unwrap_or(PLUGIN_UI_SURFACE_DETAIL_PANEL);
    let expected_assets = metadata_string_array(
        metadata.and_then(|metadata| metadata.get("assets")),
        "Plugin UI immutable asset paths",
    )?;
    let actual_assets = snapshot
        .assets
        .iter()
        .map(|asset| asset.relative_path.clone())
        .collect::<Vec<_>>();
    if actual_assets != expected_assets {
        return Err("Plugin UI assets do not match the immutable Run snapshot".to_string());
    }
    let expected_capabilities = metadata_string_array(
        metadata.and_then(|metadata| metadata.get("bridge_capabilities")),
        "Plugin UI immutable bridge capabilities",
    )?;
    let expected_mime_types = metadata_string_array(
        metadata.and_then(|metadata| metadata.get("artifact_mime_types")),
        "Plugin UI immutable artifact MIME types",
    )?;
    if snapshot.title != expected_title
        || snapshot.surface != expected_surface
        || snapshot.bridge_capabilities != expected_capabilities
        || snapshot.artifact_mime_types != expected_mime_types
    {
        return Err("Plugin UI metadata does not match the immutable Run snapshot".to_string());
    }
    if snapshot.bridge_protocol_version != PLUGIN_UI_BRIDGE_PROTOCOL_VERSION_V1
        || snapshot.content_security_policy != PLUGIN_UI_HOST_CSP_V1
        || snapshot.iframe_sandbox != PLUGIN_UI_IFRAME_SANDBOX_V1
    {
        return Err("Plugin UI Host security contract is invalid".to_string());
    }
    let mut total_asset_bytes = 0_u64;
    let mut asset_paths = BTreeSet::new();
    for asset in &snapshot.assets {
        if !asset_paths.insert(asset.relative_path.as_str())
            || !is_lower_sha256(asset.sha256.as_str())
            || asset.size_bytes > PLUGIN_UI_ASSET_MAX_BYTES
            || asset.media_type.trim().is_empty()
        {
            return Err("Plugin UI asset snapshot is invalid".to_string());
        }
        total_asset_bytes = total_asset_bytes
            .checked_add(asset.size_bytes)
            .ok_or_else(|| "Plugin UI total asset size overflow".to_string())?;
    }
    if total_asset_bytes > PLUGIN_UI_TOTAL_ASSET_MAX_BYTES {
        return Err("Plugin UI assets exceed the total size limit".to_string());
    }
    if !is_lower_sha256(snapshot.content_sha256.as_str()) {
        return Err("Plugin UI entrypoint snapshot is invalid".to_string());
    }
    let expected_snapshot_sha256 = plugin_ui_snapshot_sha256(
        plugin.plugin_id.as_str(),
        plugin.release_id.as_str(),
        component.component_key.as_str(),
        snapshot.title.as_str(),
        snapshot.surface.as_str(),
        expected_entrypoint,
        component.content_sha256.as_str(),
        snapshot.assets.as_slice(),
        PLUGIN_UI_BRIDGE_PROTOCOL_VERSION_V1,
        snapshot.bridge_capabilities.as_slice(),
        snapshot.artifact_mime_types.as_slice(),
        PLUGIN_UI_HOST_CSP_V1,
        PLUGIN_UI_IFRAME_SANDBOX_V1,
    )
    .map_err(|error| format!("hash Plugin UI snapshot failed: {error}"))?;
    if snapshot.snapshot_sha256 != expected_snapshot_sha256 {
        return Err(
            "Plugin UI snapshot hash does not match the immutable Run snapshot".to_string(),
        );
    }
    Ok(snapshot)
}

fn metadata_string_array(value: Option<&Value>, field: &str) -> Result<Vec<String>, String> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let values = value
        .as_array()
        .ok_or_else(|| format!("{field} must be an array"))?;
    let mut result = Vec::with_capacity(values.len());
    for value in values {
        let value = value
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| format!("{field} contains an invalid item"))?;
        result.push(value.to_string());
    }
    Ok(result)
}

pub(super) fn is_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}
