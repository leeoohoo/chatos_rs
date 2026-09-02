pub const USER_PREFERENCE_CONFIG_KEYS: &[&str] =
    &["shared.ui.locale", "shared.ai.internal_context_locale"];
pub const LEGACY_AGENT_MAX_ITERATIONS_CONFIG_KEYS: &[&str] = &[
    "chatos.ai.max_iterations",
    "task_runner.execution.max_iterations",
];
pub const RETIRED_CONFIG_KEYS: &[&str] = &[
    "chatos.ui.local_project_creation_enabled",
    "local_connector.relay.sandbox_image_request_timeout_ms",
    "local_connector.security.task_runner_internal_api_secret",
    "mcp_management.async_tool.local_queue_buffer",
    "mcp_management.async_tool.result_outbox_batch_size",
    "mcp_management.async_tool.result_outbox_reconcile_ms",
    "mcp_management.downstream.sandbox_manager_internal_api_secret",
    "mcp_management.downstream.sandbox_manager_service_base_url",
    "mcp_management.runtime.embedded_work_dir",
    "mcp_management.runtime.sandbox_image_tool_timeout_ms",
    "mcp_management.runtime.sandbox_tool_timeout_ms",
    "memory_engine.security.project_service_internal_api_secret",
    "memory_engine.ai.openai_api_key",
    "memory_engine.ai.openai_base_url",
    "memory_engine.ai.openai_model",
    "memory_engine.ai.openai_temperature",
    "plugin_management.oauth.flow_ttl_seconds",
    "plugin_management.oauth.frontend_origin",
    "plugin_management.oauth.max_response_bytes",
    "plugin_management.oauth.public_base_url",
    "plugin_management.oauth.refresh_skew_seconds",
    "plugin_management.oauth.request_timeout_ms",
    "plugin_management.security.cloud_credential_encryption_secret",
    "project_service.downstream.memory_engine_base_url",
    "project_service.downstream.memory_engine_internal_api_secret",
    "project_service.downstream.memory_engine_request_timeout_ms",
    "project_service.downstream.sandbox_image_mcp_request_timeout_ms",
    "project_service.downstream.sandbox_manager_base_url",
    "project_service.downstream.sandbox_manager_client_id",
    "project_service.downstream.sandbox_manager_client_key",
    "project_service.environment_analysis.stale_after_ms",
    "project_service.environment_analysis.timeout_ms",
    "project_service.mcp.result_queue_prefix",
    "project_service.mcp.result_rabbitmq_url",
    "sandbox_manager.docker.build_cache_max_used_space",
    "sandbox_manager.docker.build_cache_reserved_space",
    "sandbox_manager.docker.build_cache_timeout_secs",
    "sandbox_manager.docker.maintenance_enabled",
    "sandbox_manager.downstream.user_service_base_url",
    "sandbox_manager.downstream.user_service_request_timeout_ms",
    "sandbox_manager.frontend.proxy_client_id",
    "sandbox_manager.frontend.proxy_client_key",
    "sandbox_manager.pool.cleanup_interval_seconds",
    "sandbox_manager.pool.lease_ttl_seconds",
    "sandbox_manager.pool.max_active",
    "sandbox_manager.pool.max_pending",
    "sandbox_manager.runtime.agent_port",
    "sandbox_manager.runtime.database_url",
    "sandbox_manager.runtime.host",
    "sandbox_manager.runtime.internal_mtls_port",
    "sandbox_manager.runtime.mongodb_database",
    "sandbox_manager.runtime.port",
    "sandbox_manager.security.agent_token_secret",
    "sandbox_manager.security.mcp_management_internal_api_secret",
    "sandbox_manager.security.project_service_internal_api_secret",
    "sandbox_manager.security.require_auth",
    "sandbox_manager.security.require_signed_internal_requests",
    "sandbox_manager.security.system_client_max_lease_ttl_seconds",
    "sandbox_manager.security.task_runner_internal_api_secret",
    "task_runner.cache.plugin_cloud_bundle_max_bytes",
    "task_runner.cache.plugin_cloud_bundle_max_entries",
    "task_runner.downstream.local_connector_internal_api_secret",
    "task_runner.downstream.local_connector_service_base_url",
    "task_runner.downstream.local_connector_service_request_timeout_ms",
    "task_runner.downstream.plugin_connector_discovery_timeout_ms",
    "task_runner.downstream.plugin_hook_relay_timeout_ms",
    "task_runner.downstream.plugin_relay_timeout_ms",
    "task_runner.downstream.sandbox_manager_base_url",
    "task_runner.downstream.sandbox_manager_internal_api_secret",
    "task_runner.execution.environment_mode",
    "task_runner.queue.run_dispatch_mode",
    "task_runner.queue.run_dispatch_outbox_batch_size",
    "task_runner.queue.run_dispatch_outbox_reconcile_ms",
    "task_runner.queue.run_dispatch_queue",
    "task_runner.queue.run_dispatch_retry_delay_ms",
    "task_runner.queue.run_dispatch_retry_queue",
    "task_runner.sandbox.enabled",
    "task_runner.sandbox.lease_ttl_seconds",
    "task_runner.sandbox.manager_base_url",
    "user_service.downstream.memory_engine_base_url",
];
pub const PLATFORM_PRESSURE_LEVEL_CONFIG_KEY: &str = "platform.pressure.level";
pub const PLATFORM_PRESSURE_CONTROLLER_ENABLED_CONFIG_KEY: &str =
    "platform.pressure.controller.enabled";
pub const PLATFORM_PRESSURE_CONTROLLER_INTERVAL_MS_CONFIG_KEY: &str =
    "platform.pressure.controller.interval_ms";
pub const PLATFORM_PRESSURE_SIGNAL_TTL_SECONDS_CONFIG_KEY: &str =
    "platform.pressure.controller.signal_ttl_seconds";
pub const PLATFORM_PRESSURE_ESCALATION_STABLE_SECONDS_CONFIG_KEY: &str =
    "platform.pressure.controller.escalation_stable_seconds";
pub const PLATFORM_PRESSURE_RECOVERY_STABLE_SECONDS_CONFIG_KEY: &str =
    "platform.pressure.controller.recovery_stable_seconds";
pub const DEFAULT_RABBITMQ_ROOT_VHOST_URI_SEGMENT: &str = "%2f";
pub const DEFAULT_LOCAL_RABBITMQ_URL: &str =
    "amqp://chatos:change_me_rabbitmq_password@127.0.0.1:5672/%2f";
pub const SHARED_PLUGIN_MANAGEMENT_SERVICE_URL_CONFIG_KEY: &str =
    "shared.downstream.plugin_management_service_url";
pub const SHARED_PLUGIN_MANAGEMENT_SERVICE_INTERNAL_URL_CONFIG_KEY: &str =
    "shared.downstream.plugin_management_service_internal_url";
pub const SHARED_PLUGIN_MANAGEMENT_REQUEST_TIMEOUT_MS_CONFIG_KEY: &str =
    "shared.downstream.plugin_management_request_timeout_ms";
pub const SHARED_MCP_MANAGEMENT_SERVICE_BASE_URL_CONFIG_KEY: &str =
    "shared.downstream.mcp_management_service_base_url";
pub const SHARED_MCP_MANAGEMENT_REQUEST_TIMEOUT_MS_CONFIG_KEY: &str =
    "shared.downstream.mcp_management_request_timeout_ms";
pub const SHARED_MCP_MANAGEMENT_RUNTIME_SESSION_REQUEST_TIMEOUT_MS_CONFIG_KEY: &str =
    "shared.downstream.mcp_management_runtime_session_request_timeout_ms";
