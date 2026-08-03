// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::support::*;

use super::*;
use crate::catalog::{
    LOCAL_CONNECTOR_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY,
    MCP_MANAGEMENT_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY,
    MEMORY_ENGINE_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY,
    PROJECT_SERVICE_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY,
    SANDBOX_MANAGER_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY,
    TASK_RUNNER_EXECUTION_TIMEOUT_CONFIG_KEY, TASK_RUNNER_QUEUE_CALLBACK_DELIVERY_MODE_CONFIG_KEY,
    TASK_RUNNER_QUEUE_CALLBACK_DELIVERY_QUEUE_CONFIG_KEY,
    TASK_RUNNER_QUEUE_RABBITMQ_EXCHANGE_CONFIG_KEY, TASK_RUNNER_QUEUE_RABBITMQ_URL_CONFIG_KEY,
    TASK_RUNNER_QUEUE_RUN_DISPATCH_MODE_CONFIG_KEY,
    TASK_RUNNER_QUEUE_RUN_DISPATCH_QUEUE_CONFIG_KEY, TASK_RUNNER_QUEUE_RUN_EVENTS_QUEUE_CONFIG_KEY,
    USER_SERVICE_EMAIL_FROM_CONFIG_KEY, USER_SERVICE_EMAIL_FROM_NAME_CONFIG_KEY,
    USER_SERVICE_SMTP_HOST_CONFIG_KEY, USER_SERVICE_SMTP_PORT_CONFIG_KEY,
    USER_SERVICE_SMTP_USERNAME_CONFIG_KEY,
};

#[test]
fn legacy_agent_iteration_values_collapse_to_one_key() {
    let mut values = BTreeMap::from([
        ("chatos.ai.max_iterations".to_string(), json!(700)),
        (
            "task_runner.execution.max_iterations".to_string(),
            json!(300),
        ),
    ]);

    assert!(migrate_agent_iteration_values(&mut values, true));
    assert_eq!(
        values.get(chatos_agent::AGENT_MAX_ITERATIONS_CONFIG_KEY),
        Some(&json!(700))
    );
    assert!(!values.contains_key("chatos.ai.max_iterations"));
    assert!(!values.contains_key("task_runner.execution.max_iterations"));
}

#[test]
fn explicit_shared_agent_value_wins_over_legacy_values() {
    let mut values = BTreeMap::from([
        (
            chatos_agent::AGENT_MAX_ITERATIONS_CONFIG_KEY.to_string(),
            json!(900),
        ),
        ("chatos.ai.max_iterations".to_string(), json!(700)),
    ]);

    assert!(migrate_agent_iteration_values(&mut values, true));
    assert_eq!(
        values.get(chatos_agent::AGENT_MAX_ITERATIONS_CONFIG_KEY),
        Some(&json!(900))
    );
}

#[test]
fn empty_draft_does_not_gain_an_unrequested_change() {
    let mut values = BTreeMap::new();
    assert!(!migrate_agent_iteration_values(&mut values, false));
    assert!(values.is_empty());
}

#[test]
fn audit_keys_replace_legacy_agent_keys_once() {
    let mut keys = vec![
        "chatos.ai.max_iterations".to_string(),
        "task_runner.execution.max_iterations".to_string(),
        "shared.logging.level".to_string(),
    ];

    assert!(migrate_agent_iteration_changed_keys(&mut keys));
    assert_eq!(
        keys.iter()
            .filter(|key| *key == chatos_agent::AGENT_MAX_ITERATIONS_CONFIG_KEY)
            .count(),
        1
    );
    assert!(!keys
        .iter()
        .any(|key| LEGACY_AGENT_MAX_ITERATIONS_CONFIG_KEYS.contains(&key.as_str())));
}

#[test]
fn task_runner_iteration_inherits_shared_agent_limit_when_missing() {
    let mut values = BTreeMap::from([(
        chatos_agent::AGENT_MAX_ITERATIONS_CONFIG_KEY.to_string(),
        json!(600),
    )]);

    assert!(ensure_task_runner_iteration_value(&mut values, json!(500)));
    assert_eq!(
        values.get(TASK_RUNNER_MAX_ITERATIONS_CONFIG_KEY),
        Some(&json!(600))
    );
}

#[test]
fn task_runner_iteration_keeps_explicit_service_limit() {
    let mut values = BTreeMap::from([
        (
            chatos_agent::AGENT_MAX_ITERATIONS_CONFIG_KEY.to_string(),
            json!(600),
        ),
        (
            TASK_RUNNER_MAX_ITERATIONS_CONFIG_KEY.to_string(),
            json!(900),
        ),
    ]);

    assert!(!ensure_task_runner_iteration_value(&mut values, json!(500)));
    assert_eq!(
        values.get(TASK_RUNNER_MAX_ITERATIONS_CONFIG_KEY),
        Some(&json!(900))
    );
}

