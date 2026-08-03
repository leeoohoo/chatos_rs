// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[test]
fn catalog_exposes_shared_and_task_runner_iteration_limits() {
    let definitions = builtin_definitions();
    let iteration_definitions = definitions
        .iter()
        .filter(|definition| definition.key.contains("max_iterations"))
        .collect::<Vec<_>>();

    assert_eq!(iteration_definitions.len(), 2);
    let shared = iteration_definitions
        .iter()
        .find(|definition| definition.key == AGENT_MAX_ITERATIONS_CONFIG_KEY)
        .expect("shared agent iteration definition");
    assert_eq!(shared.scope, "shared");
    assert_eq!(shared.service_name, None);
    assert_eq!(shared.default_value, json!(DEFAULT_AGENT_MAX_ITERATIONS));
    assert!(shared.env_aliases.is_empty());

    let task_runner = iteration_definitions
        .iter()
        .find(|definition| definition.key == TASK_RUNNER_MAX_ITERATIONS_CONFIG_KEY)
        .expect("task runner iteration definition");
    assert_eq!(task_runner.scope, "service");
    assert_eq!(task_runner.service_name.as_deref(), Some("task-runner"));
    assert_eq!(
        task_runner.default_value,
        json!(DEFAULT_AGENT_MAX_ITERATIONS)
    );
    assert_eq!(task_runner.max, Some(5000));
    assert!(task_runner.env_aliases.is_empty());
}

#[test]
fn catalog_exposes_local_project_creation_as_a_managed_ui_switch() {
    let definitions = builtin_definitions();
    let definition = definitions
        .iter()
        .find(|definition| definition.key == CHATOS_LOCAL_PROJECT_CREATION_CONFIG_KEY)
        .expect("local project creation UI definition");

    assert_eq!(definition.scope, "service");
    assert_eq!(definition.service_name.as_deref(), Some("chatos-backend"));
    assert_eq!(definition.value_type, "boolean");
    assert_eq!(definition.default_value, json!(false));
    assert_eq!(definition.reload_mode, "hot_reload");
    assert_eq!(
        definition.env_aliases,
        vec!["LOCAL_PROJECT_CREATION_ENABLED"]
    );
}

#[test]
fn catalog_exposes_task_runner_runtime_controls_without_env_overrides() {
    let definitions = builtin_definitions();
    for key in [
        TASK_RUNNER_MAX_ITERATIONS_CONFIG_KEY,
        TASK_RUNNER_EXECUTION_TIMEOUT_CONFIG_KEY,
        TASK_RUNNER_EXECUTION_ENVIRONMENT_MODE_CONFIG_KEY,
        TASK_RUNNER_TOOL_RESULT_MAX_CHARS_CONFIG_KEY,
        TASK_RUNNER_TOOL_RESULTS_TOTAL_MAX_CHARS_CONFIG_KEY,
    ] {
        let definition = definitions
            .iter()
            .find(|definition| definition.key == key)
            .unwrap_or_else(|| panic!("missing definition for {key}"));
        assert_eq!(definition.scope, "service");
        assert_eq!(definition.service_name.as_deref(), Some("task-runner"));
        assert!(
            definition.env_aliases.is_empty(),
            "{key} must be managed from configuration-center values, not env aliases"
        );
    }

    let environment_mode = definitions
        .iter()
        .find(|definition| definition.key == TASK_RUNNER_EXECUTION_ENVIRONMENT_MODE_CONFIG_KEY)
        .expect("task runner execution environment mode definition");
    assert_eq!(environment_mode.value_type, "enum");
    assert_eq!(
        environment_mode.default_value,
        json!(default_task_runner_execution_environment_mode())
    );
    assert_eq!(environment_mode.enum_options, vec!["local", "cloud"]);
}

