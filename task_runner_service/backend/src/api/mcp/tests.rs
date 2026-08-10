// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_agent::{SystemAgentKey, CHATOS_PLAN_TASK_PROFILE};

use super::*;

#[test]
fn mcp_management_binding_requires_registered_agent_and_complete_identity() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-mcp-management-owner-user-id",
        " user-1 ".parse().expect("valid header"),
    );
    headers.insert(
        "x-mcp-management-agent-key",
        format!(" {} ", SystemAgentKey::TaskRunnerRunPhase.as_str())
            .parse()
            .expect("valid header"),
    );
    headers.insert(
        "x-mcp-management-session-id",
        " session-1 ".parse().expect("valid header"),
    );
    headers.insert(
        "x-mcp-management-session-expires-at-unix",
        " 4102444800 ".parse().expect("valid header"),
    );
    headers.insert(
        "x-mcp-management-project-id",
        " project-1 ".parse().expect("valid header"),
    );
    headers.insert(
        "x-mcp-management-run-id",
        " run-1 ".parse().expect("valid header"),
    );
    headers.insert(
        "x-mcp-management-task-id",
        " task-1 ".parse().expect("valid header"),
    );
    headers.insert(
        "x-mcp-management-source-session-id",
        " source-session-1 ".parse().expect("valid header"),
    );
    headers.insert(
        "x-mcp-management-source-user-message-id",
        " message-1 ".parse().expect("valid header"),
    );
    headers.insert(
        "x-mcp-management-contact-agent-id",
        " chatos-agent-1 ".parse().expect("valid header"),
    );
    headers.insert(
        "x-mcp-management-task-profile",
        format!(" {CHATOS_PLAN_TASK_PROFILE} ")
            .parse()
            .expect("valid header"),
    );
    headers.insert(
        "x-mcp-management-expected-project-task-ids",
        " project-task-b,project-task-a,project-task-a "
            .parse()
            .expect("valid header"),
    );

    let binding = mcp_management_binding_from_headers(&headers).expect("valid binding");
    assert_eq!(binding.owner_user_id, "user-1");
    assert_eq!(binding.agent_key, SystemAgentKey::TaskRunnerRunPhase);
    assert_eq!(binding.session_id, "session-1");
    assert_eq!(binding.session_expires_at_unix, 4_102_444_800);
    assert_eq!(binding.project_id, "project-1");
    assert_eq!(binding.run_id.as_deref(), Some("run-1"));
    assert_eq!(binding.task_id.as_deref(), Some("task-1"));
    assert_eq!(
        binding.source_session_id.as_deref(),
        Some("source-session-1")
    );
    assert_eq!(binding.source_user_message_id.as_deref(), Some("message-1"));
    assert_eq!(binding.contact_agent_id.as_deref(), Some("chatos-agent-1"));
    assert_eq!(
        binding.task_profile.as_deref(),
        Some(CHATOS_PLAN_TASK_PROFILE)
    );
    assert_eq!(
        binding.expected_project_task_ids,
        std::collections::BTreeSet::from([
            "project-task-a".to_string(),
            "project-task-b".to_string(),
        ])
    );

    headers.insert(
        "x-mcp-management-agent-key",
        "arbitrary-agent".parse().expect("valid header"),
    );
    assert!(mcp_management_binding_from_headers(&headers)
        .expect_err("unknown agent must fail")
        .contains("registered System Agent"));
}

#[test]
fn ask_user_timeout_stays_inside_the_immutable_session_lifetime() {
    let binding = McpManagementBinding {
        owner_user_id: "user-1".to_string(),
        agent_key: chatos_plugin_management_sdk::SystemAgentKey::TaskRunnerRunPhase,
        session_id: "session-1".to_string(),
        session_expires_at_unix: chrono::Utc::now().timestamp() + 30 * 60,
        project_id: "project-1".to_string(),
        run_id: Some("run-1".to_string()),
        turn_id: None,
        task_id: Some("task-1".to_string()),
        source_session_id: None,
        source_user_message_id: None,
        contact_agent_id: None,
        default_model_config_id: None,
        task_profile: Some(crate::models::TASK_PROFILE_DEFAULT.to_string()),
        expected_project_task_ids: std::collections::BTreeSet::new(),
    };

    let timeout = bound_ask_user_prompt_timeout_ms(&binding).expect("usable session lifetime");
    assert!(timeout <= 25 * 60 * 1_000);
    assert!(timeout >= 24 * 60 * 1_000);

    let mut expiring = binding;
    expiring.session_expires_at_unix = chrono::Utc::now().timestamp() + 60;
    assert!(bound_ask_user_prompt_timeout_ms(&expiring).is_err());
}

#[test]
fn bound_task_creator_uses_chatos_agent_and_keeps_human_owner() {
    let binding = McpManagementBinding {
        owner_user_id: "user-1".to_string(),
        agent_key: chatos_plugin_management_sdk::SystemAgentKey::ChatosConversationAgent,
        session_id: "session-1".to_string(),
        session_expires_at_unix: chrono::Utc::now().timestamp() + 30 * 60,
        project_id: "project-1".to_string(),
        run_id: None,
        turn_id: Some("turn-1".to_string()),
        task_id: None,
        source_session_id: Some("source-session-1".to_string()),
        source_user_message_id: Some("message-1".to_string()),
        contact_agent_id: Some("chatos-agent-1".to_string()),
        default_model_config_id: None,
        task_profile: Some(crate::models::TASK_PROFILE_DEFAULT.to_string()),
        expected_project_task_ids: std::collections::BTreeSet::new(),
    };

    let creator = bound_task_creator(&binding, true).expect("bound ChatOS creator");

    assert_eq!(creator.id, "chatos-agent-1");
    assert_eq!(creator.username, "chatos-agent-1");
    assert_eq!(creator.display_name, "chatos-agent-1");
    assert_eq!(creator.effective_owner_user_id(), Some("user-1"));

    let mut missing_agent = binding;
    missing_agent.contact_agent_id = None;
    assert!(bound_task_creator(&missing_agent, true)
        .expect_err("tool calls without a ChatOS Agent must fail")
        .contains("contact_agent_id"));
    assert!(bound_task_creator(&missing_agent, false).is_ok());
}