#[test]
fn task_runner_execution_environment_mode_is_backfilled_when_missing() {
    let mut values = BTreeMap::new();

    assert!(ensure_task_runner_execution_environment_mode_value(
        &mut values,
        json!("cloud")
    ));
    assert_eq!(
        values.get(TASK_RUNNER_EXECUTION_ENVIRONMENT_MODE_CONFIG_KEY),
        Some(&json!("cloud"))
    );
}

#[test]
fn task_runner_execution_environment_mode_keeps_explicit_service_value() {
    let mut values = BTreeMap::from([(
        TASK_RUNNER_EXECUTION_ENVIRONMENT_MODE_CONFIG_KEY.to_string(),
        json!("local"),
    )]);

    assert!(!ensure_task_runner_execution_environment_mode_value(
        &mut values,
        json!("cloud")
    ));
    assert_eq!(
        values.get(TASK_RUNNER_EXECUTION_ENVIRONMENT_MODE_CONFIG_KEY),
        Some(&json!("local"))
    );
}

#[test]
fn task_runner_runtime_backfill_adds_all_service_defaults() {
    let definitions = builtin_definitions();
    let defaults = task_runner_service_default_values(&definitions);
    let mut values = BTreeMap::from([(
        chatos_agent::AGENT_MAX_ITERATIONS_CONFIG_KEY.to_string(),
        json!(600),
    )]);

    let changed_keys = ensure_task_runner_runtime_values(&mut values, &defaults);

    assert!(!changed_keys.is_empty());
    for key in defaults.keys() {
        assert!(
            values.contains_key(key),
            "missing Task Runner config key {key}"
        );
    }
    assert_eq!(
        values.get(TASK_RUNNER_MAX_ITERATIONS_CONFIG_KEY),
        Some(&json!(600))
    );
    assert!(changed_keys.contains(&TASK_RUNNER_EXECUTION_TIMEOUT_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&TASK_RUNNER_EXECUTION_ENVIRONMENT_MODE_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&TASK_RUNNER_QUEUE_RUN_DISPATCH_MODE_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&TASK_RUNNER_QUEUE_CALLBACK_DELIVERY_MODE_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&TASK_RUNNER_QUEUE_RABBITMQ_URL_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&TASK_RUNNER_QUEUE_RUN_DISPATCH_QUEUE_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&TASK_RUNNER_TOOL_RESULT_MAX_CHARS_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&TASK_RUNNER_TOOL_RESULTS_TOTAL_MAX_CHARS_CONFIG_KEY.to_string()));
}

#[test]
fn task_runner_snapshot_exposes_queue_environment_aliases() {
    let definitions = builtin_definitions();
    let values = BTreeMap::from([
        (
            TASK_RUNNER_QUEUE_RUN_DISPATCH_MODE_CONFIG_KEY.to_string(),
            json!("rabbitmq"),
        ),
        (
            TASK_RUNNER_QUEUE_CALLBACK_DELIVERY_MODE_CONFIG_KEY.to_string(),
            json!("rabbitmq"),
        ),
        (
            TASK_RUNNER_QUEUE_RABBITMQ_URL_CONFIG_KEY.to_string(),
            json!("amqp://chatos:change_me_rabbitmq_password@127.0.0.1:5672/"),
        ),
        (
            TASK_RUNNER_QUEUE_RABBITMQ_EXCHANGE_CONFIG_KEY.to_string(),
            json!("task_runner"),
        ),
        (
            TASK_RUNNER_QUEUE_RUN_DISPATCH_QUEUE_CONFIG_KEY.to_string(),
            json!("task_runner.run.dispatch"),
        ),
        (
            TASK_RUNNER_QUEUE_CALLBACK_DELIVERY_QUEUE_CONFIG_KEY.to_string(),
            json!("task_runner.callback.delivery"),
        ),
        (
            TASK_RUNNER_QUEUE_RUN_EVENTS_QUEUE_CONFIG_KEY.to_string(),
            json!("task_runner.run.events"),
        ),
    ]);

    let snapshot = build_snapshot("local", "task-runner", 1, &definitions, &values)
        .expect("Task Runner snapshot");

    assert_eq!(
        snapshot.env.get("TASK_RUNNER_RUN_DISPATCH_MODE"),
        Some(&"rabbitmq".to_string())
    );
    assert_eq!(
        snapshot.env.get("TASK_RUNNER_CALLBACK_DELIVERY_MODE"),
        Some(&"rabbitmq".to_string())
    );
    assert_eq!(
        snapshot.env.get("TASK_RUNNER_RABBITMQ_URL"),
        Some(&"amqp://chatos:change_me_rabbitmq_password@127.0.0.1:5672/".to_string())
    );
    assert_eq!(
        snapshot.env.get("TASK_RUNNER_RUN_EVENTS_QUEUE"),
        Some(&"task_runner.run.events".to_string())
    );
}

