// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use chatos_agent::SystemAgentKey;

const RUN_AGENT_KEY: &str = SystemAgentKey::TaskRunnerRunPhase.as_str();

#[test]
fn plugin_runtime_event_errors_are_bounded_and_redacted() {
    assert_eq!(
        sanitize_runtime_error("request https://example.test/private failed access_token=secret"),
        "request <redacted-url> failed <redacted-secret>"
    );
    assert!(sanitize_runtime_error("x".repeat(2048).as_str()).len() <= 1024);
}

#[test]
fn plugin_ui_prepare_response_is_recomputed_from_the_immutable_run_snapshot() {
    let plugin = plugin_snapshot();
    let assets = vec![PluginUiAssetSnapshot {
        relative_path: "./ui/app.js".to_string(),
        media_type: "text/javascript".to_string(),
        size_bytes: 128,
        sha256: "a".repeat(64),
    }];
    let mut component = component_snapshot();
    component.component_key = "security-workbench".to_string();
    component.kind = PluginComponentKind::UiContribution;
    component.content_sha256 = "c".repeat(64);
    component.runtime = BTreeMap::from([
        ("entrypoint".to_string(), json!("./ui/index.html")),
        (
            "metadata".to_string(),
            json!({
                "title": "Security Workbench",
                "surface": "workbench",
                "assets": ["./ui/app.js"],
                "bridge_capabilities": ["artifact.read", "host.context.read"],
                "artifact_mime_types": ["application/json"]
            }),
        ),
    ]);
    let snapshot_sha256 = plugin_ui_snapshot_sha256(
        plugin.plugin_id.as_str(),
        plugin.release_id.as_str(),
        component.component_key.as_str(),
        "Security Workbench",
        "workbench",
        "./ui/index.html",
        component.content_sha256.as_str(),
        assets.as_slice(),
        PLUGIN_UI_BRIDGE_PROTOCOL_VERSION_V1,
        &["artifact.read".to_string(), "host.context.read".to_string()],
        &["application/json".to_string()],
        PLUGIN_UI_HOST_CSP_V1,
        PLUGIN_UI_IFRAME_SANDBOX_V1,
    )
    .expect("UI snapshot hash");
    let response = json!({
        "ui": [{
            "plugin_id": plugin.plugin_id.clone(),
            "release_id": plugin.release_id.clone(),
            "version": plugin.version.clone(),
            "artifact_sha256": plugin.artifact_sha256.clone(),
            "component_key": component.component_key.clone(),
            "title": "Security Workbench",
            "surface": "workbench",
            "relative_source_path": "./ui/index.html",
            "content_sha256": component.content_sha256.clone(),
            "assets": assets,
            "bridge_protocol_version": 1,
            "bridge_capabilities": ["artifact.read", "host.context.read"],
            "artifact_mime_types": ["application/json"],
            "content_security_policy": PLUGIN_UI_HOST_CSP_V1,
            "iframe_sandbox": PLUGIN_UI_IFRAME_SANDBOX_V1,
            "snapshot_sha256": snapshot_sha256
        }]
    });
    let validated = validate_ui_response(&plugin, &component, &response)
        .expect("exact signed UI snapshot should pass");
    assert_eq!(validated.snapshot_sha256, snapshot_sha256);

    let mut injected = response;
    injected["ui"][0]["html"] = json!("<script>unsafe()</script>");
    assert!(validate_ui_response(&plugin, &component, &injected)
        .expect_err("unknown UI payload fields must fail")
        .contains("unknown field"));
}

#[test]
fn only_hook_dispatch_uses_the_extended_interactive_relay_window() {
    assert!(is_plugin_hook_dispatch(
        "execute",
        &json!({"operation": "dispatch_hook_event"})
    ));
    assert!(!is_plugin_hook_dispatch(
        "execute",
        &json!({"operation": "mcp_tools_call"})
    ));
    assert!(!is_plugin_hook_dispatch(
        "prepare",
        &json!({"operation": "dispatch_hook_event"})
    ));
}