#[test]
fn task_process_log_arguments_cannot_override_bound_identity() {
    let error = serde_json::from_value::<BoundTaskProcessLogArgs>(json!({
        "operation": "append",
        "content": "verified",
        "heading": null,
        "task_id": "another-task",
        "run_id": "another-run",
        "owner_user_id": "another-user"
    }))
    .expect_err("identity override fields must be rejected");

    assert!(error.to_string().contains("unknown field"));
}

#[test]
fn request_context_reads_inherited_model_header() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-task-runner-default-model-config-id",
        " model-selected ".parse().expect("valid header"),
    );

    let context = mcp_request_context_from_headers(&headers).expect("valid context");

    assert_eq!(
        context.default_model_config_id.as_deref(),
        Some("model-selected")
    );
}

#[test]
fn request_context_reads_exact_project_task_scope_header() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-task-runner-expected-project-task-ids",
        " task-b,task-a,task-a ".parse().expect("valid header"),
    );

    let context = mcp_request_context_from_headers(&headers).expect("valid context");

    assert_eq!(
        context.expected_project_task_ids,
        std::collections::BTreeSet::from(["task-a".to_string(), "task-b".to_string()])
    );
}

#[test]
fn request_context_reads_and_normalizes_user_plugin_selection() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-task-runner-plugin-device-id",
        " device-1 ".parse().expect("valid header"),
    );
    headers.insert(
        "x-task-runner-plugin-workspace-id",
        " workspace-1 ".parse().expect("valid header"),
    );
    headers.insert(
        "x-task-runner-selected-plugins",
        r#"["plugin-a",{"plugin_id":" plugin-a ","selected_skill_ids":[],"selected_command_ids":[]},{"plugin_id":"plugin-b","selected_skill_ids":[" skill-1 ","skill-1"],"selected_command_ids":[" review ","review"]}]"#
            .parse()
            .expect("valid header"),
    );
    let command_invocations = serde_json::to_vec(&vec![
        chatos_plugin_management_sdk::PluginCommandInvocation {
            plugin_id: " plugin-b ".to_string(),
            command_id: " review ".to_string(),
            arguments: Some(" 检查中文参数 ".to_string()),
        },
    ])
    .expect("serialize command invocations");
    headers.insert(
        "x-task-runner-plugin-command-invocations",
        URL_SAFE_NO_PAD
            .encode(command_invocations)
            .parse()
            .expect("valid header"),
    );

    let context = mcp_request_context_from_headers(&headers).expect("valid context");
    let config = context
        .plugin_config_override
        .expect("plugin config override");

    assert_eq!(config.device_id.as_deref(), Some("device-1"));
    assert_eq!(config.workspace_id.as_deref(), Some("workspace-1"));
    assert_eq!(config.selected_plugins.len(), 2);
    assert_eq!(config.selected_plugins[0].plugin_id, "plugin-a");
    assert_eq!(
        config.selected_plugins[1].selected_skill_ids,
        vec!["skill-1".to_string()]
    );
    assert_eq!(
        config.selected_plugins[1].selected_command_ids,
        vec!["review".to_string()]
    );
    assert_eq!(
        config.command_invocations,
        vec![chatos_plugin_management_sdk::PluginCommandInvocation {
            plugin_id: "plugin-b".to_string(),
            command_id: "review".to_string(),
            arguments: Some("检查中文参数".to_string()),
        }]
    );
}

#[test]
fn request_context_rejects_invalid_plugin_selection_json() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-task-runner-selected-plugins",
        "not-json".parse().expect("valid header"),
    );

    let error = mcp_request_context_from_headers(&headers).expect_err("invalid context");

    assert!(error.contains("invalid x-task-runner-selected-plugins JSON"));
}

#[test]
fn request_context_rejects_duplicate_plugin_command_invocations() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-task-runner-plugin-command-invocations",
        r#"[{"plugin_id":"plugin-a","command_id":"review","arguments":null},{"plugin_id":" plugin-a ","command_id":" review ","arguments":"src"}]"#
            .parse()
            .expect("valid header"),
    );

    let error = mcp_request_context_from_headers(&headers).expect_err("invalid context");

    assert!(error.contains("Plugin Command invocation is duplicated"));
}

#[test]
fn request_context_rejects_oversized_plugin_command_arguments() {
    let payload = serde_json::to_string(&vec![
        chatos_plugin_management_sdk::PluginCommandInvocation {
            plugin_id: "plugin-a".to_string(),
            command_id: "review".to_string(),
            arguments: Some("a".repeat(PLUGIN_COMMAND_ARGUMENT_LIMIT_BYTES + 1)),
        },
    ])
    .expect("serialize command invocations");
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-task-runner-plugin-command-invocations",
        payload.parse().expect("valid header"),
    );

    let error = mcp_request_context_from_headers(&headers).expect_err("invalid context");

    assert!(error.contains("Plugin Command arguments exceed"));
}