#[test]
fn mcp_management_async_dispatch_backfill_adds_all_service_defaults() {
    let definitions = builtin_definitions();
    let defaults = mcp_management_service_default_values(&definitions);
    let mut values = BTreeMap::new();

    let changed_keys = ensure_mcp_management_runtime_values(&mut values, &defaults);

    assert_eq!(defaults.len(), 16);
    for key in defaults.keys() {
        assert!(
            values.contains_key(key),
            "missing MCP Management config key {key}"
        );
    }
    assert!(changed_keys.contains(&MCP_MANAGEMENT_ASYNC_TOOL_DISPATCH_MODE_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&MCP_MANAGEMENT_ASYNC_TOOL_RABBITMQ_URL_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&MCP_MANAGEMENT_ASYNC_TOOL_DISPATCH_QUEUE_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&MCP_MANAGEMENT_INTERNAL_API_SECRET_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&MCP_MANAGEMENT_ALLOWED_INTERNAL_CALLERS_CONFIG_KEY.to_string()));
    assert!(changed_keys
        .contains(&MCP_MANAGEMENT_TASK_RUNNER_INTERNAL_API_SECRET_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&MCP_MANAGEMENT_RUNTIME_GRANT_SECRET_CONFIG_KEY.to_string()));
    assert!(changed_keys
        .contains(&MCP_MANAGEMENT_RUNTIME_SESSION_ENCRYPTION_SECRET_CONFIG_KEY.to_string()));
}

#[test]
fn mcp_management_snapshot_exposes_async_dispatch_environment_aliases() {
    let definitions = builtin_definitions();
    let values = BTreeMap::from([
        (
            MCP_MANAGEMENT_ASYNC_TOOL_DISPATCH_MODE_CONFIG_KEY.to_string(),
            json!("rabbitmq"),
        ),
        (
            MCP_MANAGEMENT_ASYNC_TOOL_WORKER_CONCURRENCY_CONFIG_KEY.to_string(),
            json!(12),
        ),
        (
            MCP_MANAGEMENT_ASYNC_TOOL_LOCAL_QUEUE_BUFFER_CONFIG_KEY.to_string(),
            json!(256),
        ),
        (
            MCP_MANAGEMENT_ASYNC_TOOL_RABBITMQ_URL_CONFIG_KEY.to_string(),
            json!("amqp://chatos:change_me_rabbitmq_password@127.0.0.1:5672/"),
        ),
        (
            MCP_MANAGEMENT_ASYNC_TOOL_RABBITMQ_EXCHANGE_CONFIG_KEY.to_string(),
            json!("mcp_management"),
        ),
        (
            MCP_MANAGEMENT_ASYNC_TOOL_DISPATCH_QUEUE_CONFIG_KEY.to_string(),
            json!("mcp_management.async.dispatch"),
        ),
        (
            MCP_MANAGEMENT_INTERNAL_API_SECRET_CONFIG_KEY.to_string(),
            json!("change_me_mcp_management_internal_secret"),
        ),
        (
            MCP_MANAGEMENT_ALLOWED_INTERNAL_CALLERS_CONFIG_KEY.to_string(),
            json!(
                "chatos,task-runner,project-service,memory-engine,local-connector-service,sandbox-manager,plugin-management-service"
            ),
        ),
        (
            MCP_MANAGEMENT_PLUGIN_MANAGEMENT_INTERNAL_API_SECRET_CONFIG_KEY.to_string(),
            json!("change_me_plugin_management_mcp_management_secret"),
        ),
        (
            MCP_MANAGEMENT_PROJECT_SERVICE_INTERNAL_API_SECRET_CONFIG_KEY.to_string(),
            json!("change_me_mcp_management_project_service_secret"),
        ),
        (
            MCP_MANAGEMENT_TASK_RUNNER_INTERNAL_API_SECRET_CONFIG_KEY.to_string(),
            json!("change_me_mcp_management_task_runner_secret"),
        ),
        (
            MCP_MANAGEMENT_CHATOS_INTERNAL_API_SECRET_CONFIG_KEY.to_string(),
            json!("change_me_mcp_management_chatos_secret"),
        ),
        (
            MCP_MANAGEMENT_LOCAL_CONNECTOR_INTERNAL_API_SECRET_CONFIG_KEY.to_string(),
            json!("change_me_mcp_management_local_connector_secret"),
        ),
        (
            MCP_MANAGEMENT_SANDBOX_MANAGER_INTERNAL_API_SECRET_CONFIG_KEY.to_string(),
            json!("change_me_mcp_management_sandbox_manager_secret"),
        ),
        (
            MCP_MANAGEMENT_RUNTIME_GRANT_SECRET_CONFIG_KEY.to_string(),
            json!("change_me_mcp_management_runtime_grant_secret"),
        ),
        (
            MCP_MANAGEMENT_RUNTIME_SESSION_ENCRYPTION_SECRET_CONFIG_KEY.to_string(),
            json!("change_me_mcp_management_runtime_session_encryption_secret"),
        ),
    ]);

    let snapshot = build_snapshot("local", "mcp-management-service", 1, &definitions, &values)
        .expect("MCP Management snapshot");

    assert_eq!(
        snapshot.env.get("MCP_MANAGEMENT_ASYNC_TOOL_DISPATCH_MODE"),
        Some(&"rabbitmq".to_string())
    );
    assert_eq!(
        snapshot.env.get("MCP_MANAGEMENT_ASYNC_TOOL_RABBITMQ_URL"),
        Some(&"amqp://chatos:change_me_rabbitmq_password@127.0.0.1:5672/".to_string())
    );
    assert_eq!(
        snapshot.env.get("MCP_MANAGEMENT_ASYNC_TOOL_DISPATCH_QUEUE"),
        Some(&"mcp_management.async.dispatch".to_string())
    );
    assert_eq!(
        snapshot.env.get("MCP_MANAGEMENT_INTERNAL_API_SECRET"),
        Some(&"change_me_mcp_management_internal_secret".to_string())
    );
    assert_eq!(
        snapshot.env.get("MCP_MANAGEMENT_ALLOWED_INTERNAL_CALLERS"),
        Some(
            &"chatos,task-runner,project-service,memory-engine,local-connector-service,sandbox-manager,plugin-management-service"
                .to_string()
        )
    );
    assert_eq!(
        snapshot
            .env
            .get("PLUGIN_MANAGEMENT_MCP_MANAGEMENT_INTERNAL_API_SECRET"),
        Some(&"change_me_plugin_management_mcp_management_secret".to_string())
    );
    assert_eq!(
        snapshot
            .env
            .get("MCP_MANAGEMENT_SANDBOX_MANAGER_INTERNAL_API_SECRET"),
        Some(&"change_me_mcp_management_sandbox_manager_secret".to_string())
    );
    assert_eq!(
        snapshot.env.get("MCP_MANAGEMENT_RUNTIME_GRANT_SECRET"),
        Some(&"change_me_mcp_management_runtime_grant_secret".to_string())
    );
    assert_eq!(
        snapshot
            .env
            .get("MCP_MANAGEMENT_RUNTIME_SESSION_ENCRYPTION_SECRET"),
        Some(&"change_me_mcp_management_runtime_session_encryption_secret".to_string())
    );
}

