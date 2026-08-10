// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use chatos_agent::SystemAgentKey;

const RUN_AGENT_KEY: &str = SystemAgentKey::TaskRunnerRunPhase.as_str();
const PLAN_AGENT_KEY: &str = SystemAgentKey::TaskRunnerPlanPhase.as_str();

#[test]
fn selected_agent_constraints_narrow_tools_and_iterations() {
    let mut agent = component_snapshot();
    agent.kind = PluginComponentKind::Agent;
    agent.component_key = "reviewer".to_string();
    agent.runtime.insert(
        "metadata".to_string(),
        json!({
            "base_agent": RUN_AGENT_KEY,
            "allowed_tools": ["browser_snapshot"],
            "max_iterations": 12
        }),
    );
    let mut plugin = plugin_snapshot();
    plugin.component_snapshots = vec![agent];
    let run = crate::models::TaskRunRecord::queued(
        "run-1".to_string(),
        "task-1".to_string(),
        "model-1".to_string(),
        "memory-1".to_string(),
        json!({}),
        vec![plugin],
        "2026-07-26T00:00:00Z".to_string(),
    );
    let constraints = plugin_command_execution_constraints(&run).expect("Agent constraints");
    assert_eq!(constraints.target_agent.as_deref(), Some(RUN_AGENT_KEY));
    assert_eq!(
        constraints.agent_identity.as_deref(),
        Some("plugin-browser:reviewer")
    );
    assert_eq!(constraints.max_iterations, Some(12));
    assert_eq!(constraints.tool_allowlists, vec![vec!["browser_snapshot"]]);
}

#[test]
fn selected_command_constraints_preserve_agent_and_individual_tool_allowlists() {
    let mut first = component_snapshot();
    first.kind = PluginComponentKind::Command;
    first.component_key = "review".to_string();
    first.runtime.insert(
        "metadata".to_string(),
        json!({
            "target_agent": RUN_AGENT_KEY,
            "allowed_tools": ["browser_snapshot", "browser_click"]
        }),
    );
    let mut second = first.clone();
    second.component_key = "inspect".to_string();
    second.runtime.insert(
        "metadata".to_string(),
        json!({
            "target_agent": RUN_AGENT_KEY,
            "allowed_tools": ["browser_snapshot"]
        }),
    );
    let mut plugin = plugin_snapshot();
    plugin.component_snapshots = vec![first, second];
    let run = crate::models::TaskRunRecord::queued(
        "run-1".to_string(),
        "task-1".to_string(),
        "model-1".to_string(),
        "memory-1".to_string(),
        json!({}),
        vec![plugin],
        "2026-07-26T00:00:00Z".to_string(),
    );

    let constraints = plugin_command_execution_constraints(&run).expect("Command constraints");
    assert_eq!(constraints.target_agent.as_deref(), Some(RUN_AGENT_KEY));
    assert_eq!(constraints.tool_allowlists.len(), 2);
    assert_eq!(
        constraints.tool_allowlists[0],
        vec!["browser_click", "browser_snapshot"]
    );
    assert_eq!(constraints.tool_allowlists[1], vec!["browser_snapshot"]);
}

#[test]
fn selected_commands_with_different_target_agents_fail_closed() {
    let mut first = component_snapshot();
    first.kind = PluginComponentKind::Command;
    first.component_key = "review".to_string();
    first.runtime.insert(
        "metadata".to_string(),
        json!({"target_agent": RUN_AGENT_KEY}),
    );
    let mut second = first.clone();
    second.component_key = "plan".to_string();
    second.runtime.insert(
        "metadata".to_string(),
        json!({"target_agent": PLAN_AGENT_KEY}),
    );
    let mut plugin = plugin_snapshot();
    plugin.component_snapshots = vec![first, second];
    let run = crate::models::TaskRunRecord::queued(
        "run-1".to_string(),
        "task-1".to_string(),
        "model-1".to_string(),
        "memory-1".to_string(),
        json!({}),
        vec![plugin],
        "2026-07-26T00:00:00Z".to_string(),
    );

    assert!(plugin_command_execution_constraints(&run)
        .expect_err("Agent mismatch must fail")
        .contains("different target Agents"));
}

