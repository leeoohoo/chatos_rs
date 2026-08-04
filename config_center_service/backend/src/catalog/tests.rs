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
fn catalog_exposes_task_runner_and_chatos_runtime_routes_via_env_projection() {
    let definitions = builtin_definitions();
    for (key, service_name, env_alias, expected_value_type) in [
        (
            CHATOS_NODE_ENV_CONFIG_KEY,
            "chatos-backend",
            "NODE_ENV",
            "enum",
        ),
        (CHATOS_HOST_CONFIG_KEY, "chatos-backend", "HOST", "string"),
        (
            CHATOS_BACKEND_PORT_CONFIG_KEY,
            "chatos-backend",
            "BACKEND_PORT",
            "integer",
        ),
        (
            TASK_RUNNER_HOST_CONFIG_KEY,
            "task-runner",
            "TASK_RUNNER_HOST",
            "string",
        ),
        (
            TASK_RUNNER_PORT_CONFIG_KEY,
            "task-runner",
            "TASK_RUNNER_PORT",
            "integer",
        ),
        (
            TASK_RUNNER_DATABASE_URL_CONFIG_KEY,
            "task-runner",
            "TASK_RUNNER_DATABASE_URL",
            "string",
        ),
        (
            TASK_RUNNER_MONGODB_DATABASE_CONFIG_KEY,
            "task-runner",
            "TASK_RUNNER_MONGODB_DATABASE",
            "string",
        ),
        (
            TASK_RUNNER_WORKSPACE_DIR_CONFIG_KEY,
            "task-runner",
            "TASK_RUNNER_WORKSPACE_DIR",
            "string",
        ),
        (
            TASK_RUNNER_LOCAL_CONNECTOR_SERVICE_BASE_URL_CONFIG_KEY,
            "task-runner",
            "TASK_RUNNER_LOCAL_CONNECTOR_SERVICE_BASE_URL",
            "string",
        ),
        (
            TASK_RUNNER_LOCAL_CONNECTOR_SERVICE_REQUEST_TIMEOUT_MS_CONFIG_KEY,
            "task-runner",
            "TASK_RUNNER_LOCAL_CONNECTOR_SERVICE_REQUEST_TIMEOUT_MS",
            "duration_ms",
        ),
        (
            TASK_RUNNER_USER_SERVICE_BASE_URL_CONFIG_KEY,
            "task-runner",
            "TASK_RUNNER_USER_SERVICE_BASE_URL",
            "string",
        ),
        (
            TASK_RUNNER_USER_SERVICE_REQUEST_TIMEOUT_MS_CONFIG_KEY,
            "task-runner",
            "TASK_RUNNER_USER_SERVICE_REQUEST_TIMEOUT_MS",
            "duration_ms",
        ),
        (
            TASK_RUNNER_PROJECT_SERVICE_BASE_URL_CONFIG_KEY,
            "task-runner",
            "TASK_RUNNER_PROJECT_SERVICE_BASE_URL",
            "string",
        ),
        (
            TASK_RUNNER_PROJECT_SERVICE_REQUEST_TIMEOUT_MS_CONFIG_KEY,
            "task-runner",
            "TASK_RUNNER_PROJECT_SERVICE_REQUEST_TIMEOUT_MS",
            "duration_ms",
        ),
        (
            TASK_RUNNER_MEMORY_ENGINE_BASE_URL_CONFIG_KEY,
            "task-runner",
            "TASK_RUNNER_MEMORY_ENGINE_BASE_URL",
            "string",
        ),
        (
            TASK_RUNNER_MEMORY_TIMEOUT_MS_CONFIG_KEY,
            "task-runner",
            "TASK_RUNNER_MEMORY_TIMEOUT_MS",
            "duration_ms",
        ),
        (
            TASK_RUNNER_SCHEDULER_POLL_MS_CONFIG_KEY,
            "task-runner",
            "TASK_RUNNER_SCHEDULER_POLL_MS",
            "duration_ms",
        ),
        (
            TASK_RUNNER_AUTO_MEMORY_SUMMARY_CONFIG_KEY,
            "task-runner",
            "TASK_RUNNER_AUTO_MEMORY_SUMMARY",
            "boolean",
        ),
        (
            TASK_RUNNER_SANDBOX_MANAGER_BASE_URL_CONFIG_KEY,
            "task-runner",
            "TASK_RUNNER_SANDBOX_MANAGER_BASE_URL",
            "string",
        ),
        (
            TASK_RUNNER_PLUGIN_RELAY_TIMEOUT_MS_CONFIG_KEY,
            "task-runner",
            "TASK_RUNNER_PLUGIN_RELAY_TIMEOUT_MS",
            "duration_ms",
        ),
        (
            TASK_RUNNER_PLUGIN_HOOK_RELAY_TIMEOUT_MS_CONFIG_KEY,
            "task-runner",
            "TASK_RUNNER_PLUGIN_HOOK_RELAY_TIMEOUT_MS",
            "duration_ms",
        ),
        (
            TASK_RUNNER_PLUGIN_CONNECTOR_DISCOVERY_TIMEOUT_MS_CONFIG_KEY,
            "task-runner",
            "TASK_RUNNER_PLUGIN_CONNECTOR_DISCOVERY_TIMEOUT_MS",
            "duration_ms",
        ),
        (
            TASK_RUNNER_CHATOS_CALLBACK_URL_CONFIG_KEY,
            "task-runner",
            "TASK_RUNNER_CHATOS_CALLBACK_URL",
            "string",
        ),
        (
            TASK_RUNNER_CALLBACK_TIMEOUT_MS_CONFIG_KEY,
            "task-runner",
            "TASK_RUNNER_CALLBACK_TIMEOUT_MS",
            "duration_ms",
        ),
        (
            TASK_RUNNER_ADMIN_USERNAME_CONFIG_KEY,
            "task-runner",
            "TASK_RUNNER_ADMIN_USERNAME",
            "string",
        ),
        (
            TASK_RUNNER_ADMIN_PASSWORD_CONFIG_KEY,
            "task-runner",
            "TASK_RUNNER_ADMIN_PASSWORD",
            "string",
        ),
        (
            TASK_RUNNER_ADMIN_DISPLAY_NAME_CONFIG_KEY,
            "task-runner",
            "TASK_RUNNER_ADMIN_DISPLAY_NAME",
            "string",
        ),
        (
            CHATOS_USER_SERVICE_BASE_URL_CONFIG_KEY,
            "chatos-backend",
            "CHATOS_USER_SERVICE_BASE_URL",
            "string",
        ),
        (
            CHATOS_USER_SERVICE_REQUEST_TIMEOUT_MS_CONFIG_KEY,
            "chatos-backend",
            "CHATOS_USER_SERVICE_REQUEST_TIMEOUT_MS",
            "duration_ms",
        ),
        (
            CHATOS_LOG_MAX_FILES_CONFIG_KEY,
            "chatos-backend",
            "LOG_MAX_FILES",
            "string",
        ),
        (
            CHATOS_CORS_ORIGINS_CONFIG_KEY,
            "chatos-backend",
            "CORS_ORIGINS",
            "string",
        ),
        (
            CHATOS_PROJECT_SERVICE_BASE_URL_CONFIG_KEY,
            "chatos-backend",
            "CHATOS_PROJECT_SERVICE_BASE_URL",
            "string",
        ),
        (
            CHATOS_TASK_RUNNER_BASE_URL_CONFIG_KEY,
            "chatos-backend",
            "CHATOS_TASK_RUNNER_BASE_URL",
            "string",
        ),
        (
            CHATOS_TASK_RUNNER_REQUEST_TIMEOUT_MS_CONFIG_KEY,
            "chatos-backend",
            "CHATOS_TASK_RUNNER_REQUEST_TIMEOUT_MS",
            "duration_ms",
        ),
        (
            CHATOS_LOCAL_CONNECTOR_SERVICE_BASE_URL_CONFIG_KEY,
            "chatos-backend",
            "CHATOS_LOCAL_CONNECTOR_SERVICE_BASE_URL",
            "string",
        ),
        (
            CHATOS_LOCAL_CONNECTOR_SERVICE_REQUEST_TIMEOUT_MS_CONFIG_KEY,
            "chatos-backend",
            "CHATOS_LOCAL_CONNECTOR_SERVICE_REQUEST_TIMEOUT_MS",
            "duration_ms",
        ),
        (
            CHATOS_MEMORY_ENGINE_BASE_URL_CONFIG_KEY,
            "chatos-backend",
            "CHATOS_MEMORY_ENGINE_BASE_URL",
            "string",
        ),
        (
            CHATOS_MEMORY_ENGINE_REQUEST_TIMEOUT_MS_CONFIG_KEY,
            "chatos-backend",
            "CHATOS_MEMORY_ENGINE_REQUEST_TIMEOUT_MS",
            "duration_ms",
        ),
        (
            CHATOS_OPENAI_API_KEY_CONFIG_KEY,
            "chatos-backend",
            "OPENAI_API_KEY",
            "string",
        ),
        (
            CHATOS_OPENAI_BASE_URL_CONFIG_KEY,
            "chatos-backend",
            "OPENAI_BASE_URL",
            "string",
        ),
        (
            CHATOS_SUMMARY_ENABLED_CONFIG_KEY,
            "chatos-backend",
            "SUMMARY_ENABLED",
            "boolean",
        ),
        (
            CHATOS_SUMMARY_MESSAGE_LIMIT_CONFIG_KEY,
            "chatos-backend",
            "SUMMARY_MESSAGE_LIMIT",
            "integer",
        ),
        (
            CHATOS_SUMMARY_MAX_CONTEXT_TOKENS_CONFIG_KEY,
            "chatos-backend",
            "SUMMARY_MAX_CONTEXT_TOKENS",
            "integer",
        ),
        (
            CHATOS_SUMMARY_KEEP_LAST_N_CONFIG_KEY,
            "chatos-backend",
            "SUMMARY_KEEP_LAST_N",
            "integer",
        ),
        (
            CHATOS_SUMMARY_TARGET_TOKENS_CONFIG_KEY,
            "chatos-backend",
            "SUMMARY_TARGET_TOKENS",
            "integer",
        ),
        (
            CHATOS_SUMMARY_MERGE_TARGET_TOKENS_CONFIG_KEY,
            "chatos-backend",
            "SUMMARY_MERGE_TARGET_TOKENS",
            "integer",
        ),
        (
            CHATOS_SUMMARY_TEMPERATURE_CONFIG_KEY,
            "chatos-backend",
            "SUMMARY_TEMPERATURE",
            "string",
        ),
        (
            CHATOS_SUMMARY_COOLDOWN_SECONDS_CONFIG_KEY,
            "chatos-backend",
            "SUMMARY_COOLDOWN_SECONDS",
            "integer",
        ),
        (
            CHATOS_DYNAMIC_SUMMARY_ENABLED_CONFIG_KEY,
            "chatos-backend",
            "DYNAMIC_SUMMARY_ENABLED",
            "boolean",
        ),
        (
            CHATOS_SUMMARY_BISECT_ENABLED_CONFIG_KEY,
            "chatos-backend",
            "SUMMARY_BISECT_ENABLED",
            "boolean",
        ),
        (
            CHATOS_SUMMARY_BISECT_MAX_DEPTH_CONFIG_KEY,
            "chatos-backend",
            "SUMMARY_BISECT_MAX_DEPTH",
            "integer",
        ),
        (
            CHATOS_SUMMARY_BISECT_MIN_MESSAGES_CONFIG_KEY,
            "chatos-backend",
            "SUMMARY_BISECT_MIN_MESSAGES",
            "integer",
        ),
        (
            CHATOS_SUMMARY_RETRY_ON_CONTEXT_OVERFLOW_CONFIG_KEY,
            "chatos-backend",
            "SUMMARY_RETRY_ON_CONTEXT_OVERFLOW",
            "boolean",
        ),
        (
            CHATOS_AUTH_JWT_SECRET_CONFIG_KEY,
            "chatos-backend",
            "AUTH_JWT_SECRET",
            "string",
        ),
        (
            CHATOS_AUTH_COMPAT_SECRET_CONFIG_KEY,
            "chatos-backend",
            "AUTH_COMPAT_SECRET",
            "string",
        ),
        (
            CHATOS_AUTH_ACCESS_TOKEN_TTL_SECONDS_CONFIG_KEY,
            "chatos-backend",
            "AUTH_ACCESS_TOKEN_TTL_SECONDS",
            "integer",
        ),
        (
            CHATOS_PLUGIN_UI_PARENT_ORIGIN_CONFIG_KEY,
            "chatos-backend",
            "CHATOS_PLUGIN_UI_PARENT_ORIGIN",
            "string",
        ),
        (
            CHATOS_PLUGIN_UI_RESOURCE_ORIGIN_CONFIG_KEY,
            "chatos-backend",
            "CHATOS_PLUGIN_UI_RESOURCE_ORIGIN",
            "string",
        ),
        (
            CHATOS_MEMORY_ENGINE_ACTIVE_SUMMARY_TRIGGER_TIMEOUT_MS_CONFIG_KEY,
            "chatos-backend",
            "MEMORY_ENGINE_ACTIVE_SUMMARY_TRIGGER_TIMEOUT_MS",
            "duration_ms",
        ),
        (
            CHATOS_MEMORY_ENGINE_ACTIVE_SUMMARY_POLL_INTERVAL_MS_CONFIG_KEY,
            "chatos-backend",
            "MEMORY_ENGINE_ACTIVE_SUMMARY_POLL_INTERVAL_MS",
            "duration_ms",
        ),
        (
            CHATOS_MEMORY_ENGINE_ACTIVE_SUMMARY_POLL_TIMEOUT_MS_CONFIG_KEY,
            "chatos-backend",
            "MEMORY_ENGINE_ACTIVE_SUMMARY_POLL_TIMEOUT_MS",
            "duration_ms",
        ),
    ] {
        let definition = definitions
            .iter()
            .find(|definition| definition.key == key)
            .unwrap_or_else(|| panic!("missing definition for {key}"));
        assert_eq!(definition.scope, "service");
        assert_eq!(definition.service_name.as_deref(), Some(service_name));
        assert_eq!(definition.value_type, expected_value_type);
        assert_eq!(definition.reload_mode, "restart_required");
        assert_eq!(definition.env_aliases, vec![env_alias.to_string()]);
    }
}