#[test]
fn catalog_exposes_task_runner_queue_controls_via_managed_env_projection() {
    let definitions = builtin_definitions();
    for (key, env_alias) in [
        (
            TASK_RUNNER_QUEUE_RUN_DISPATCH_MODE_CONFIG_KEY,
            "TASK_RUNNER_RUN_DISPATCH_MODE",
        ),
        (
            TASK_RUNNER_QUEUE_CALLBACK_DELIVERY_MODE_CONFIG_KEY,
            "TASK_RUNNER_CALLBACK_DELIVERY_MODE",
        ),
        (
            TASK_RUNNER_QUEUE_RABBITMQ_URL_CONFIG_KEY,
            "TASK_RUNNER_RABBITMQ_URL",
        ),
        (
            TASK_RUNNER_QUEUE_RABBITMQ_EXCHANGE_CONFIG_KEY,
            "TASK_RUNNER_RABBITMQ_EXCHANGE",
        ),
        (
            TASK_RUNNER_QUEUE_RUN_DISPATCH_QUEUE_CONFIG_KEY,
            "TASK_RUNNER_RUN_DISPATCH_QUEUE",
        ),
        (
            TASK_RUNNER_QUEUE_CALLBACK_DELIVERY_QUEUE_CONFIG_KEY,
            "TASK_RUNNER_CALLBACK_DELIVERY_QUEUE",
        ),
        (
            TASK_RUNNER_QUEUE_RUN_EVENTS_QUEUE_CONFIG_KEY,
            "TASK_RUNNER_RUN_EVENTS_QUEUE",
        ),
    ] {
        let definition = definitions
            .iter()
            .find(|definition| definition.key == key)
            .unwrap_or_else(|| panic!("missing definition for {key}"));
        assert_eq!(definition.scope, "service");
        assert_eq!(definition.service_name.as_deref(), Some("task-runner"));
        assert_eq!(definition.reload_mode, "restart_required");
        assert_eq!(definition.env_aliases, vec![env_alias.to_string()]);
    }
}

#[test]
fn catalog_exposes_shared_memory_policies_for_server_and_client() {
    let definitions = builtin_definitions();
    let memory_definitions = definitions
        .iter()
        .filter(|definition| definition.key.starts_with("memory_engine.policy."))
        .collect::<Vec<_>>();

    assert!(!memory_definitions.is_empty());
    assert!(memory_definitions
        .iter()
        .all(|definition| definition.scope == "shared"));
    assert!(memory_definitions
        .iter()
        .all(|definition| !definition.key.ends_with("model_profile_id")));
    assert!(memory_definitions.iter().any(|definition| {
        definition.key == "memory_engine.policy.rollup.keep_level0_count"
            && definition.default_value == json!(5)
    }));
    assert!(memory_definitions.iter().any(|definition| {
        definition.key == "memory_engine.policy.thread_repair.token_limit"
            && definition.default_value == json!(200000)
    }));
}