#[test]
fn relay_base_url_accepts_http_origins_only() {
    assert_eq!(
        validate_plugin_relay_base_url("http://127.0.0.1:39232".to_string()).expect("valid origin"),
        "http://127.0.0.1:39232"
    );
    assert!(validate_plugin_relay_base_url("https://connector.example.com".to_string()).is_ok());
    for value in [
        "file:///tmp/connector",
        "https://user@example.com",
        "https://connector.example.com/api",
        "https://connector.example.com?token=x",
        "https://connector.example.com/#fragment",
    ] {
        assert!(
            validate_plugin_relay_base_url(value.to_string()).is_err(),
            "{value} should be rejected"
        );
    }
}

#[test]
fn prepare_response_must_match_immutable_identity() {
    let plugin = plugin_snapshot();
    let component = component_snapshot();
    let response = json!({
        "plugin_id": plugin.plugin_id,
        "release_id": plugin.release_id,
        "version": plugin.version,
        "artifact_sha256": plugin.artifact_sha256,
        "component_key": component.component_key,
        "adapter_session_id": "session-1",
        "permission_snapshot": plugin.permission_snapshot,
    });
    assert!(validate_prepare_response(&plugin, &component, &response).is_ok());

    let mut drifted = response;
    drifted["release_id"] = json!("release-2");
    let error = validate_prepare_response(&plugin, &component, &drifted)
        .expect_err("release drift should fail closed");
    assert!(error.contains("release_id"));
}

#[test]
fn plugin_server_names_are_normalized_and_bounded() {
    let plugin = plugin_snapshot();
    let component = component_snapshot();
    let name = plugin_server_name(&plugin, &component);
    assert_eq!(name, "plugin_plugin_browser_browser_tools_v1");
    assert!(name.len() <= 96);
    assert!(name.chars().all(|character| character.is_ascii_lowercase()
        || character.is_ascii_digit()
        || character == '_'));
}

#[test]
fn tool_lifecycle_events_map_to_bounded_plugin_hook_contexts() {
    let hook = PluginToolLifecycleHook {
        sessions: Vec::new(),
        agent_key: RUN_AGENT_KEY.to_string(),
        component_by_server: BTreeMap::from([(
            "plugin_plugin_browser_browser_tools".to_string(),
            "browser-tools".to_string(),
        )]),
    };
    let event = ToolLifecycleEvent {
        tool_name: "plugin_plugin_browser_browser_tools_snapshot".to_string(),
        original_name: "snapshot".to_string(),
        server_name: "plugin_plugin_browser_browser_tools".to_string(),
        server_type: "builtin".to_string(),
        arguments_sha256: "a".repeat(64),
        outcome: Some(ToolLifecycleOutcome::Succeeded),
        result_sha256: Some("b".repeat(64)),
    };

    let (pre_event, pre_context) = hook.map_event(&event, PluginToolLifecycleStage::Pre);
    assert_eq!(pre_event, PluginHookEvent::PreToolUse);
    assert_eq!(pre_context.agent_key.as_deref(), Some(RUN_AGENT_KEY));
    assert_eq!(
        pre_context.tool_name.as_deref(),
        Some("plugin_plugin_browser_browser_tools_snapshot")
    );
    assert_eq!(pre_context.tool_kind.as_deref(), Some("builtin"));
    assert_eq!(pre_context.component_key.as_deref(), Some("browser-tools"));
    assert_eq!(pre_context.outcome, None);
    assert_eq!(pre_context.summary_sha256, Some("a".repeat(64)));

    let (post_event, post_context) = hook.map_event(&event, PluginToolLifecycleStage::Post);
    assert_eq!(post_event, PluginHookEvent::PostToolUse);
    assert_eq!(post_context.outcome, Some(PluginHookOutcome::Succeeded));
    assert_eq!(post_context.summary_sha256, Some("b".repeat(64)));
}