#[test]
fn catalog_exposes_project_service_runtime_routes_via_env_projection() {
    let definitions = builtin_definitions();
    for (key, env_alias, expected_value_type) in [
        (
            PROJECT_SERVICE_HOST_CONFIG_KEY,
            "PROJECT_SERVICE_HOST",
            "string",
        ),
        (
            PROJECT_SERVICE_PORT_CONFIG_KEY,
            "PROJECT_SERVICE_PORT",
            "integer",
        ),
        (
            PROJECT_SERVICE_DATABASE_URL_CONFIG_KEY,
            "PROJECT_SERVICE_DATABASE_URL",
            "string",
        ),
        (
            PROJECT_SERVICE_USER_SERVICE_BASE_URL_CONFIG_KEY,
            "PROJECT_SERVICE_USER_SERVICE_BASE_URL",
            "string",
        ),
        (
            PROJECT_SERVICE_USER_SERVICE_REQUEST_TIMEOUT_MS_CONFIG_KEY,
            "PROJECT_SERVICE_USER_SERVICE_REQUEST_TIMEOUT_MS",
            "duration_ms",
        ),
        (
            PROJECT_SERVICE_LOCAL_CONNECTOR_SERVICE_BASE_URL_CONFIG_KEY,
            "PROJECT_SERVICE_LOCAL_CONNECTOR_SERVICE_BASE_URL",
            "string",
        ),
        (
            PROJECT_SERVICE_LOCAL_CONNECTOR_SERVICE_REQUEST_TIMEOUT_MS_CONFIG_KEY,
            "PROJECT_SERVICE_LOCAL_CONNECTOR_SERVICE_REQUEST_TIMEOUT_MS",
            "duration_ms",
        ),
        (
            PROJECT_SERVICE_MEMORY_ENGINE_BASE_URL_CONFIG_KEY,
            "PROJECT_SERVICE_MEMORY_ENGINE_BASE_URL",
            "string",
        ),
        (
            PROJECT_SERVICE_MEMORY_ENGINE_REQUEST_TIMEOUT_MS_CONFIG_KEY,
            "PROJECT_SERVICE_MEMORY_ENGINE_REQUEST_TIMEOUT_MS",
            "duration_ms",
        ),
        (
            PROJECT_SERVICE_SANDBOX_MANAGER_BASE_URL_CONFIG_KEY,
            "PROJECT_SERVICE_SANDBOX_MANAGER_BASE_URL",
            "string",
        ),
        (
            PROJECT_SERVICE_CLOUD_PROJECT_IMPORT_ENABLED_CONFIG_KEY,
            "PROJECT_SERVICE_CLOUD_PROJECT_IMPORT_ENABLED",
            "boolean",
        ),
        (
            PROJECT_SERVICE_CLOUD_PROJECT_MAX_ZIP_BYTES_CONFIG_KEY,
            "PROJECT_SERVICE_CLOUD_PROJECT_MAX_ZIP_BYTES",
            "integer",
        ),
        (
            PROJECT_SERVICE_CLOUD_PROJECT_MAX_UNPACKED_BYTES_CONFIG_KEY,
            "PROJECT_SERVICE_CLOUD_PROJECT_MAX_UNPACKED_BYTES",
            "integer",
        ),
        (
            PROJECT_SERVICE_CLOUD_PROJECT_MAX_FILES_CONFIG_KEY,
            "PROJECT_SERVICE_CLOUD_PROJECT_MAX_FILES",
            "integer",
        ),
        (
            PROJECT_SERVICE_TASK_RUNNER_BASE_URL_CONFIG_KEY,
            "PROJECT_SERVICE_TASK_RUNNER_BASE_URL",
            "string",
        ),
        (
            PROJECT_SERVICE_TASK_RUNNER_REQUEST_TIMEOUT_MS_CONFIG_KEY,
            "PROJECT_SERVICE_TASK_RUNNER_REQUEST_TIMEOUT_MS",
            "duration_ms",
        ),
        (
            PROJECT_SERVICE_SANDBOX_IMAGE_MCP_REQUEST_TIMEOUT_MS_CONFIG_KEY,
            "PROJECT_SERVICE_SANDBOX_IMAGE_MCP_REQUEST_TIMEOUT_MS",
            "duration_ms",
        ),
        (
            PROJECT_SERVICE_CLOUD_PROJECT_GIT_TIMEOUT_MS_CONFIG_KEY,
            "PROJECT_SERVICE_CLOUD_PROJECT_GIT_TIMEOUT_MS",
            "duration_ms",
        ),
    ] {
        let definition = definitions
            .iter()
            .find(|definition| definition.key == key)
            .unwrap_or_else(|| panic!("missing definition for {key}"));
        assert_eq!(definition.scope, "service");
        assert_eq!(definition.service_name.as_deref(), Some("project-service"));
        assert_eq!(definition.value_type, expected_value_type);
        assert_eq!(definition.reload_mode, "restart_required");
        assert_eq!(definition.env_aliases, vec![env_alias.to_string()]);
    }
}