#[test]
fn catalog_exposes_local_connector_remote_control_trust_as_managed_config_only() {
    let definitions = builtin_definitions();
    let require_signed = definitions
        .iter()
        .find(|definition| {
            definition.key == LOCAL_CONNECTOR_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY
        })
        .expect("local connector internal signing definition");
    assert_eq!(require_signed.scope, "service");
    assert_eq!(
        require_signed.service_name.as_deref(),
        Some("local-connector-service")
    );
    assert_eq!(
        require_signed.env_aliases,
        vec!["LOCAL_CONNECTOR_REQUIRE_SIGNED_INTERNAL_REQUESTS".to_string()]
    );

    for (key, env_alias, expected_default) in [
        (
            LOCAL_CONNECTOR_CHATOS_INTERNAL_API_SECRET_CONFIG_KEY,
            "CHATOS_LOCAL_CONNECTOR_INTERNAL_API_SECRET",
            json!("change_me_chatos_local_connector_secret"),
        ),
        (
            LOCAL_CONNECTOR_TASK_RUNNER_INTERNAL_API_SECRET_CONFIG_KEY,
            "TASK_RUNNER_LOCAL_CONNECTOR_INTERNAL_API_SECRET",
            json!("change_me_task_runner_local_connector_secret"),
        ),
        (
            LOCAL_CONNECTOR_PROJECT_SERVICE_INTERNAL_API_SECRET_CONFIG_KEY,
            "PROJECT_SERVICE_LOCAL_CONNECTOR_INTERNAL_API_SECRET",
            json!("change_me_project_service_local_connector_secret"),
        ),
        (
            LOCAL_CONNECTOR_MCP_MANAGEMENT_INTERNAL_API_SECRET_CONFIG_KEY,
            "MCP_MANAGEMENT_LOCAL_CONNECTOR_INTERNAL_API_SECRET",
            json!("change_me_mcp_management_local_connector_secret"),
        ),
    ] {
        let definition = definitions
            .iter()
            .find(|definition| definition.key == key)
            .unwrap_or_else(|| panic!("missing definition for {key}"));
        assert_eq!(definition.scope, "service");
        assert_eq!(
            definition.service_name.as_deref(),
            Some("local-connector-service")
        );
        assert_eq!(definition.env_aliases, vec![env_alias.to_string()]);
        assert_eq!(definition.default_value, expected_default);
        assert_eq!(definition.sensitivity, "secret");
    }

    for key in [
        LOCAL_CONNECTOR_RELAY_SIGNING_KEY_PATH_CONFIG_KEY,
        LOCAL_CONNECTOR_RELAY_SIGNING_KEY_ID_CONFIG_KEY,
        LOCAL_CONNECTOR_REMOTE_CONTROL_REQUIRE_SIGNED_CONFIG_KEY,
        LOCAL_CONNECTOR_REMOTE_CONTROL_SIGNATURE_MAX_SKEW_SECONDS_CONFIG_KEY,
        LOCAL_CONNECTOR_REMOTE_CONTROL_TRUSTED_RELAY_PUBLIC_KEYS_CONFIG_KEY,
    ] {
        let definition = definitions
            .iter()
            .find(|definition| definition.key == key)
            .unwrap_or_else(|| panic!("missing definition for {key}"));
        assert_eq!(definition.scope, "service");
        assert_eq!(
            definition.service_name.as_deref(),
            Some("local-connector-service")
        );
        assert!(
            definition.env_aliases.is_empty(),
            "{key} must be sourced from configuration center values, not env aliases"
        );
    }
}

#[test]
fn catalog_exposes_internal_request_security_toggles() {
    let definitions = builtin_definitions();
    for (key, service_name, env_alias) in [
        (
            LOCAL_CONNECTOR_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY,
            "local-connector-service",
            "LOCAL_CONNECTOR_REQUIRE_SIGNED_INTERNAL_REQUESTS",
        ),
        (
            MCP_MANAGEMENT_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY,
            "mcp-management-service",
            "MCP_MANAGEMENT_REQUIRE_SIGNED_INTERNAL_REQUESTS",
        ),
        (
            PROJECT_SERVICE_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY,
            "project-service",
            "PROJECT_SERVICE_REQUIRE_SIGNED_INTERNAL_REQUESTS",
        ),
        (
            MEMORY_ENGINE_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY,
            "memory-engine",
            "MEMORY_ENGINE_REQUIRE_SIGNED_INTERNAL_REQUESTS",
        ),
        (
            SANDBOX_MANAGER_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY,
            "sandbox-manager",
            "SANDBOX_MANAGER_REQUIRE_SIGNED_INTERNAL_REQUESTS",
        ),
    ] {
        let definition = definitions
            .iter()
            .find(|definition| definition.key == key)
            .unwrap_or_else(|| panic!("missing definition for {key}"));
        assert_eq!(definition.scope, "service");
        assert_eq!(definition.service_name.as_deref(), Some(service_name));
        assert_eq!(definition.value_type, "boolean");
        assert_eq!(definition.default_value, json!(false));
        assert_eq!(definition.reload_mode, "restart_required");
        assert_eq!(definition.env_aliases, vec![env_alias.to_string()]);
    }
}