#[test]
fn sandbox_manager_pool_backfill_adds_both_limits() {
    let definitions = builtin_definitions();
    let defaults = sandbox_manager_pool_default_values(&definitions);
    let mut values = BTreeMap::new();

    let changed_keys = ensure_sandbox_manager_pool_values(&mut values, &defaults);

    assert_eq!(defaults.len(), 2);
    assert_eq!(
        values.get(SANDBOX_MANAGER_POOL_MAX_ACTIVE_CONFIG_KEY),
        Some(&json!(5))
    );
    assert_eq!(
        values.get(SANDBOX_MANAGER_POOL_MAX_PENDING_CONFIG_KEY),
        Some(&json!(50))
    );
    assert!(changed_keys.contains(&SANDBOX_MANAGER_POOL_MAX_ACTIVE_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&SANDBOX_MANAGER_POOL_MAX_PENDING_CONFIG_KEY.to_string()));
}

#[test]
fn sandbox_manager_snapshot_exposes_pool_environment_aliases() {
    let definitions = builtin_definitions();
    let values = BTreeMap::from([
        (
            SANDBOX_MANAGER_POOL_MAX_ACTIVE_CONFIG_KEY.to_string(),
            json!(8),
        ),
        (
            SANDBOX_MANAGER_POOL_MAX_PENDING_CONFIG_KEY.to_string(),
            json!(80),
        ),
    ]);

    let snapshot = build_snapshot("local", "sandbox-manager", 1, &definitions, &values)
        .expect("Sandbox Manager snapshot");

    assert_eq!(
        snapshot.env.get("SANDBOX_MANAGER_POOL_MAX_ACTIVE"),
        Some(&"8".to_string())
    );
    assert_eq!(
        snapshot.env.get("SANDBOX_MANAGER_POOL_MAX_PENDING"),
        Some(&"80".to_string())
    );
}

