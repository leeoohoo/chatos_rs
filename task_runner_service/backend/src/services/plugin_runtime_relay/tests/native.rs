// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[test]
fn native_skill_response_must_match_run_component_snapshot() {
    let mut component = component_snapshot();
    component.kind = PluginComponentKind::SkillCollection;
    component.component_key = "computer-use".to_string();
    component.content_sha256 = "a".repeat(64);
    component
        .runtime
        .insert("runtime_kind".to_string(), json!("native_adapter"));
    component.runtime.insert(
        "metadata".to_string(),
        json!({
            "skill_id": "internal_skill_computer_use",
            "bundle_id": "chatos.internal.computer-use",
            "bundle_hash": "a".repeat(64),
        }),
    );
    let plugin = plugin_snapshot();
    let native = json!({
        "plugin_id": plugin.plugin_id,
        "release_id": plugin.release_id,
        "plugin_version": plugin.version,
        "artifact_sha256": plugin.artifact_sha256,
        "component_key": component.component_key,
        "skill_id": "internal_skill_computer_use",
        "bundle_id": "chatos.internal.computer-use",
        "bundle_hash": "a".repeat(64),
        "snapshot_sha256": "b".repeat(64),
        "tool_snapshot_sha256": "c".repeat(64),
        "permissions": ["system.accessibility"],
        "tools": [{"name": "computer_list_windows", "inputSchema": {"type": "object"}}],
    });
    assert!(validate_native_skill_response(&plugin, &component, &native).is_ok());

    let mut drifted = native;
    drifted["bundle_hash"] = json!("d".repeat(64));
    assert!(
        validate_native_skill_response(&plugin, &component, &drifted)
            .expect_err("bundle hash drift must fail closed")
            .contains("bundle_hash")
    );
}

#[test]
fn chrome_native_skill_tools_flow_through_the_generic_plugin_relay() {
    let mut component = component_snapshot();
    component.kind = PluginComponentKind::SkillCollection;
    component.component_key = "control-chrome".to_string();
    component.content_sha256 = "a".repeat(64);
    component
        .runtime
        .insert("runtime_kind".to_string(), json!("native_adapter"));
    component.runtime.insert(
        "metadata".to_string(),
        json!({
            "skill_id": "internal_skill_chrome",
            "bundle_id": "chatos.internal.chrome",
            "bundle_hash": "a".repeat(64),
        }),
    );
    let plugin = plugin_snapshot();
    let native = json!({
        "plugin_id": plugin.plugin_id,
        "release_id": plugin.release_id,
        "plugin_version": plugin.version,
        "artifact_sha256": plugin.artifact_sha256,
        "component_key": component.component_key,
        "skill_id": "internal_skill_chrome",
        "bundle_id": "chatos.internal.chrome",
        "bundle_hash": "a".repeat(64),
        "snapshot_sha256": "b".repeat(64),
        "tool_snapshot_sha256": "c".repeat(64),
        "permissions": ["browser.chrome.control", "workspace.read", "workspace.write"],
        "tools": [
            {"name": "chrome_status", "inputSchema": {"type": "object"}},
            {"name": "chrome_tabs", "inputSchema": {"type": "object"}},
            {"name": "chrome_tab_snapshot", "inputSchema": {"type": "object"}},
            {"name": "chrome_tab_navigate", "inputSchema": {"type": "object"}},
            {"name": "chrome_tab_click", "inputSchema": {"type": "object"}},
            {"name": "chrome_tab_type_text", "inputSchema": {"type": "object"}},
            {"name": "chrome_tab_select", "inputSchema": {"type": "object"}},
            {"name": "chrome_tab_scroll", "inputSchema": {"type": "object"}},
            {"name": "chrome_tab_history", "inputSchema": {"type": "object"}},
            {"name": "chrome_tab_activate", "inputSchema": {"type": "object"}},
            {"name": "chrome_tab_upload", "inputSchema": {"type": "object"}},
            {"name": "chrome_tab_download", "inputSchema": {"type": "object"}},
            {"name": "chrome_tab_screenshot", "inputSchema": {"type": "object"}},
            {"name": "chrome_tab_release", "inputSchema": {"type": "object"}}
        ],
    });
    assert!(validate_native_skill_response(&plugin, &component, &native).is_ok());
}

#[test]
fn transient_plugin_images_require_declared_model_image_support() {
    let result = json!({
        "text": "captured",
        "_model_input": [{"image_url": "data:image/jpeg;base64,/9j/AA=="}]
    });
    let supported = filter_transient_model_input_for_runtime(result.clone(), Some(true));
    assert!(supported.get("_model_input").is_some());

    let unsupported = filter_transient_model_input_for_runtime(result, Some(false));
    assert!(unsupported.get("_model_input").is_none());
    assert_eq!(
        unsupported
            .pointer("/model_image_delivery/delivered")
            .and_then(serde_json::Value::as_bool),
        Some(false)
    );
    assert!(unsupported
        .get("text")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|text| text.contains("does not declare image input support")));
}