#[test]
fn catalog_exposes_mcp_management_async_dispatch_controls() {
    let definitions = builtin_definitions();
    for key in [
        MCP_MANAGEMENT_ASYNC_TOOL_DISPATCH_MODE_CONFIG_KEY,
        MCP_MANAGEMENT_ASYNC_TOOL_WORKER_CONCURRENCY_CONFIG_KEY,
        MCP_MANAGEMENT_ASYNC_TOOL_LOCAL_QUEUE_BUFFER_CONFIG_KEY,
        MCP_MANAGEMENT_ASYNC_TOOL_RABBITMQ_URL_CONFIG_KEY,
        MCP_MANAGEMENT_ASYNC_TOOL_RABBITMQ_EXCHANGE_CONFIG_KEY,
        MCP_MANAGEMENT_ASYNC_TOOL_DISPATCH_QUEUE_CONFIG_KEY,
    ] {
        let definition = definitions
            .iter()
            .find(|definition| definition.key == key)
            .unwrap_or_else(|| panic!("missing definition for {key}"));
        assert_eq!(definition.scope, "service");
        assert_eq!(
            definition.service_name.as_deref(),
            Some("mcp-management-service")
        );
        assert!(
            !definition.env_aliases.is_empty(),
            "{key} must project into managed env aliases for bootstrap loading"
        );
    }

    let rabbitmq_url = definitions
        .iter()
        .find(|definition| definition.key == MCP_MANAGEMENT_ASYNC_TOOL_RABBITMQ_URL_CONFIG_KEY)
        .expect("mcp management rabbitmq url definition");
    assert_eq!(
        rabbitmq_url.default_value,
        json!("amqp://chatos:change_me_rabbitmq_password@127.0.0.1:5672/")
    );
}

#[test]
fn catalog_exposes_mcp_management_internal_security_controls() {
    let definitions = builtin_definitions();
    for (key, env_alias, expected_default, sensitivity) in [
        (
            MCP_MANAGEMENT_INTERNAL_API_SECRET_CONFIG_KEY,
            "MCP_MANAGEMENT_INTERNAL_API_SECRET",
            json!("change_me_mcp_management_internal_secret"),
            "secret",
        ),
        (
            MCP_MANAGEMENT_ALLOWED_INTERNAL_CALLERS_CONFIG_KEY,
            "MCP_MANAGEMENT_ALLOWED_INTERNAL_CALLERS",
            json!(
                "chatos,task-runner,project-service,memory-engine,local-connector-service,sandbox-manager,plugin-management-service"
            ),
            "public",
        ),
        (
            MCP_MANAGEMENT_PLUGIN_MANAGEMENT_INTERNAL_API_SECRET_CONFIG_KEY,
            "PLUGIN_MANAGEMENT_MCP_MANAGEMENT_INTERNAL_API_SECRET",
            json!("change_me_plugin_management_mcp_management_secret"),
            "secret",
        ),
        (
            MCP_MANAGEMENT_PROJECT_SERVICE_INTERNAL_API_SECRET_CONFIG_KEY,
            "MCP_MANAGEMENT_PROJECT_SERVICE_INTERNAL_API_SECRET",
            json!("change_me_mcp_management_project_service_secret"),
            "secret",
        ),
        (
            MCP_MANAGEMENT_TASK_RUNNER_INTERNAL_API_SECRET_CONFIG_KEY,
            "MCP_MANAGEMENT_TASK_RUNNER_INTERNAL_API_SECRET",
            json!("change_me_mcp_management_task_runner_secret"),
            "secret",
        ),
        (
            MCP_MANAGEMENT_CHATOS_INTERNAL_API_SECRET_CONFIG_KEY,
            "MCP_MANAGEMENT_CHATOS_INTERNAL_API_SECRET",
            json!("change_me_mcp_management_chatos_secret"),
            "secret",
        ),
        (
            MCP_MANAGEMENT_LOCAL_CONNECTOR_INTERNAL_API_SECRET_CONFIG_KEY,
            "MCP_MANAGEMENT_LOCAL_CONNECTOR_INTERNAL_API_SECRET",
            json!("change_me_mcp_management_local_connector_secret"),
            "secret",
        ),
        (
            MCP_MANAGEMENT_SANDBOX_MANAGER_INTERNAL_API_SECRET_CONFIG_KEY,
            "MCP_MANAGEMENT_SANDBOX_MANAGER_INTERNAL_API_SECRET",
            json!("change_me_mcp_management_sandbox_manager_secret"),
            "secret",
        ),
        (
            MCP_MANAGEMENT_RUNTIME_GRANT_SECRET_CONFIG_KEY,
            "MCP_MANAGEMENT_RUNTIME_GRANT_SECRET",
            json!("change_me_mcp_management_runtime_grant_secret"),
            "secret",
        ),
        (
            MCP_MANAGEMENT_RUNTIME_SESSION_ENCRYPTION_SECRET_CONFIG_KEY,
            "MCP_MANAGEMENT_RUNTIME_SESSION_ENCRYPTION_SECRET",
            json!("change_me_mcp_management_runtime_session_encryption_secret"),
            "secret",
        ),
    ] {
        let definition = definitions
            .iter()
            .find(|definition| definition.key == key)
            .unwrap_or_else(|| panic!("missing definition for {key}"));
        assert_eq!(definition.scope, "service");
        assert_eq!(
            definition.service_name.as_deref(),
            Some("mcp-management-service")
        );
        assert_eq!(definition.reload_mode, "restart_required");
        assert_eq!(definition.env_aliases, vec![env_alias.to_string()]);
        assert_eq!(definition.default_value, expected_default);
        assert_eq!(definition.sensitivity, sensitivity);
    }
}