#[test]
fn internal_request_security_backfill_adds_all_service_defaults() {
    let definitions = builtin_definitions();
    let defaults = internal_request_security_default_values(&definitions);
    let mut values = BTreeMap::new();

    let changed_keys = ensure_internal_request_security_values(&mut values, &defaults);

    assert_eq!(defaults.len(), 31);
    for key in defaults.keys() {
        assert!(
            values.contains_key(key),
            "missing internal request security config key {key}"
        );
    }
    assert!(
        changed_keys.contains(&LOCAL_CONNECTOR_CHATOS_INTERNAL_API_SECRET_CONFIG_KEY.to_string())
    );
    assert!(changed_keys
        .contains(&LOCAL_CONNECTOR_TASK_RUNNER_INTERNAL_API_SECRET_CONFIG_KEY.to_string()));
    assert!(changed_keys
        .contains(&LOCAL_CONNECTOR_PROJECT_SERVICE_INTERNAL_API_SECRET_CONFIG_KEY.to_string()));
    assert!(changed_keys
        .contains(&LOCAL_CONNECTOR_MCP_MANAGEMENT_INTERNAL_API_SECRET_CONFIG_KEY.to_string()));
    assert!(
        changed_keys.contains(&PROJECT_SERVICE_CHATOS_INTERNAL_API_SECRET_CONFIG_KEY.to_string())
    );
    assert!(
        changed_keys.contains(&PROJECT_SERVICE_USER_SERVICE_INTERNAL_SECRET_CONFIG_KEY.to_string())
    );
    assert!(
        changed_keys.contains(&PROJECT_SERVICE_MEMORY_ENGINE_OPERATOR_TOKEN_CONFIG_KEY.to_string())
    );
    assert!(changed_keys.contains(&MEMORY_ENGINE_OPERATOR_TOKEN_CONFIG_KEY.to_string()));
    assert!(changed_keys
        .contains(&MEMORY_ENGINE_TASK_RUNNER_INTERNAL_API_SECRET_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&SANDBOX_MANAGER_OPERATOR_TOKEN_CONFIG_KEY.to_string()));
    assert!(changed_keys
        .contains(&SANDBOX_MANAGER_PROJECT_SERVICE_INTERNAL_API_SECRET_CONFIG_KEY.to_string()));
    assert!(changed_keys
        .contains(&LOCAL_CONNECTOR_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY.to_string()));
    assert!(changed_keys
        .contains(&MCP_MANAGEMENT_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY.to_string()));
    assert!(changed_keys
        .contains(&PROJECT_SERVICE_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY.to_string()));
    assert!(changed_keys
        .contains(&MEMORY_ENGINE_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY.to_string()));
    assert!(changed_keys
        .contains(&SANDBOX_MANAGER_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY.to_string()));
}