#[test]
fn catalog_exposes_plugin_management_runtime_routes_via_env_projection() {
    let definitions = builtin_definitions();
    for (key, env_alias, expected_value_type) in [
        (
            PLUGIN_MANAGEMENT_INTERNAL_API_SECRET_CONFIG_KEY,
            "PLUGIN_MANAGEMENT_INTERNAL_API_SECRET",
            "string",
        ),
        (
            PLUGIN_MANAGEMENT_HOST_CONFIG_KEY,
            "PLUGIN_MANAGEMENT_SERVICE_HOST",
            "string",
        ),
        (
            PLUGIN_MANAGEMENT_PORT_CONFIG_KEY,
            "PLUGIN_MANAGEMENT_SERVICE_PORT",
            "integer",
        ),
        (
            PLUGIN_MANAGEMENT_DATABASE_URL_CONFIG_KEY,
            "PLUGIN_MANAGEMENT_SERVICE_DATABASE_URL",
            "string",
        ),
        (
            PLUGIN_MANAGEMENT_MONGODB_DATABASE_CONFIG_KEY,
            "PLUGIN_MANAGEMENT_SERVICE_MONGODB_DATABASE",
            "string",
        ),
        (
            PLUGIN_MANAGEMENT_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY,
            "PLUGIN_MANAGEMENT_REQUIRE_SIGNED_INTERNAL_REQUESTS",
            "boolean",
        ),
        (
            PLUGIN_MANAGEMENT_SERVICE_USER_SERVICE_BASE_URL_CONFIG_KEY,
            "PLUGIN_MANAGEMENT_SERVICE_USER_SERVICE_BASE_URL",
            "string",
        ),
        (
            PLUGIN_MANAGEMENT_SERVICE_USER_SERVICE_REQUEST_TIMEOUT_MS_CONFIG_KEY,
            "PLUGIN_MANAGEMENT_SERVICE_USER_SERVICE_REQUEST_TIMEOUT_MS",
            "duration_ms",
        ),
        (
            PLUGIN_MANAGEMENT_TASK_RUNNER_BASE_URL_CONFIG_KEY,
            "PLUGIN_MANAGEMENT_TASK_RUNNER_BASE_URL",
            "string",
        ),
        (
            PLUGIN_MANAGEMENT_CORS_ORIGINS_CONFIG_KEY,
            "PLUGIN_MANAGEMENT_CORS_ORIGINS",
            "string",
        ),
        (
            PLUGIN_MANAGEMENT_PUBLIC_BASE_URL_CONFIG_KEY,
            "PLUGIN_MANAGEMENT_PUBLIC_BASE_URL",
            "string",
        ),
        (
            PLUGIN_MANAGEMENT_FRONTEND_ORIGIN_CONFIG_KEY,
            "PLUGIN_MANAGEMENT_FRONTEND_ORIGIN",
            "string",
        ),
        (
            PLUGIN_MANAGEMENT_OAUTH_FLOW_TTL_SECONDS_CONFIG_KEY,
            "PLUGIN_MANAGEMENT_OAUTH_FLOW_TTL_SECONDS",
            "integer",
        ),
        (
            PLUGIN_MANAGEMENT_OAUTH_REFRESH_SKEW_SECONDS_CONFIG_KEY,
            "PLUGIN_MANAGEMENT_OAUTH_REFRESH_SKEW_SECONDS",
            "integer",
        ),
        (
            PLUGIN_MANAGEMENT_OAUTH_REQUEST_TIMEOUT_MS_CONFIG_KEY,
            "PLUGIN_MANAGEMENT_OAUTH_REQUEST_TIMEOUT_MS",
            "duration_ms",
        ),
        (
            PLUGIN_MANAGEMENT_OAUTH_MAX_RESPONSE_BYTES_CONFIG_KEY,
            "PLUGIN_MANAGEMENT_OAUTH_MAX_RESPONSE_BYTES",
            "bytes",
        ),
        (
            PLUGIN_MANAGEMENT_LOCAL_CONNECTOR_CHECK_TTL_SECONDS_CONFIG_KEY,
            "PLUGIN_MANAGEMENT_LOCAL_CONNECTOR_CHECK_TTL_SECONDS",
            "integer",
        ),
        (
            PLUGIN_MANAGEMENT_LOCAL_CONNECTOR_MAX_TOOL_SNAPSHOT_BYTES_CONFIG_KEY,
            "PLUGIN_MANAGEMENT_LOCAL_CONNECTOR_MAX_TOOL_SNAPSHOT_BYTES",
            "bytes",
        ),
        (
            PLUGIN_MANAGEMENT_CATALOG_SYNC_ENABLED_CONFIG_KEY,
            "PLUGIN_MANAGEMENT_CATALOG_SYNC_ENABLED",
            "boolean",
        ),
        (
            PLUGIN_MANAGEMENT_CATALOG_SYNC_INTERVAL_SECONDS_CONFIG_KEY,
            "PLUGIN_MANAGEMENT_CATALOG_SYNC_INTERVAL_SECONDS",
            "integer",
        ),
        (
            PLUGIN_MANAGEMENT_CATALOG_REQUEST_TIMEOUT_MS_CONFIG_KEY,
            "PLUGIN_MANAGEMENT_CATALOG_REQUEST_TIMEOUT_MS",
            "duration_ms",
        ),
        (
            PLUGIN_MANAGEMENT_CATALOG_MAX_BYTES_CONFIG_KEY,
            "PLUGIN_MANAGEMENT_CATALOG_MAX_BYTES",
            "bytes",
        ),
        (
            PLUGIN_MANAGEMENT_SUPER_ADMIN_USERNAME_CONFIG_KEY,
            "PLUGIN_MANAGEMENT_SERVICE_SUPER_ADMIN_USERNAME",
            "string",
        ),
        (
            PLUGIN_MANAGEMENT_SUPER_ADMIN_PASSWORD_CONFIG_KEY,
            "PLUGIN_MANAGEMENT_SERVICE_SUPER_ADMIN_PASSWORD",
            "string",
        ),
        (
            PLUGIN_MANAGEMENT_SEED_SYSTEM_RESOURCES_CONFIG_KEY,
            "PLUGIN_MANAGEMENT_SERVICE_SEED_SYSTEM_RESOURCES",
            "boolean",
        ),
    ] {
        let definition = definitions
            .iter()
            .find(|definition| definition.key == key)
            .unwrap_or_else(|| panic!("missing definition for {key}"));
        assert_eq!(definition.scope, "service");
        assert_eq!(
            definition.service_name.as_deref(),
            Some("plugin-management-service")
        );
        assert_eq!(definition.value_type, expected_value_type);
        assert_eq!(definition.reload_mode, "restart_required");
        assert_eq!(definition.env_aliases, vec![env_alias.to_string()]);
    }

    let secret_definition = definitions
        .iter()
        .find(|definition| definition.key == PLUGIN_MANAGEMENT_INTERNAL_API_SECRET_CONFIG_KEY)
        .expect("plugin management internal api secret definition");
    assert_eq!(secret_definition.sensitivity, "secret");
}