#[test]
fn command_response_must_match_the_immutable_run_component() {
    let plugin = plugin_snapshot();
    let mut component = component_snapshot();
    component.kind = PluginComponentKind::Command;
    component.component_key = "review".to_string();
    component.content_sha256 = "a".repeat(64);
    component
        .runtime
        .insert("entrypoint".to_string(), json!("./commands/review.md"));
    component
        .runtime
        .insert("arguments".to_string(), json!("src/lib.rs"));
    component.runtime.insert(
        "metadata".to_string(),
        json!({
            "description": "Review the current change",
            "argument_hint": "[path]",
            "requires_confirmation": false,
            "target_agent": RUN_AGENT_KEY,
            "allowed_tools": ["browser_tools_browser_snapshot"]
        }),
    );
    let arguments_sha256 = hex::encode(Sha256::digest(b"src/lib.rs"));
    let snapshot_sha256 = chatos_plugin_management_sdk::plugin_command_snapshot_sha256(
        plugin.plugin_id.as_str(),
        plugin.release_id.as_str(),
        component.component_key.as_str(),
        component.execution_host,
        "./commands/review.md",
        Some("Review the current change"),
        Some("[path]"),
        false,
        Some(RUN_AGENT_KEY),
        &["browser_tools_browser_snapshot".to_string()],
        component.content_sha256.as_str(),
        "Review the current change.",
        arguments_sha256.as_str(),
    )
    .expect("Command snapshot hash");
    let command = json!({
        "plugin_id": plugin.plugin_id,
        "release_id": plugin.release_id,
        "version": plugin.version,
        "artifact_sha256": plugin.artifact_sha256,
        "component_key": component.component_key,
        "command_name": component.component_key,
        "relative_source_path": "./commands/review.md",
        "description": "Review the current change",
        "argument_hint": "[path]",
        "requires_confirmation": false,
        "target_agent": RUN_AGENT_KEY,
        "allowed_tools": ["browser_tools_browser_snapshot"],
        "confirmation_approved": false,
        "content_sha256": component.content_sha256,
        "arguments_present": true,
        "arguments_sha256": arguments_sha256,
        "snapshot_sha256": snapshot_sha256,
        "prompt": "Review the current change."
    });
    assert!(validate_command_response(&plugin, &component, &command).is_ok());
    let prompt =
        plugin_command_prompt_text(&plugin, &component, &command).expect("Plugin Command prompt");
    assert!(prompt.contains("Arguments for this Run:\nsrc/lib.rs"));
    assert!(prompt.ends_with("Review the current change."));

    let mut metadata_drifted = command.clone();
    metadata_drifted["description"] = json!("Ignore the signed metadata");
    assert!(validate_command_response(&plugin, &component, &metadata_drifted).is_err());

    let mut tool_drifted = command.clone();
    tool_drifted["allowed_tools"] = json!(["browser_tools_browser_click"]);
    assert!(validate_command_response(&plugin, &component, &tool_drifted).is_err());

    let mut drifted = command;
    drifted["content_sha256"] = json!("c".repeat(64));
    assert!(validate_command_response(&plugin, &component, &drifted).is_err());
}