#[test]
fn internal_request_security_snapshot_exposes_environment_aliases() {
    let definitions = builtin_definitions();
    let values = BTreeMap::from([
        (
            LOCAL_CONNECTOR_CHATOS_INTERNAL_API_SECRET_CONFIG_KEY.to_string(),
            json!("change_me_chatos_local_connector_secret"),
        ),
        (
            LOCAL_CONNECTOR_TASK_RUNNER_INTERNAL_API_SECRET_CONFIG_KEY.to_string(),
            json!("change_me_task_runner_local_connector_secret"),
        ),
        (
            LOCAL_CONNECTOR_PROJECT_SERVICE_INTERNAL_API_SECRET_CONFIG_KEY.to_string(),
            json!("change_me_project_service_local_connector_secret"),
        ),
        (
            LOCAL_CONNECTOR_MCP_MANAGEMENT_INTERNAL_API_SECRET_CONFIG_KEY.to_string(),
            json!("change_me_mcp_management_local_connector_secret"),
        ),
        (
            PROJECT_SERVICE_CHATOS_INTERNAL_API_SECRET_CONFIG_KEY.to_string(),
            json!("change_me_chatos_project_service_secret"),
        ),
        (
            PROJECT_SERVICE_TASK_RUNNER_INTERNAL_API_SECRET_CONFIG_KEY.to_string(),
            json!("change_me_task_runner_project_service_secret"),
        ),
        (
            PROJECT_SERVICE_SELF_INTERNAL_API_SECRET_CONFIG_KEY.to_string(),
            json!("change_me_project_service_self_secret"),
        ),
        (
            PROJECT_SERVICE_MCP_MANAGEMENT_INTERNAL_API_SECRET_CONFIG_KEY.to_string(),
            json!("change_me_mcp_management_project_service_secret"),
        ),
        (
            PROJECT_SERVICE_USER_SERVICE_INTERNAL_SECRET_CONFIG_KEY.to_string(),
            json!("change_me_project_service_user_service_secret"),
        ),
        (
            PROJECT_SERVICE_TASK_RUNNER_INTERNAL_SECRET_CONFIG_KEY.to_string(),
            json!("change_me_project_service_task_runner_secret"),
        ),
        (
            PROJECT_SERVICE_SYNC_SECRET_CONFIG_KEY.to_string(),
            json!("change_me_project_sync_secret"),
        ),
        (
            PROJECT_SERVICE_MEMORY_ENGINE_OPERATOR_TOKEN_CONFIG_KEY.to_string(),
            json!("chatos-memory-engine-dev-operator-token"),
        ),
        (
            PROJECT_SERVICE_SANDBOX_MANAGER_CLIENT_ID_CONFIG_KEY.to_string(),
            json!("project-service"),
        ),
        (
            PROJECT_SERVICE_SANDBOX_MANAGER_CLIENT_KEY_CONFIG_KEY.to_string(),
            json!("chatos-task-runner-sandbox-dev-key"),
        ),
        (
            MEMORY_ENGINE_CHATOS_INTERNAL_API_SECRET_CONFIG_KEY.to_string(),
            json!("change_me_chatos_memory_engine_secret"),
        ),
        (
            MEMORY_ENGINE_TASK_RUNNER_INTERNAL_API_SECRET_CONFIG_KEY.to_string(),
            json!("change_me_task_runner_memory_engine_secret"),
        ),
        (
            MEMORY_ENGINE_PROJECT_SERVICE_INTERNAL_API_SECRET_CONFIG_KEY.to_string(),
            json!("change_me_project_service_memory_engine_secret"),
        ),
        (
            MEMORY_ENGINE_USER_SERVICE_INTERNAL_API_SECRET_CONFIG_KEY.to_string(),
            json!("change_me_user_service_memory_engine_secret"),
        ),
        (
            MEMORY_ENGINE_OPERATOR_TOKEN_CONFIG_KEY.to_string(),
            json!("chatos-memory-engine-dev-operator-token"),
        ),
        (
            SANDBOX_MANAGER_TASK_RUNNER_INTERNAL_API_SECRET_CONFIG_KEY.to_string(),
            json!("change_me_task_runner_sandbox_manager_secret"),
        ),
        (
            SANDBOX_MANAGER_PROJECT_SERVICE_INTERNAL_API_SECRET_CONFIG_KEY.to_string(),
            json!("change_me_project_service_sandbox_manager_secret"),
        ),
        (
            SANDBOX_MANAGER_MCP_MANAGEMENT_INTERNAL_API_SECRET_CONFIG_KEY.to_string(),
            json!("change_me_mcp_management_sandbox_manager_secret"),
        ),
        (
            SANDBOX_MANAGER_OPERATOR_TOKEN_CONFIG_KEY.to_string(),
            json!("chatos-sandbox-manager-dev-operator-token"),
        ),
        (
            SANDBOX_MANAGER_SYSTEM_CLIENT_ID_CONFIG_KEY.to_string(),
            json!("task_runner"),
        ),
        (
            SANDBOX_MANAGER_SYSTEM_CLIENT_KEY_CONFIG_KEY.to_string(),
            json!("chatos-task-runner-sandbox-dev-key"),
        ),
        (
            SANDBOX_MANAGER_AGENT_TOKEN_SECRET_CONFIG_KEY.to_string(),
            json!("chatos-sandbox-agent-dev-secret"),
        ),
        (
            LOCAL_CONNECTOR_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY.to_string(),
            json!(true),
        ),
        (
            MCP_MANAGEMENT_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY.to_string(),
            json!(true),
        ),
        (
            PROJECT_SERVICE_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY.to_string(),
            json!(true),
        ),
        (
            MEMORY_ENGINE_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY.to_string(),
            json!(true),
        ),
        (
            SANDBOX_MANAGER_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY.to_string(),
            json!(true),
        ),
    ]);

    let local_connector_snapshot =
        build_snapshot("local", "local-connector-service", 1, &definitions, &values)
            .expect("Local Connector snapshot");
    assert_eq!(
        local_connector_snapshot
            .env
            .get("LOCAL_CONNECTOR_REQUIRE_SIGNED_INTERNAL_REQUESTS"),
        Some(&"true".to_string())
    );
    assert_eq!(
        local_connector_snapshot
            .env
            .get("CHATOS_LOCAL_CONNECTOR_INTERNAL_API_SECRET"),
        Some(&"change_me_chatos_local_connector_secret".to_string())
    );
    assert_eq!(
        local_connector_snapshot
            .env
            .get("MCP_MANAGEMENT_LOCAL_CONNECTOR_INTERNAL_API_SECRET"),
        Some(&"change_me_mcp_management_local_connector_secret".to_string())
    );

    let mcp_management_snapshot =
        build_snapshot("local", "mcp-management-service", 1, &definitions, &values)
            .expect("MCP Management snapshot");
    assert_eq!(
        mcp_management_snapshot
            .env
            .get("MCP_MANAGEMENT_REQUIRE_SIGNED_INTERNAL_REQUESTS"),
        Some(&"true".to_string())
    );

    let project_snapshot = build_snapshot("local", "project-service", 1, &definitions, &values)
        .expect("Project Service snapshot");
    assert_eq!(
        project_snapshot
            .env
            .get("PROJECT_SERVICE_REQUIRE_SIGNED_INTERNAL_REQUESTS"),
        Some(&"true".to_string())
    );
    assert_eq!(
        project_snapshot
            .env
            .get("CHATOS_PROJECT_SERVICE_INTERNAL_API_SECRET"),
        Some(&"change_me_chatos_project_service_secret".to_string())
    );
    assert_eq!(
        project_snapshot
            .env
            .get("PROJECT_SERVICE_MEMORY_ENGINE_OPERATOR_TOKEN"),
        Some(&"chatos-memory-engine-dev-operator-token".to_string())
    );
    assert_eq!(
        project_snapshot
            .env
            .get("PROJECT_SERVICE_SANDBOX_MANAGER_CLIENT_KEY"),
        Some(&"chatos-task-runner-sandbox-dev-key".to_string())
    );

    let memory_snapshot = build_snapshot("local", "memory-engine", 1, &definitions, &values)
        .expect("Memory Engine snapshot");
    assert_eq!(
        memory_snapshot
            .env
            .get("MEMORY_ENGINE_REQUIRE_SIGNED_INTERNAL_REQUESTS"),
        Some(&"true".to_string())
    );
    assert_eq!(
        memory_snapshot
            .env
            .get("TASK_RUNNER_MEMORY_ENGINE_INTERNAL_API_SECRET"),
        Some(&"change_me_task_runner_memory_engine_secret".to_string())
    );
    assert_eq!(
        memory_snapshot.env.get("MEMORY_ENGINE_OPERATOR_TOKEN"),
        Some(&"chatos-memory-engine-dev-operator-token".to_string())
    );

    let sandbox_snapshot = build_snapshot("local", "sandbox-manager", 1, &definitions, &values)
        .expect("Sandbox Manager snapshot");
    assert_eq!(
        sandbox_snapshot
            .env
            .get("SANDBOX_MANAGER_REQUIRE_SIGNED_INTERNAL_REQUESTS"),
        Some(&"true".to_string())
    );
    assert_eq!(
        sandbox_snapshot
            .env
            .get("PROJECT_SERVICE_SANDBOX_MANAGER_INTERNAL_API_SECRET"),
        Some(&"change_me_project_service_sandbox_manager_secret".to_string())
    );
    assert_eq!(
        sandbox_snapshot.env.get("SANDBOX_MANAGER_OPERATOR_TOKEN"),
        Some(&"chatos-sandbox-manager-dev-operator-token".to_string())
    );
    assert_eq!(
        sandbox_snapshot
            .env
            .get("SANDBOX_MANAGER_AGENT_TOKEN_SECRET"),
        Some(&"chatos-sandbox-agent-dev-secret".to_string())
    );
}

