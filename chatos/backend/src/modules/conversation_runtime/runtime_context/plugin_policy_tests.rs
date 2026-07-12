// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

fn test_config() -> Config {
    Config {
        openai_api_key: String::new(),
        openai_base_url: "https://api.openai.com/v1".to_string(),
        port: 3997,
        node_env: "test".to_string(),
        host: "127.0.0.1".to_string(),
        log_level: "info".to_string(),
        log_max_files: "7d".to_string(),
        cors_origins: vec!["*".to_string()],
        summary_enabled: true,
        summary_message_limit: 40,
        summary_max_context_tokens: 6000,
        summary_keep_last_n: 6,
        summary_target_tokens: 700,
        summary_merge_target_tokens: 700,
        summary_temperature: 0.2,
        summary_cooldown_seconds: 60,
        dynamic_summary_enabled: true,
        summary_bisect_enabled: true,
        summary_bisect_max_depth: 6,
        summary_bisect_min_messages: 4,
        summary_retry_on_context_overflow: true,
        auth_jwt_secret: "test-secret".to_string(),
        auth_compat_secret: None,
        auth_access_token_ttl_seconds: 43_200,
        user_service_base_url: Some("http://127.0.0.1:3998".to_string()),
        user_service_request_timeout_ms: 10_000,
        project_service_base_url: "http://127.0.0.1:3999/".to_string(),
        project_service_sync_secret: Some("project-sync-secret".to_string()),
        task_runner_base_url: "http://127.0.0.1:4000".to_string(),
        task_runner_internal_api_secret: Some("task-runner-internal-secret".to_string()),
        task_runner_request_timeout_ms: 10_000,
        local_connector_service_base_url: "http://127.0.0.1:4001".to_string(),
        local_connector_service_request_timeout_ms: 10_000,
        memory_engine_base_url: "http://127.0.0.1:4002".to_string(),
        memory_engine_operator_token: None,
        memory_engine_request_timeout_ms: 10_000,
        memory_engine_active_summary_trigger_timeout_ms: 30_000,
        memory_engine_active_summary_poll_interval_ms: 1_000,
        memory_engine_active_summary_poll_timeout_ms: 120_000,
        task_runner_callback_secret: None,
    }
}

#[test]
fn normal_and_plan_modes_use_distinct_system_agent_keys() {
    assert_eq!(
        ChatosAgentProfile::from_flags(false, false).key(),
        chatos_plugin_management_sdk::SystemAgentKey::ChatosConversationAgent
    );
    assert_eq!(
        ChatosAgentProfile::from_flags(true, false).key(),
        chatos_plugin_management_sdk::SystemAgentKey::ChatosPlanningAgent
    );
    assert_eq!(
        ChatosAgentProfile::from_flags(false, true).key(),
        chatos_plugin_management_sdk::SystemAgentKey::ProjectRequirementExecutionPlannerAgent
    );
}

#[test]
fn project_planner_project_mcp_is_project_scoped_and_read_only() {
    let server =
        build_project_management_mcp_runtime(&test_config(), Some("user-1"), Some("project-1"))
            .expect("build project mcp runtime");

    assert_eq!(server.name, PROJECT_MANAGEMENT_SERVER_NAME);
    assert_eq!(server.url, "http://127.0.0.1:3999/mcp");
    let headers = server.headers.expect("headers");
    assert_eq!(
        headers
            .get("X-Project-Service-Sync-Secret")
            .map(String::as_str),
        Some("project-sync-secret")
    );
    assert_eq!(
        headers.get("X-Project-Service-Caller").map(String::as_str),
        Some("chatos-backend")
    );
    assert_eq!(
        headers
            .get("X-Project-Service-Internal-Scope")
            .map(String::as_str),
        Some("project.mcp")
    );
    assert!(!headers.contains_key("X-Project-Service-Internal-Token"));
    assert_eq!(
        headers
            .get("X-Task-Runner-Owner-User-Id")
            .map(String::as_str),
        Some("user-1")
    );
    assert_eq!(
        headers.get("X-Chatos-Project-Id").map(String::as_str),
        Some("project-1")
    );

    let tools = server.allowed_tool_names.expect("tool allowlist");
    assert!(tools.contains(&"list_project_tasks".to_string()));
    assert!(tools.contains(&"get_requirement_technical_document".to_string()));
    assert!(!tools.contains(&"create_project_task".to_string()));
    assert!(!tools.contains(&"update_requirement".to_string()));
    assert!(!tools.contains(&"delete_project_task".to_string()));
}

#[test]
fn project_planner_project_mcp_requires_sync_secret() {
    let mut config = test_config();
    config.project_service_sync_secret = None;
    let err = build_project_management_mcp_runtime(&config, Some("user-1"), Some("project-1"))
        .expect_err("missing sync secret should fail");

    assert!(err.contains("PROJECT_SERVICE_SYNC_SECRET"));
}