#[test]
fn catalog_exposes_managed_internal_caller_secrets_for_core_services() {
    let definitions = builtin_definitions();
    for (key, service_name, env_alias, expected_default) in [
        (
            PROJECT_SERVICE_CHATOS_INTERNAL_API_SECRET_CONFIG_KEY,
            "project-service",
            "CHATOS_PROJECT_SERVICE_INTERNAL_API_SECRET",
            json!("change_me_chatos_project_service_secret"),
        ),
        (
            PROJECT_SERVICE_TASK_RUNNER_INTERNAL_API_SECRET_CONFIG_KEY,
            "project-service",
            "TASK_RUNNER_PROJECT_SERVICE_INTERNAL_API_SECRET",
            json!("change_me_task_runner_project_service_secret"),
        ),
        (
            PROJECT_SERVICE_SELF_INTERNAL_API_SECRET_CONFIG_KEY,
            "project-service",
            "PROJECT_SERVICE_SELF_INTERNAL_API_SECRET",
            json!("change_me_project_service_self_secret"),
        ),
        (
            PROJECT_SERVICE_MCP_MANAGEMENT_INTERNAL_API_SECRET_CONFIG_KEY,
            "project-service",
            "MCP_MANAGEMENT_PROJECT_SERVICE_INTERNAL_API_SECRET",
            json!("change_me_mcp_management_project_service_secret"),
        ),
        (
            MEMORY_ENGINE_CHATOS_INTERNAL_API_SECRET_CONFIG_KEY,
            "memory-engine",
            "CHATOS_MEMORY_ENGINE_INTERNAL_API_SECRET",
            json!("change_me_chatos_memory_engine_secret"),
        ),
        (
            MEMORY_ENGINE_TASK_RUNNER_INTERNAL_API_SECRET_CONFIG_KEY,
            "memory-engine",
            "TASK_RUNNER_MEMORY_ENGINE_INTERNAL_API_SECRET",
            json!("change_me_task_runner_memory_engine_secret"),
        ),
        (
            MEMORY_ENGINE_PROJECT_SERVICE_INTERNAL_API_SECRET_CONFIG_KEY,
            "memory-engine",
            "PROJECT_SERVICE_MEMORY_ENGINE_INTERNAL_API_SECRET",
            json!("change_me_project_service_memory_engine_secret"),
        ),
        (
            MEMORY_ENGINE_USER_SERVICE_INTERNAL_API_SECRET_CONFIG_KEY,
            "memory-engine",
            "USER_SERVICE_MEMORY_ENGINE_INTERNAL_API_SECRET",
            json!("change_me_user_service_memory_engine_secret"),
        ),
        (
            SANDBOX_MANAGER_TASK_RUNNER_INTERNAL_API_SECRET_CONFIG_KEY,
            "sandbox-manager",
            "TASK_RUNNER_SANDBOX_MANAGER_INTERNAL_API_SECRET",
            json!("change_me_task_runner_sandbox_manager_secret"),
        ),
        (
            SANDBOX_MANAGER_PROJECT_SERVICE_INTERNAL_API_SECRET_CONFIG_KEY,
            "sandbox-manager",
            "PROJECT_SERVICE_SANDBOX_MANAGER_INTERNAL_API_SECRET",
            json!("change_me_project_service_sandbox_manager_secret"),
        ),
        (
            SANDBOX_MANAGER_MCP_MANAGEMENT_INTERNAL_API_SECRET_CONFIG_KEY,
            "sandbox-manager",
            "MCP_MANAGEMENT_SANDBOX_MANAGER_INTERNAL_API_SECRET",
            json!("change_me_mcp_management_sandbox_manager_secret"),
        ),
    ] {
        let definition = definitions
            .iter()
            .find(|definition| definition.key == key)
            .unwrap_or_else(|| panic!("missing definition for {key}"));
        assert_eq!(definition.scope, "service");
        assert_eq!(definition.service_name.as_deref(), Some(service_name));
        assert_eq!(definition.env_aliases, vec![env_alias.to_string()]);
        assert_eq!(definition.default_value, expected_default);
        assert_eq!(definition.sensitivity, "secret");
    }
}