#[test]
fn user_service_smtp_backfill_adds_all_service_defaults() {
    let definitions = builtin_definitions();
    let defaults = user_service_smtp_default_values(&definitions);
    let mut values = BTreeMap::new();

    let changed_keys = ensure_user_service_smtp_values(&mut values, &defaults);

    assert_eq!(defaults.len(), 5);
    for key in defaults.keys() {
        assert!(
            values.contains_key(key),
            "missing User Service SMTP config key {key}"
        );
        assert_eq!(values.get(key), Some(&Value::Null));
    }
    assert!(changed_keys.contains(&USER_SERVICE_SMTP_HOST_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&USER_SERVICE_SMTP_PORT_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&USER_SERVICE_SMTP_USERNAME_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&USER_SERVICE_EMAIL_FROM_CONFIG_KEY.to_string()));
    assert!(changed_keys.contains(&USER_SERVICE_EMAIL_FROM_NAME_CONFIG_KEY.to_string()));
}

#[test]
fn user_service_smtp_snapshot_skips_null_env_aliases_until_configured() {
    let definitions = builtin_definitions();
    let values = BTreeMap::from([
        (USER_SERVICE_SMTP_HOST_CONFIG_KEY.to_string(), Value::Null),
        (USER_SERVICE_SMTP_PORT_CONFIG_KEY.to_string(), Value::Null),
        (
            USER_SERVICE_SMTP_USERNAME_CONFIG_KEY.to_string(),
            Value::Null,
        ),
        (USER_SERVICE_EMAIL_FROM_CONFIG_KEY.to_string(), Value::Null),
        (
            USER_SERVICE_EMAIL_FROM_NAME_CONFIG_KEY.to_string(),
            Value::Null,
        ),
    ]);

    let snapshot = build_snapshot("local", "user-service", 1, &definitions, &values)
        .expect("User Service snapshot");

    assert!(!snapshot.env.contains_key("USER_SERVICE_SMTP_HOST"));
    assert!(!snapshot.env.contains_key("USER_SERVICE_SMTP_PORT"));
    assert!(!snapshot.env.contains_key("USER_SERVICE_SMTP_USERNAME"));
    assert!(!snapshot.env.contains_key("USER_SERVICE_EMAIL_FROM"));
    assert!(!snapshot.env.contains_key("USER_SERVICE_EMAIL_FROM_NAME"));
}