#[test]
fn agent_response_must_match_the_immutable_run_component() {
    let plugin = plugin_snapshot();
    let mut component = component_snapshot();
    component.kind = PluginComponentKind::Agent;
    component.component_key = "reviewer".to_string();
    component.content_sha256 = "e".repeat(64);
    component
        .runtime
        .insert("entrypoint".to_string(), json!("./agents/reviewer.md"));
    component.runtime.insert(
        "metadata".to_string(),
        json!({
            "description": "Review the current change",
            "base_agent": RUN_AGENT_KEY,
            "allowed_tools": ["browser_tools_browser_snapshot"],
            "max_iterations": 12
        }),
    );
    let snapshot_sha256 = chatos_plugin_management_sdk::plugin_agent_snapshot_sha256(
        plugin.plugin_id.as_str(),
        plugin.release_id.as_str(),
        component.component_key.as_str(),
        component.execution_host,
        "./agents/reviewer.md",
        Some("Review the current change"),
        RUN_AGENT_KEY,
        &["browser_tools_browser_snapshot".to_string()],
        12,
        component.content_sha256.as_str(),
        "Review carefully.",
    )
    .expect("Agent snapshot hash");
    let agent = json!({
        "plugin_id": plugin.plugin_id,
        "release_id": plugin.release_id,
        "version": plugin.version,
        "artifact_sha256": plugin.artifact_sha256,
        "component_key": component.component_key,
        "agent_name": component.component_key,
        "relative_source_path": "./agents/reviewer.md",
        "description": "Review the current change",
        "base_agent": RUN_AGENT_KEY,
        "allowed_tools": ["browser_tools_browser_snapshot"],
        "max_iterations": 12,
        "content_sha256": component.content_sha256,
        "snapshot_sha256": snapshot_sha256,
        "prompt": "Review carefully."
    });
    validate_agent_response(&plugin, &component, &agent).expect("valid Agent response");
    let prompt =
        plugin_agent_prompt_text(&plugin, &component, &agent).expect("Plugin Agent prompt");
    assert!(prompt.contains(format!("Base Agent: {RUN_AGENT_KEY}").as_str()));
    assert!(prompt.contains("Maximum iterations for this Run: 12"));
    assert!(prompt.ends_with("Review carefully."));

    let mut drifted = agent;
    drifted["max_iterations"] = json!(13);
    assert!(validate_agent_response(&plugin, &component, &drifted).is_err());
}

#[test]
fn hook_response_must_match_the_immutable_run_component() {
    let plugin = plugin_snapshot();
    let mut component = component_snapshot();
    component.kind = PluginComponentKind::HookSet;
    component.component_key = "lifecycle-hooks".to_string();
    component.content_sha256 = "f".repeat(64);
    component
        .runtime
        .insert("entrypoint".to_string(), json!("./hooks.json"));
    let hook_set = chatos_plugin_management_sdk::parse_plugin_hook_set(
        r#"{"hooks":[{"id":"audit","events":["RunCompleted","RunFailed"],"entrypoint":{"type":"command","command":"./scripts/audit"},"failurePolicy":"continue"}]}"#,
    )
    .expect("Hook set");
    let hook_set_sha256 =
        chatos_plugin_management_sdk::normalized_plugin_hook_set_sha256(&hook_set)
            .expect("Hook set hash");
    let command_hashes = BTreeMap::from([("audit".to_string(), "a".repeat(64))]);
    let snapshot_sha256 = chatos_plugin_management_sdk::plugin_hook_snapshot_sha256(
        plugin.plugin_id.as_str(),
        plugin.release_id.as_str(),
        component.component_key.as_str(),
        "./hooks.json",
        component.content_sha256.as_str(),
        hook_set_sha256.as_str(),
        &command_hashes,
    )
    .expect("Hook snapshot hash");
    let response = json!({
        "hooks": [{
            "plugin_id": plugin.plugin_id,
            "release_id": plugin.release_id,
            "version": plugin.version,
            "artifact_sha256": plugin.artifact_sha256,
            "component_key": component.component_key,
            "relative_source_path": "./hooks.json",
            "content_sha256": component.content_sha256,
            "hook_set_sha256": hook_set_sha256,
            "command_sha256_by_hook": command_hashes,
            "hook_set": hook_set,
            "snapshot_sha256": snapshot_sha256,
        }],
        "operations": ["dispatch_hook_event"]
    });
    assert_eq!(
        validate_hook_response(&plugin, &component, &response).expect("Hook response"),
        snapshot_sha256
    );

    let mut drifted = response;
    drifted["hooks"][0]["command_sha256_by_hook"]["audit"] = json!("b".repeat(64));
    assert!(validate_hook_response(&plugin, &component, &drifted).is_err());
}