#[test]
fn catalog_exposes_project_service_downstream_auth_controls() {
    let definitions = builtin_definitions();
    for (key, env_alias, expected_default, sensitivity) in [
        (
            PROJECT_SERVICE_USER_SERVICE_INTERNAL_SECRET_CONFIG_KEY,
            "PROJECT_SERVICE_USER_SERVICE_INTERNAL_SECRET",
            json!("change_me_project_service_user_service_secret"),
            "secret",
        ),
        (
            PROJECT_SERVICE_TASK_RUNNER_INTERNAL_SECRET_CONFIG_KEY,
            "PROJECT_SERVICE_TASK_RUNNER_INTERNAL_SECRET",
            json!("change_me_project_service_task_runner_secret"),
            "secret",
        ),
        (
            PROJECT_SERVICE_SYNC_SECRET_CONFIG_KEY,
            "PROJECT_SERVICE_SYNC_SECRET",
            json!("change_me_project_sync_secret"),
            "secret",
        ),
        (
            PROJECT_SERVICE_MEMORY_ENGINE_OPERATOR_TOKEN_CONFIG_KEY,
            "PROJECT_SERVICE_MEMORY_ENGINE_OPERATOR_TOKEN",
            json!(DEFAULT_MEMORY_ENGINE_OPERATOR_TOKEN),
            "secret",
        ),
        (
            PROJECT_SERVICE_SANDBOX_MANAGER_CLIENT_ID_CONFIG_KEY,
            "PROJECT_SERVICE_SANDBOX_MANAGER_CLIENT_ID",
            json!("project-service"),
            "public",
        ),
        (
            PROJECT_SERVICE_SANDBOX_MANAGER_CLIENT_KEY_CONFIG_KEY,
            "PROJECT_SERVICE_SANDBOX_MANAGER_CLIENT_KEY",
            json!(DEFAULT_SANDBOX_MANAGER_SYSTEM_CLIENT_KEY),
            "secret",
        ),
    ] {
        let definition = definitions
            .iter()
            .find(|definition| definition.key == key)
            .unwrap_or_else(|| panic!("missing definition for {key}"));
        assert_eq!(definition.scope, "service");
        assert_eq!(definition.service_name.as_deref(), Some("project-service"));
        assert_eq!(definition.env_aliases, vec![env_alias.to_string()]);
        assert_eq!(definition.default_value, expected_default);
        assert_eq!(definition.sensitivity, sensitivity);
    }
}