#[test]
fn user_service_smtp_snapshot_exposes_environment_aliases_when_values_are_present() {
    let definitions = builtin_definitions();
    let values = BTreeMap::from([
        (
            USER_SERVICE_SMTP_HOST_CONFIG_KEY.to_string(),
            json!("smtp.example.com"),
        ),
        (USER_SERVICE_SMTP_PORT_CONFIG_KEY.to_string(), json!(465)),
        (
            USER_SERVICE_SMTP_USERNAME_CONFIG_KEY.to_string(),
            json!("mailer@example.com"),
        ),
        (
            USER_SERVICE_EMAIL_FROM_CONFIG_KEY.to_string(),
            json!("mailer@example.com"),
        ),
        (
            USER_SERVICE_EMAIL_FROM_NAME_CONFIG_KEY.to_string(),
            json!("Chat OS Mailer"),
        ),
    ]);

    let snapshot = build_snapshot("local", "user-service", 1, &definitions, &values)
        .expect("User Service snapshot");

    assert_eq!(
        snapshot.env.get("USER_SERVICE_SMTP_HOST"),
        Some(&"smtp.example.com".to_string())
    );
    assert_eq!(
        snapshot.env.get("USER_SERVICE_SMTP_PORT"),
        Some(&"465".to_string())
    );
    assert_eq!(
        snapshot.env.get("USER_SERVICE_SMTP_USERNAME"),
        Some(&"mailer@example.com".to_string())
    );
    assert_eq!(
        snapshot.env.get("USER_SERVICE_EMAIL_FROM"),
        Some(&"mailer@example.com".to_string())
    );
    assert_eq!(
        snapshot.env.get("USER_SERVICE_EMAIL_FROM_NAME"),
        Some(&"Chat OS Mailer".to_string())
    );
}

#[test]
fn chatos_local_project_creation_backfill_uses_catalog_default() {
    let definitions = builtin_definitions();
    let default_value = chatos_local_project_creation_default_value(&definitions)
        .expect("local project creation default");
    let mut values = BTreeMap::new();

    assert_eq!(default_value, json!(false));
    assert!(ensure_chatos_local_project_creation_value(
        &mut values,
        default_value
    ));
    assert_eq!(
        values.get(CHATOS_LOCAL_PROJECT_CREATION_CONFIG_KEY),
        Some(&json!(false))
    );
}

#[test]
fn chatos_local_project_creation_backfill_keeps_explicit_value() {
    let mut values = BTreeMap::from([(
        CHATOS_LOCAL_PROJECT_CREATION_CONFIG_KEY.to_string(),
        json!(true),
    )]);

    assert!(!ensure_chatos_local_project_creation_value(
        &mut values,
        json!(false)
    ));
    assert_eq!(
        values.get(CHATOS_LOCAL_PROJECT_CREATION_CONFIG_KEY),
        Some(&json!(true))
    );
}

#[test]
fn chatos_snapshot_exposes_local_project_creation_value_and_legacy_env() {
    let definitions = builtin_definitions();
    for enabled in [false, true] {
        let values = BTreeMap::from([(
            CHATOS_LOCAL_PROJECT_CREATION_CONFIG_KEY.to_string(),
            json!(enabled),
        )]);
        let snapshot = build_snapshot("local", "chatos-backend", 1, &definitions, &values)
            .expect("ChatOS snapshot");

        assert_eq!(
            snapshot
                .values
                .get(CHATOS_LOCAL_PROJECT_CREATION_CONFIG_KEY),
            Some(&json!(enabled))
        );
        assert_eq!(
            snapshot.env.get("LOCAL_PROJECT_CREATION_ENABLED"),
            Some(&enabled.to_string())
        );
    }
}

#[test]
fn non_chatos_snapshot_does_not_gain_local_project_creation_value() {
    let definitions = builtin_definitions();
    let values = BTreeMap::from([(
        CHATOS_LOCAL_PROJECT_CREATION_CONFIG_KEY.to_string(),
        json!(true),
    )]);
    let snapshot = build_snapshot("local", "task-runner", 1, &definitions, &values)
        .expect("Task Runner snapshot");

    assert!(!snapshot
        .values
        .contains_key(CHATOS_LOCAL_PROJECT_CREATION_CONFIG_KEY));
    assert!(!snapshot.env.contains_key("LOCAL_PROJECT_CREATION_ENABLED"));
}