#[test]
fn catalog_exposes_task_runner_queue_controls_via_managed_env_projection() {
    let definitions = builtin_definitions();
    for key in [
        TASK_RUNNER_QUEUE_RUN_DISPATCH_MODE_CONFIG_KEY,
        TASK_RUNNER_QUEUE_CALLBACK_DELIVERY_MODE_CONFIG_KEY,
    ] {
        let definition = definitions
            .iter()
            .find(|definition| definition.key == key)
            .unwrap_or_else(|| panic!("missing definition for {key}"));
        assert_eq!(definition.default_value, json!("rabbitmq"));
    }
    let run_events_mode = definitions
        .iter()
        .find(|definition| definition.key == TASK_RUNNER_QUEUE_RUN_EVENTS_PUBLISH_MODE_CONFIG_KEY)
        .expect("missing run event publish mode definition");
    assert_eq!(run_events_mode.default_value, json!("inline"));
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
            TASK_RUNNER_QUEUE_RUN_EVENTS_PUBLISH_MODE_CONFIG_KEY,
            "TASK_RUNNER_RUN_EVENTS_PUBLISH_MODE",
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
fn catalog_exposes_local_connector_runtime_routes_via_env_projection() {
    let definitions = builtin_definitions();
    for (key, env_alias, expected_value_type) in [
        (
            LOCAL_CONNECTOR_HOST_CONFIG_KEY,
            "LOCAL_CONNECTOR_SERVICE_HOST",
            "string",
        ),
        (
            LOCAL_CONNECTOR_PORT_CONFIG_KEY,
            "LOCAL_CONNECTOR_SERVICE_PORT",
            "integer",
        ),
        (
            LOCAL_CONNECTOR_DATABASE_URL_CONFIG_KEY,
            "LOCAL_CONNECTOR_DATABASE_URL",
            "string",
        ),
        (
            LOCAL_CONNECTOR_USER_SERVICE_BASE_URL_CONFIG_KEY,
            "LOCAL_CONNECTOR_USER_SERVICE_BASE_URL",
            "string",
        ),
        (
            LOCAL_CONNECTOR_USER_SERVICE_REQUEST_TIMEOUT_MS_CONFIG_KEY,
            "LOCAL_CONNECTOR_USER_SERVICE_REQUEST_TIMEOUT_MS",
            "duration_ms",
        ),
        (
            LOCAL_CONNECTOR_PUBLIC_BASE_URL_CONFIG_KEY,
            "LOCAL_CONNECTOR_PUBLIC_BASE_URL",
            "string",
        ),
        (
            LOCAL_CONNECTOR_REQUIRE_DEVICE_CONNECT_SIGNATURE_CONFIG_KEY,
            "LOCAL_CONNECTOR_REQUIRE_DEVICE_CONNECT_SIGNATURE",
            "boolean",
        ),
        (
            LOCAL_CONNECTOR_ALLOW_DEVICE_CONNECT_QUERY_TOKEN_CONFIG_KEY,
            "LOCAL_CONNECTOR_ALLOW_DEVICE_CONNECT_QUERY_TOKEN",
            "boolean",
        ),
        (
            LOCAL_CONNECTOR_RELAY_REQUEST_TIMEOUT_MS_CONFIG_KEY,
            "LOCAL_CONNECTOR_RELAY_REQUEST_TIMEOUT_MS",
            "duration_ms",
        ),
        (
            LOCAL_CONNECTOR_PLUGIN_HOOK_RELAY_REQUEST_TIMEOUT_MS_CONFIG_KEY,
            "LOCAL_CONNECTOR_PLUGIN_HOOK_RELAY_REQUEST_TIMEOUT_MS",
            "duration_ms",
        ),
        (
            LOCAL_CONNECTOR_SANDBOX_IMAGE_RELAY_REQUEST_TIMEOUT_MS_CONFIG_KEY,
            "LOCAL_CONNECTOR_SANDBOX_IMAGE_RELAY_REQUEST_TIMEOUT_MS",
            "duration_ms",
        ),
        (
            LOCAL_CONNECTOR_DEVICE_CONNECT_SIGNATURE_MAX_SKEW_SECONDS_CONFIG_KEY,
            "LOCAL_CONNECTOR_DEVICE_SIGNATURE_MAX_SKEW_SECONDS",
            "integer",
        ),
        (
            LOCAL_CONNECTOR_ACTIVE_SESSION_LEASE_TTL_SECONDS_CONFIG_KEY,
            "LOCAL_CONNECTOR_ACTIVE_SESSION_LEASE_TTL_SECONDS",
            "integer",
        ),
        (
            LOCAL_CONNECTOR_MANAGED_REQUIREMENTS_BUNDLE_TTL_SECONDS_CONFIG_KEY,
            "LOCAL_CONNECTOR_MANAGED_REQUIREMENTS_BUNDLE_TTL_SECONDS",
            "integer",
        ),
        (
            LOCAL_CONNECTOR_MANAGED_REQUIREMENTS_TOML_PATH_CONFIG_KEY,
            "LOCAL_CONNECTOR_MANAGED_REQUIREMENTS_TOML_PATH",
            "string",
        ),
        (
            LOCAL_CONNECTOR_MANAGED_REQUIREMENTS_SIGNING_KEY_PATH_CONFIG_KEY,
            "LOCAL_CONNECTOR_MANAGED_REQUIREMENTS_SIGNING_KEY_PATH",
            "string",
        ),
        (
            LOCAL_CONNECTOR_MANAGED_REQUIREMENTS_SIGNING_KEY_ID_CONFIG_KEY,
            "LOCAL_CONNECTOR_MANAGED_REQUIREMENTS_SIGNING_KEY_ID",
            "string",
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
        assert_eq!(definition.value_type, expected_value_type);
        assert_eq!(definition.reload_mode, "restart_required");
        assert_eq!(definition.env_aliases, vec![env_alias.to_string()]);
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
        json!(DEFAULT_LOCAL_RABBITMQ_URL)
    );
}

#[test]
fn catalog_exposes_mcp_management_runtime_routes_via_env_projection() {
    let definitions = builtin_definitions();
    for (key, env_alias, expected_value_type) in [
        (
            MCP_MANAGEMENT_HOST_CONFIG_KEY,
            "MCP_MANAGEMENT_HOST",
            "string",
        ),
        (
            MCP_MANAGEMENT_PORT_CONFIG_KEY,
            "MCP_MANAGEMENT_PORT",
            "integer",
        ),
        (
            MCP_MANAGEMENT_DATABASE_URL_CONFIG_KEY,
            "MCP_MANAGEMENT_DATABASE_URL",
            "string",
        ),
        (
            MCP_MANAGEMENT_EMBEDDED_WORK_DIR_CONFIG_KEY,
            "MCP_MANAGEMENT_EMBEDDED_WORK_DIR",
            "string",
        ),
        (
            MCP_MANAGEMENT_DOWNSTREAM_REQUEST_TIMEOUT_MS_CONFIG_KEY,
            "MCP_MANAGEMENT_DOWNSTREAM_REQUEST_TIMEOUT_MS",
            "duration_ms",
        ),
        (
            MCP_MANAGEMENT_EXTERNAL_HTTP_TOOL_TIMEOUT_MS_CONFIG_KEY,
            "MCP_MANAGEMENT_EXTERNAL_HTTP_TOOL_TIMEOUT_MS",
            "duration_ms",
        ),
        (
            MCP_MANAGEMENT_RUNTIME_SESSION_TTL_SECONDS_CONFIG_KEY,
            "MCP_MANAGEMENT_RUNTIME_SESSION_TTL_SECONDS",
            "integer",
        ),
        (
            MCP_MANAGEMENT_SANDBOX_TOOL_TIMEOUT_MS_CONFIG_KEY,
            "MCP_MANAGEMENT_SANDBOX_TOOL_TIMEOUT_MS",
            "duration_ms",
        ),
        (
            MCP_MANAGEMENT_SANDBOX_IMAGE_TOOL_TIMEOUT_MS_CONFIG_KEY,
            "MCP_MANAGEMENT_SANDBOX_IMAGE_TOOL_TIMEOUT_MS",
            "duration_ms",
        ),
        (
            MCP_MANAGEMENT_TASK_RUNNER_TOOL_TIMEOUT_MS_CONFIG_KEY,
            "MCP_MANAGEMENT_TASK_RUNNER_TOOL_TIMEOUT_MS",
            "duration_ms",
        ),
        (
            MCP_MANAGEMENT_TASK_RUNNER_ASK_USER_TOOL_TIMEOUT_MS_CONFIG_KEY,
            "MCP_MANAGEMENT_TASK_RUNNER_ASK_USER_TOOL_TIMEOUT_MS",
            "duration_ms",
        ),
        (
            MCP_MANAGEMENT_CHATOS_ASK_USER_TOOL_TIMEOUT_MS_CONFIG_KEY,
            "MCP_MANAGEMENT_CHATOS_ASK_USER_TOOL_TIMEOUT_MS",
            "duration_ms",
        ),
        (
            MCP_MANAGEMENT_CHATOS_BROWSER_TOOL_TIMEOUT_MS_CONFIG_KEY,
            "MCP_MANAGEMENT_CHATOS_BROWSER_TOOL_TIMEOUT_MS",
            "duration_ms",
        ),
        (
            MCP_MANAGEMENT_PROVIDER_RESPONSE_LIMIT_BYTES_CONFIG_KEY,
            "MCP_MANAGEMENT_PROVIDER_RESPONSE_LIMIT_BYTES",
            "integer",
        ),
        (
            MCP_MANAGEMENT_PUBLIC_BASE_URL_CONFIG_KEY,
            "MCP_MANAGEMENT_PUBLIC_BASE_URL",
            "string",
        ),
        (
            MCP_MANAGEMENT_PLUGIN_MANAGEMENT_SERVICE_BASE_URL_CONFIG_KEY,
            "MCP_MANAGEMENT_PLUGIN_MANAGEMENT_SERVICE_BASE_URL",
            "string",
        ),
        (
            MCP_MANAGEMENT_PROJECT_SERVICE_BASE_URL_CONFIG_KEY,
            "MCP_MANAGEMENT_PROJECT_SERVICE_BASE_URL",
            "string",
        ),
        (
            MCP_MANAGEMENT_TASK_RUNNER_SERVICE_BASE_URL_CONFIG_KEY,
            "MCP_MANAGEMENT_TASK_RUNNER_SERVICE_BASE_URL",
            "string",
        ),
        (
            MCP_MANAGEMENT_CHATOS_SERVICE_BASE_URL_CONFIG_KEY,
            "MCP_MANAGEMENT_CHATOS_SERVICE_BASE_URL",
            "string",
        ),
        (
            MCP_MANAGEMENT_LOCAL_CONNECTOR_SERVICE_BASE_URL_CONFIG_KEY,
            "MCP_MANAGEMENT_LOCAL_CONNECTOR_SERVICE_BASE_URL",
            "string",
        ),
        (
            MCP_MANAGEMENT_SANDBOX_MANAGER_SERVICE_BASE_URL_CONFIG_KEY,
            "MCP_MANAGEMENT_SANDBOX_MANAGER_SERVICE_BASE_URL",
            "string",
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
        assert_eq!(definition.value_type, expected_value_type);
        assert_eq!(definition.reload_mode, "restart_required");
        assert_eq!(definition.env_aliases, vec![env_alias.to_string()]);
    }
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
fn catalog_exposes_runtime_secrets_for_task_runner_chatos_plugin_and_user_services() {
    let definitions = builtin_definitions();
    for (key, service_name, env_alias, expected_default) in [
        (
            TASK_RUNNER_PROJECT_SERVICE_INTERNAL_API_SECRET_CONFIG_KEY,
            "task-runner",
            "TASK_RUNNER_PROJECT_SERVICE_INTERNAL_API_SECRET",
            json!("change_me_task_runner_project_service_secret"),
        ),
        (
            TASK_RUNNER_MEMORY_ENGINE_INTERNAL_API_SECRET_CONFIG_KEY,
            "task-runner",
            "TASK_RUNNER_MEMORY_ENGINE_INTERNAL_API_SECRET",
            json!(DEFAULT_MEMORY_ENGINE_OPERATOR_TOKEN),
        ),
        (
            TASK_RUNNER_LOCAL_CONNECTOR_INTERNAL_API_SECRET_CONFIG_KEY,
            "task-runner",
            "TASK_RUNNER_LOCAL_CONNECTOR_INTERNAL_API_SECRET",
            json!("change_me_task_runner_local_connector_secret"),
        ),
        (
            TASK_RUNNER_SANDBOX_MANAGER_INTERNAL_API_SECRET_CONFIG_KEY,
            "task-runner",
            "TASK_RUNNER_SANDBOX_MANAGER_INTERNAL_API_SECRET",
            json!(DEFAULT_SANDBOX_MANAGER_SYSTEM_CLIENT_KEY),
        ),
        (
            TASK_RUNNER_PROJECT_SERVICE_CALLER_SECRET_CONFIG_KEY,
            "task-runner",
            "PROJECT_SERVICE_TASK_RUNNER_INTERNAL_API_SECRET",
            json!("change_me_project_service_task_runner_secret"),
        ),
        (
            TASK_RUNNER_CHATOS_INTERNAL_API_SECRET_CONFIG_KEY,
            "task-runner",
            "CHATOS_TASK_RUNNER_INTERNAL_API_SECRET",
            json!("change_me_chatos_task_runner_internal_secret"),
        ),
        (
            TASK_RUNNER_MCP_MANAGEMENT_INTERNAL_API_SECRET_CONFIG_KEY,
            "task-runner",
            "MCP_MANAGEMENT_TASK_RUNNER_INTERNAL_API_SECRET",
            json!("change_me_mcp_management_task_runner_secret"),
        ),
        (
            CHATOS_PROJECT_SERVICE_INTERNAL_API_SECRET_CONFIG_KEY,
            "chatos-backend",
            "CHATOS_PROJECT_SERVICE_INTERNAL_API_SECRET",
            json!("change_me_chatos_project_service_secret"),
        ),
        (
            CHATOS_TASK_RUNNER_INTERNAL_API_SECRET_CONFIG_KEY,
            "chatos-backend",
            "CHATOS_TASK_RUNNER_INTERNAL_API_SECRET",
            json!("change_me_chatos_task_runner_internal_secret"),
        ),
        (
            CHATOS_MCP_MANAGEMENT_INTERNAL_API_SECRET_CONFIG_KEY,
            "chatos-backend",
            "MCP_MANAGEMENT_CHATOS_INTERNAL_API_SECRET",
            json!("change_me_mcp_management_chatos_secret"),
        ),
        (
            CHATOS_LOCAL_CONNECTOR_INTERNAL_API_SECRET_CONFIG_KEY,
            "chatos-backend",
            "CHATOS_LOCAL_CONNECTOR_INTERNAL_API_SECRET",
            json!("change_me_chatos_local_connector_secret"),
        ),
        (
            CHATOS_MEMORY_ENGINE_INTERNAL_API_SECRET_CONFIG_KEY,
            "chatos-backend",
            "CHATOS_MEMORY_ENGINE_INTERNAL_API_SECRET",
            json!(DEFAULT_MEMORY_ENGINE_OPERATOR_TOKEN),
        ),
        (
            PLUGIN_MANAGEMENT_TASK_RUNNER_INTERNAL_API_SECRET_CONFIG_KEY,
            "plugin-management-service",
            "PLUGIN_MANAGEMENT_TASK_RUNNER_INTERNAL_API_SECRET",
            json!("change_me_plugin_management_task_runner_secret"),
        ),
        (
            PLUGIN_MANAGEMENT_PROJECT_SERVICE_INTERNAL_API_SECRET_CONFIG_KEY,
            "plugin-management-service",
            "PLUGIN_MANAGEMENT_PROJECT_SERVICE_INTERNAL_API_SECRET",
            json!("change_me_plugin_management_project_service_secret"),
        ),
        (
            PLUGIN_MANAGEMENT_LOCAL_CONNECTOR_INTERNAL_API_SECRET_CONFIG_KEY,
            "plugin-management-service",
            "PLUGIN_MANAGEMENT_LOCAL_CONNECTOR_SERVICE_INTERNAL_API_SECRET",
            json!("change_me_plugin_management_local_connector_secret"),
        ),
        (
            PLUGIN_MANAGEMENT_MEMORY_ENGINE_INTERNAL_API_SECRET_CONFIG_KEY,
            "plugin-management-service",
            "PLUGIN_MANAGEMENT_MEMORY_ENGINE_INTERNAL_API_SECRET",
            json!("change_me_plugin_management_memory_engine_secret"),
        ),
        (
            PLUGIN_MANAGEMENT_MCP_MANAGEMENT_INTERNAL_API_SECRET_CONFIG_KEY,
            "plugin-management-service",
            "PLUGIN_MANAGEMENT_MCP_MANAGEMENT_INTERNAL_API_SECRET",
            json!("change_me_plugin_management_mcp_management_secret"),
        ),
        (
            PLUGIN_MANAGEMENT_CLOUD_CREDENTIAL_ENCRYPTION_SECRET_CONFIG_KEY,
            "plugin-management-service",
            "PLUGIN_MANAGEMENT_CLOUD_CREDENTIAL_ENCRYPTION_SECRET",
            json!("change_me_plugin_management_cloud_credential_encryption_secret"),
        ),
        (
            USER_SERVICE_JWT_SECRET_CONFIG_KEY,
            "user-service",
            "USER_SERVICE_JWT_SECRET",
            json!("change_me_user_service_secret"),
        ),
        (
            USER_SERVICE_SECRET_KEY_CONFIG_KEY,
            "user-service",
            "USER_SERVICE_SECRET_KEY",
            json!("change_me_user_service_secret_key"),
        ),
        (
            USER_SERVICE_PREVIOUS_SECRET_KEYS_CONFIG_KEY,
            "user-service",
            "USER_SERVICE_PREVIOUS_SECRET_KEYS",
            Value::Null,
        ),
        (
            USER_SERVICE_PROJECT_SERVICE_INTERNAL_SECRET_CONFIG_KEY,
            "user-service",
            "PROJECT_SERVICE_USER_SERVICE_INTERNAL_API_SECRET",
            json!("change_me_project_service_user_service_secret"),
        ),
        (
            USER_SERVICE_MEMORY_ENGINE_INTERNAL_API_SECRET_CONFIG_KEY,
            "user-service",
            "USER_SERVICE_MEMORY_ENGINE_INTERNAL_API_SECRET",
            json!(DEFAULT_MEMORY_ENGINE_OPERATOR_TOKEN),
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
fn catalog_exposes_memory_engine_runtime_routes_via_env_projection() {
    let definitions = builtin_definitions();
    for (key, env_alias, expected_value_type, expect_nullable) in [
        (
            MEMORY_ENGINE_HOST_CONFIG_KEY,
            "MEMORY_ENGINE_HOST",
            "string",
            false,
        ),
        (
            MEMORY_ENGINE_PORT_CONFIG_KEY,
            "MEMORY_ENGINE_PORT",
            "integer",
            false,
        ),
        (
            MEMORY_ENGINE_MONGODB_URI_CONFIG_KEY,
            "MEMORY_ENGINE_MONGODB_URI",
            "string",
            false,
        ),
        (
            MEMORY_ENGINE_MONGODB_DATABASE_CONFIG_KEY,
            "MEMORY_ENGINE_MONGODB_DATABASE",
            "string",
            false,
        ),
        (
            MEMORY_ENGINE_USER_SERVICE_BASE_URL_CONFIG_KEY,
            "MEMORY_ENGINE_USER_SERVICE_BASE_URL",
            "string",
            false,
        ),
        (
            MEMORY_ENGINE_USER_SERVICE_REQUEST_TIMEOUT_MS_CONFIG_KEY,
            "MEMORY_ENGINE_USER_SERVICE_REQUEST_TIMEOUT_MS",
            "duration_ms",
            false,
        ),
        (
            MEMORY_ENGINE_AI_REQUEST_TIMEOUT_SECS_CONFIG_KEY,
            "MEMORY_ENGINE_AI_TIMEOUT_SECS",
            "integer",
            false,
        ),
        (
            MEMORY_ENGINE_OPENAI_API_KEY_CONFIG_KEY,
            "MEMORY_ENGINE_OPENAI_API_KEY",
            "string",
            true,
        ),
        (
            MEMORY_ENGINE_OPENAI_BASE_URL_CONFIG_KEY,
            "MEMORY_ENGINE_OPENAI_BASE_URL",
            "string",
            false,
        ),
        (
            MEMORY_ENGINE_OPENAI_MODEL_CONFIG_KEY,
            "MEMORY_ENGINE_OPENAI_MODEL",
            "string",
            false,
        ),
        (
            MEMORY_ENGINE_OPENAI_TEMPERATURE_CONFIG_KEY,
            "MEMORY_ENGINE_OPENAI_TEMPERATURE",
            "string",
            false,
        ),
        (
            MEMORY_ENGINE_WORKER_ENABLED_CONFIG_KEY,
            "MEMORY_ENGINE_WORKER_ENABLED",
            "boolean",
            false,
        ),
        (
            MEMORY_ENGINE_WORKER_INTERVAL_SECS_CONFIG_KEY,
            "MEMORY_ENGINE_WORKER_INTERVAL_SECS",
            "integer",
            false,
        ),
        (
            MEMORY_ENGINE_WORKER_MAX_THREADS_PER_TICK_CONFIG_KEY,
            "MEMORY_ENGINE_WORKER_MAX_THREADS_PER_TICK",
            "integer",
            false,
        ),
        (
            MEMORY_ENGINE_WORKER_SUMMARY_CONCURRENCY_CONFIG_KEY,
            "MEMORY_ENGINE_WORKER_SUMMARY_CONCURRENCY",
            "integer",
            false,
        ),
        (
            MEMORY_ENGINE_WORKER_ROLLUP_CONCURRENCY_CONFIG_KEY,
            "MEMORY_ENGINE_WORKER_ROLLUP_CONCURRENCY",
            "integer",
            false,
        ),
        (
            MEMORY_ENGINE_WORKER_SUBJECT_MEMORY_CONCURRENCY_CONFIG_KEY,
            "MEMORY_ENGINE_WORKER_SUBJECT_MEMORY_CONCURRENCY",
            "integer",
            false,
        ),
        (
            MEMORY_ENGINE_WORKER_RECONCILE_CONCURRENCY_CONFIG_KEY,
            "MEMORY_ENGINE_WORKER_RECONCILE_CONCURRENCY",
            "integer",
            false,
        ),
    ] {
        let definition = definitions
            .iter()
            .find(|definition| definition.key == key)
            .unwrap_or_else(|| panic!("missing definition for {key}"));
        assert_eq!(definition.scope, "service");
        assert_eq!(definition.service_name.as_deref(), Some("memory-engine"));
        assert_eq!(definition.value_type, expected_value_type);
        assert_eq!(definition.nullable, expect_nullable);
        assert_eq!(definition.reload_mode, "restart_required");
        assert_eq!(definition.env_aliases, vec![env_alias.to_string()]);
    }
}

#[test]
fn catalog_exposes_sandbox_manager_runtime_routes_via_env_projection() {
    let definitions = builtin_definitions();
    for (key, env_alias, expected_value_type) in [
        (
            SANDBOX_MANAGER_HOST_CONFIG_KEY,
            "SANDBOX_MANAGER_HOST",
            "string",
        ),
        (
            SANDBOX_MANAGER_PORT_CONFIG_KEY,
            "SANDBOX_MANAGER_PORT",
            "integer",
        ),
        (
            SANDBOX_MANAGER_DATABASE_URL_CONFIG_KEY,
            "SANDBOX_MANAGER_DATABASE_URL",
            "string",
        ),
        (
            SANDBOX_MANAGER_MONGODB_DATABASE_CONFIG_KEY,
            "SANDBOX_MANAGER_MONGODB_DATABASE",
            "string",
        ),
        (
            SANDBOX_MANAGER_AGENT_PORT_CONFIG_KEY,
            "SANDBOX_MANAGER_AGENT_PORT",
            "integer",
        ),
        (
            SANDBOX_MANAGER_REQUIRE_AUTH_CONFIG_KEY,
            "SANDBOX_MANAGER_REQUIRE_AUTH",
            "boolean",
        ),
        (
            SANDBOX_MANAGER_LEASE_TTL_SECONDS_CONFIG_KEY,
            "SANDBOX_MANAGER_LEASE_TTL_SECONDS",
            "integer",
        ),
        (
            SANDBOX_MANAGER_CLEANUP_INTERVAL_SECONDS_CONFIG_KEY,
            "SANDBOX_MANAGER_CLEANUP_INTERVAL_SECONDS",
            "integer",
        ),
        (
            SANDBOX_MANAGER_DOCKER_MAINTENANCE_ENABLED_CONFIG_KEY,
            "SANDBOX_MANAGER_DOCKER_MAINTENANCE_ENABLED",
            "boolean",
        ),
        (
            SANDBOX_MANAGER_DOCKER_BUILD_CACHE_MAX_USED_SPACE_CONFIG_KEY,
            "SANDBOX_MANAGER_DOCKER_BUILD_CACHE_MAX_USED_SPACE",
            "string",
        ),
        (
            SANDBOX_MANAGER_DOCKER_BUILD_CACHE_RESERVED_SPACE_CONFIG_KEY,
            "SANDBOX_MANAGER_DOCKER_BUILD_CACHE_RESERVED_SPACE",
            "string",
        ),
        (
            SANDBOX_MANAGER_DOCKER_BUILD_CACHE_TIMEOUT_SECS_CONFIG_KEY,
            "SANDBOX_MANAGER_DOCKER_BUILD_CACHE_TIMEOUT_SECS",
            "integer",
        ),
        (
            SANDBOX_MANAGER_USER_SERVICE_BASE_URL_CONFIG_KEY,
            "SANDBOX_MANAGER_USER_SERVICE_BASE_URL",
            "string",
        ),
        (
            SANDBOX_MANAGER_USER_SERVICE_REQUEST_TIMEOUT_MS_CONFIG_KEY,
            "SANDBOX_MANAGER_USER_SERVICE_REQUEST_TIMEOUT_MS",
            "duration_ms",
        ),
        (
            SANDBOX_MANAGER_SYSTEM_CLIENT_SCOPES_CONFIG_KEY,
            "SANDBOX_MANAGER_SYSTEM_CLIENT_SCOPES",
            "string",
        ),
        (
            SANDBOX_MANAGER_SYSTEM_CLIENT_ALLOWED_TENANT_IDS_CONFIG_KEY,
            "SANDBOX_MANAGER_SYSTEM_CLIENT_ALLOWED_TENANT_IDS",
            "string",
        ),
        (
            SANDBOX_MANAGER_SYSTEM_CLIENT_ALLOWED_PROJECT_IDS_CONFIG_KEY,
            "SANDBOX_MANAGER_SYSTEM_CLIENT_ALLOWED_PROJECT_IDS",
            "string",
        ),
        (
            SANDBOX_MANAGER_SYSTEM_CLIENT_ALLOWED_TOOLS_CONFIG_KEY,
            "SANDBOX_MANAGER_SYSTEM_CLIENT_ALLOWED_TOOLS",
            "string",
        ),
        (
            SANDBOX_MANAGER_SYSTEM_CLIENT_MAX_LEASE_TTL_SECONDS_CONFIG_KEY,
            "SANDBOX_MANAGER_SYSTEM_CLIENT_MAX_LEASE_TTL_SECONDS",
            "integer",
        ),
    ] {
        let definition = definitions
            .iter()
            .find(|definition| definition.key == key)
            .unwrap_or_else(|| panic!("missing definition for {key}"));
        assert_eq!(definition.scope, "service");
        assert_eq!(definition.service_name.as_deref(), Some("sandbox-manager"));
        assert_eq!(definition.value_type, expected_value_type);
        assert_eq!(definition.reload_mode, "restart_required");
        assert_eq!(definition.env_aliases, vec![env_alias.to_string()]);
    }
}

#[test]
fn catalog_exposes_user_service_runtime_routes_via_env_projection() {
    let definitions = builtin_definitions();
    for (key, env_alias, expected_value_type, expect_nullable) in [
        (
            USER_SERVICE_MEMORY_ENGINE_BASE_URL_CONFIG_KEY,
            "USER_SERVICE_MEMORY_ENGINE_BASE_URL",
            "string",
            true,
        ),
        (
            USER_SERVICE_TASK_RUNNER_BASE_URL_CONFIG_KEY,
            "USER_SERVICE_TASK_RUNNER_BASE_URL",
            "string",
            true,
        ),
        (
            USER_SERVICE_TASK_RUNNER_CALLBACK_SECRET_CONFIG_KEY,
            "USER_SERVICE_TASK_RUNNER_CALLBACK_SECRET",
            "string",
            true,
        ),
        (
            USER_SERVICE_DOWNSTREAM_REQUEST_TIMEOUT_MS_CONFIG_KEY,
            "USER_SERVICE_DOWNSTREAM_REQUEST_TIMEOUT_MS",
            "duration_ms",
            false,
        ),
        (
            USER_SERVICE_JWT_ISSUER_CONFIG_KEY,
            "USER_SERVICE_JWT_ISSUER",
            "string",
            false,
        ),
        (
            USER_SERVICE_USER_AUDIENCE_CONFIG_KEY,
            "USER_SERVICE_USER_AUDIENCE",
            "string",
            false,
        ),
        (
            USER_SERVICE_TASK_RUNNER_AUDIENCE_CONFIG_KEY,
            "USER_SERVICE_TASK_RUNNER_AUDIENCE",
            "string",
            false,
        ),
        (
            USER_SERVICE_USER_ACCESS_TTL_SECONDS_CONFIG_KEY,
            "USER_SERVICE_USER_ACCESS_TTL_SECONDS",
            "integer",
            false,
        ),
        (
            USER_SERVICE_TASK_RUNNER_ACCESS_TTL_SECONDS_CONFIG_KEY,
            "USER_SERVICE_TASK_RUNNER_ACCESS_TTL_SECONDS",
            "integer",
            false,
        ),
        (
            USER_SERVICE_REGISTER_CODE_TTL_SECONDS_CONFIG_KEY,
            "USER_SERVICE_REGISTER_CODE_TTL_SECONDS",
            "integer",
            false,
        ),
        (
            USER_SERVICE_REGISTER_CODE_RESEND_SECONDS_CONFIG_KEY,
            "USER_SERVICE_REGISTER_CODE_RESEND_SECONDS",
            "integer",
            false,
        ),
        (
            USER_SERVICE_REGISTER_CODE_HOURLY_LIMIT_CONFIG_KEY,
            "USER_SERVICE_REGISTER_CODE_HOURLY_LIMIT",
            "integer",
            false,
        ),
        (
            USER_SERVICE_REGISTER_CODE_MAX_ATTEMPTS_CONFIG_KEY,
            "USER_SERVICE_REGISTER_CODE_MAX_ATTEMPTS",
            "integer",
            false,
        ),
        (
            USER_SERVICE_LOGIN_MAX_FAILED_ATTEMPTS_CONFIG_KEY,
            "USER_SERVICE_LOGIN_MAX_FAILED_ATTEMPTS",
            "integer",
            false,
        ),
        (
            USER_SERVICE_LOGIN_FAILURE_WINDOW_SECONDS_CONFIG_KEY,
            "USER_SERVICE_LOGIN_FAILURE_WINDOW_SECONDS",
            "integer",
            false,
        ),
        (
            USER_SERVICE_LOGIN_LOCKOUT_SECONDS_CONFIG_KEY,
            "USER_SERVICE_LOGIN_LOCKOUT_SECONDS",
            "integer",
            false,
        ),
        (
            USER_SERVICE_HARNESS_PROVISIONING_ENABLED_CONFIG_KEY,
            "USER_SERVICE_HARNESS_PROVISIONING_ENABLED",
            "boolean",
            false,
        ),
        (
            USER_SERVICE_SUPER_ADMIN_USERNAME_CONFIG_KEY,
            "USER_SERVICE_SUPER_ADMIN_USERNAME",
            "string",
            false,
        ),
        (
            USER_SERVICE_SUPER_ADMIN_PASSWORD_CONFIG_KEY,
            "USER_SERVICE_SUPER_ADMIN_PASSWORD",
            "string",
            false,
        ),
        (
            USER_SERVICE_SUPER_ADMIN_DISPLAY_NAME_CONFIG_KEY,
            "USER_SERVICE_SUPER_ADMIN_DISPLAY_NAME",
            "string",
            false,
        ),
        (
            USER_SERVICE_HARNESS_BASE_URL_CONFIG_KEY,
            "USER_SERVICE_HARNESS_BASE_URL",
            "string",
            true,
        ),
        (
            USER_SERVICE_HARNESS_SYNTHETIC_EMAIL_DOMAIN_CONFIG_KEY,
            "USER_SERVICE_HARNESS_SYNTHETIC_EMAIL_DOMAIN",
            "string",
            false,
        ),
        (
            USER_SERVICE_HARNESS_SPACE_PREFIX_CONFIG_KEY,
            "USER_SERVICE_HARNESS_SPACE_PREFIX",
            "string",
            false,
        ),
        (
            USER_SERVICE_HARNESS_REQUEST_TIMEOUT_MS_CONFIG_KEY,
            "USER_SERVICE_HARNESS_REQUEST_TIMEOUT_MS",
            "duration_ms",
            false,
        ),
        (
            USER_SERVICE_HARNESS_PROJECT_PAT_PREFIX_CONFIG_KEY,
            "USER_SERVICE_HARNESS_PROJECT_PAT_PREFIX",
            "string",
            false,
        ),
    ] {
        let definition = definitions
            .iter()
            .find(|definition| definition.key == key)
            .unwrap_or_else(|| panic!("missing definition for {key}"));
        assert_eq!(definition.scope, "service");
        assert_eq!(definition.service_name.as_deref(), Some("user-service"));
        assert_eq!(definition.reload_mode, "restart_required");
        assert_eq!(definition.value_type, expected_value_type);
        assert_eq!(definition.nullable, expect_nullable);
        assert_eq!(definition.env_aliases, vec![env_alias.to_string()]);
    }

    let callback_secret = definitions
        .iter()
        .find(|definition| definition.key == USER_SERVICE_TASK_RUNNER_CALLBACK_SECRET_CONFIG_KEY)
        .expect("user service task runner callback secret");
    assert_eq!(callback_secret.sensitivity, "secret");
}

#[test]
fn catalog_exposes_user_service_smtp_controls_via_nullable_env_projection() {
    let definitions = builtin_definitions();
    for (key, env_alias) in [
        (USER_SERVICE_SMTP_HOST_CONFIG_KEY, "USER_SERVICE_SMTP_HOST"),
        (
            USER_SERVICE_SMTP_USERNAME_CONFIG_KEY,
            "USER_SERVICE_SMTP_USERNAME",
        ),
        (
            USER_SERVICE_SMTP_PASSWORD_CONFIG_KEY,
            "USER_SERVICE_SMTP_PASSWORD",
        ),
        (
            USER_SERVICE_EMAIL_FROM_CONFIG_KEY,
            "USER_SERVICE_EMAIL_FROM",
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

    let smtp_port = definitions
        .iter()
        .find(|definition| definition.key == USER_SERVICE_SMTP_PORT_CONFIG_KEY)
        .expect("smtp port definition");
    assert!(!smtp_port.nullable);
    assert_eq!(smtp_port.default_value, json!(587));

    let email_from_name = definitions
        .iter()
        .find(|definition| definition.key == USER_SERVICE_EMAIL_FROM_NAME_CONFIG_KEY)
        .expect("email from name definition");
    assert!(!email_from_name.nullable);
    assert_eq!(email_from_name.default_value, json!("Chat OS"));

    let smtp_password = definitions
        .iter()
        .find(|definition| definition.key == USER_SERVICE_SMTP_PASSWORD_CONFIG_KEY)
        .expect("smtp password definition");
    assert_eq!(smtp_password.sensitivity, "secret");
}