#[test]
fn catalog_exposes_managed_memory_and_sandbox_runtime_auth_controls() {
    let definitions = builtin_definitions();
    for (key, service_name, env_alias, expected_default, sensitivity) in [
        (
            MEMORY_ENGINE_OPERATOR_TOKEN_CONFIG_KEY,
            "memory-engine",
            "MEMORY_ENGINE_OPERATOR_TOKEN",
            json!(DEFAULT_MEMORY_ENGINE_OPERATOR_TOKEN),
            "secret",
        ),
        (
            SANDBOX_MANAGER_OPERATOR_TOKEN_CONFIG_KEY,
            "sandbox-manager",
            "SANDBOX_MANAGER_OPERATOR_TOKEN",
            json!(DEFAULT_SANDBOX_MANAGER_OPERATOR_TOKEN),
            "secret",
        ),
        (
            SANDBOX_MANAGER_SYSTEM_CLIENT_ID_CONFIG_KEY,
            "sandbox-manager",
            "SANDBOX_MANAGER_SYSTEM_CLIENT_ID",
            json!(DEFAULT_SANDBOX_MANAGER_SYSTEM_CLIENT_ID),
            "public",
        ),
        (
            SANDBOX_MANAGER_SYSTEM_CLIENT_KEY_CONFIG_KEY,
            "sandbox-manager",
            "SANDBOX_MANAGER_SYSTEM_CLIENT_KEY",
            json!(DEFAULT_SANDBOX_MANAGER_SYSTEM_CLIENT_KEY),
            "secret",
        ),
        (
            SANDBOX_MANAGER_AGENT_TOKEN_SECRET_CONFIG_KEY,
            "sandbox-manager",
            "SANDBOX_MANAGER_AGENT_TOKEN_SECRET",
            json!(DEFAULT_SANDBOX_MANAGER_AGENT_TOKEN_SECRET),
            "secret",
        ),
    ] {
        let definition = definitions
            .iter()
            .find(|definition| definition.key == key)
            .unwrap_or_else(|| panic!("missing definition for {key}"));
        assert_eq!(definition.scope, "service");
        assert_eq!(definition.service_name.as_deref(), Some(service_name));
        assert_eq!(definition.env_aliases, vec![env_alias.to_string()]);
        assert_eq!(definition.default_value, expected_default);
        assert_eq!(definition.sensitivity, sensitivity);
    }
}

#[test]
fn catalog_exposes_user_service_smtp_controls_via_nullable_env_projection() {
    let definitions = builtin_definitions();
    for (key, env_alias) in [
        (USER_SERVICE_SMTP_HOST_CONFIG_KEY, "USER_SERVICE_SMTP_HOST"),
        (USER_SERVICE_SMTP_PORT_CONFIG_KEY, "USER_SERVICE_SMTP_PORT"),
        (
            USER_SERVICE_SMTP_USERNAME_CONFIG_KEY,
            "USER_SERVICE_SMTP_USERNAME",
        ),
        (
            USER_SERVICE_EMAIL_FROM_CONFIG_KEY,
            "USER_SERVICE_EMAIL_FROM",
        ),
        (
            USER_SERVICE_EMAIL_FROM_NAME_CONFIG_KEY,
            "USER_SERVICE_EMAIL_FROM_NAME",
        ),
    ] {
        let definition = definitions
            .iter()
            .find(|definition| definition.key == key)
            .unwrap_or_else(|| panic!("missing definition for {key}"));
        assert_eq!(definition.scope, "service");
        assert_eq!(definition.service_name.as_deref(), Some("user-service"));
        assert_eq!(definition.reload_mode, "restart_required");
        assert!(definition.nullable, "{key} should remain optional");
        assert_eq!(definition.env_aliases, vec![env_alias.to_string()]);
    }

    let smtp_password = definitions
        .iter()
        .find(|definition| definition.env_aliases == vec!["USER_SERVICE_SMTP_PASSWORD"]);
    assert!(
        smtp_password.is_none(),
        "SMTP password should remain outside the public managed catalog"
    );
}
