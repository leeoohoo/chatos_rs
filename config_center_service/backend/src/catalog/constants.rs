pub const USER_PREFERENCE_CONFIG_KEYS: &[&str] =
    &["shared.ui.locale", "shared.ai.internal_context_locale"];
pub const LEGACY_AGENT_MAX_ITERATIONS_CONFIG_KEYS: &[&str] = &[
    "chatos.ai.max_iterations",
    "task_runner.execution.max_iterations",
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
pub const TASK_RUNNER_EXECUTION_TIMEOUT_CONFIG_KEY: &str = "task_runner.execution.timeout_ms";
pub const TASK_RUNNER_EXECUTION_ENVIRONMENT_MODE_CONFIG_KEY: &str =
    "task_runner.execution.environment_mode";
pub const TASK_RUNNER_SUPPLY_CHAIN_BASELINE_REVISION_CONFIG_KEY: &str =
    "task_runner.supply_chain.baseline_revision";
pub const TASK_RUNNER_SUPPLY_CHAIN_NODE_DEPENDENCY_REQUIREMENTS_CONFIG_KEY: &str =
    "task_runner.supply_chain.node_dependency_requirements";
pub const TASK_RUNNER_SUPPLY_CHAIN_NODE_AUDIT_LEVEL_CONFIG_KEY: &str =
    "task_runner.supply_chain.node_audit_level";
pub const TASK_RUNNER_SUPPLY_CHAIN_INSTALL_SCRIPT_ALLOWLIST_CONFIG_KEY: &str =
    "task_runner.supply_chain.install_script_allowlist";
pub const TASK_RUNNER_QUEUE_RUN_DISPATCH_MODE_CONFIG_KEY: &str =
    "task_runner.queue.run_dispatch_mode";
pub const TASK_RUNNER_QUEUE_CALLBACK_DELIVERY_MODE_CONFIG_KEY: &str =
    "task_runner.queue.callback_delivery_mode";
pub const TASK_RUNNER_QUEUE_RABBITMQ_URL_CONFIG_KEY: &str = "task_runner.queue.rabbitmq_url";
pub const TASK_RUNNER_QUEUE_RABBITMQ_EXCHANGE_CONFIG_KEY: &str =
    "task_runner.queue.rabbitmq_exchange";
pub const TASK_RUNNER_QUEUE_RABBITMQ_RECONNECT_MS_CONFIG_KEY: &str =
    "task_runner.queue.rabbitmq_reconnect_ms";
pub const TASK_RUNNER_QUEUE_RUN_DISPATCH_QUEUE_CONFIG_KEY: &str =
    "task_runner.queue.run_dispatch_queue";
pub const TASK_RUNNER_QUEUE_RUN_DISPATCH_RETRY_QUEUE_CONFIG_KEY: &str =
    "task_runner.queue.run_dispatch_retry_queue";
pub const TASK_RUNNER_QUEUE_RUN_DISPATCH_RETRY_DELAY_MS_CONFIG_KEY: &str =
    "task_runner.queue.run_dispatch_retry_delay_ms";
pub const TASK_RUNNER_QUEUE_RUN_DISPATCH_OUTBOX_RECONCILE_MS_CONFIG_KEY: &str =
    "task_runner.queue.run_dispatch_outbox_reconcile_ms";
pub const TASK_RUNNER_QUEUE_RUN_DISPATCH_OUTBOX_BATCH_SIZE_CONFIG_KEY: &str =
    "task_runner.queue.run_dispatch_outbox_batch_size";
pub const TASK_RUNNER_QUEUE_WORKER_CONTROL_QUEUE_PREFIX_CONFIG_KEY: &str =
    "task_runner.queue.worker_control_queue_prefix";
pub const TASK_RUNNER_QUEUE_RUN_POST_PROCESS_QUEUE_CONFIG_KEY: &str =
    "task_runner.queue.run_post_process_queue";
pub const TASK_RUNNER_QUEUE_RUN_POST_PROCESS_RETRY_QUEUE_CONFIG_KEY: &str =
    "task_runner.queue.run_post_process_retry_queue";
pub const TASK_RUNNER_QUEUE_RUN_POST_PROCESS_DEAD_LETTER_QUEUE_CONFIG_KEY: &str =
    "task_runner.queue.run_post_process_dead_letter_queue";
pub const TASK_RUNNER_QUEUE_RUN_POST_PROCESS_MAX_DELIVERY_ATTEMPTS_CONFIG_KEY: &str =
    "task_runner.queue.run_post_process_max_delivery_attempts";
pub const TASK_RUNNER_QUEUE_RUN_POST_PROCESS_RETRY_DELAY_MS_CONFIG_KEY: &str =
    "task_runner.queue.run_post_process_retry_delay_ms";
pub const TASK_RUNNER_QUEUE_RUN_POST_PROCESS_OUTBOX_RECONCILE_MS_CONFIG_KEY: &str =
    "task_runner.queue.run_post_process_outbox_reconcile_ms";
pub const TASK_RUNNER_QUEUE_RUN_POST_PROCESS_OUTBOX_BATCH_SIZE_CONFIG_KEY: &str =
    "task_runner.queue.run_post_process_outbox_batch_size";
pub const TASK_RUNNER_QUEUE_CALLBACK_DELIVERY_QUEUE_CONFIG_KEY: &str =
    "task_runner.queue.callback_delivery_queue";
pub const TASK_RUNNER_QUEUE_RUN_EVENTS_PUBLISH_MODE_CONFIG_KEY: &str =
    "task_runner.queue.run_events_publish_mode";
pub const TASK_RUNNER_QUEUE_RUN_EVENTS_ROUTING_KEY_CONFIG_KEY: &str =
    "task_runner.queue.run_events_routing_key";
pub const TASK_RUNNER_MCP_RESULT_QUEUE_PREFIX_CONFIG_KEY: &str =
    "task_runner.mcp.result_queue_prefix";
pub const TASK_RUNNER_PRESSURE_QUEUE_ELEVATED_MESSAGES_CONFIG_KEY: &str =
    "task_runner.pressure.queue_elevated_messages";
pub const TASK_RUNNER_PRESSURE_QUEUE_CRITICAL_MESSAGES_CONFIG_KEY: &str =
    "task_runner.pressure.queue_critical_messages";
pub const TASK_RUNNER_PRESSURE_REPORT_INTERVAL_MS_CONFIG_KEY: &str =
    "task_runner.pressure.report_interval_ms";
pub const TASK_RUNNER_TOOL_RESULT_MAX_CHARS_CONFIG_KEY: &str =
    "task_runner.ai.tool_result_max_chars";
pub const TASK_RUNNER_TOOL_RESULTS_TOTAL_MAX_CHARS_CONFIG_KEY: &str =
    "task_runner.ai.tool_results_total_max_chars";
pub const TASK_RUNNER_PLUGIN_CLOUD_BUNDLE_CACHE_MAX_ENTRIES_CONFIG_KEY: &str =
    "task_runner.cache.plugin_cloud_bundle_max_entries";
pub const TASK_RUNNER_PLUGIN_CLOUD_BUNDLE_CACHE_MAX_BYTES_CONFIG_KEY: &str =
    "task_runner.cache.plugin_cloud_bundle_max_bytes";
pub const TASK_RUNNER_MEMORY_TIMEOUT_MS_CONFIG_KEY: &str =
    "task_runner.downstream.memory_engine_request_timeout_ms";
pub const TASK_RUNNER_SCHEDULER_POLL_MS_CONFIG_KEY: &str = "task_runner.scheduler.poll_interval_ms";
pub const TASK_RUNNER_RUN_EVENT_RETENTION_DAYS_CONFIG_KEY: &str =
    "task_runner.retention.run_event_days";
pub const TASK_RUNNER_RUN_EVENT_CLEANUP_INTERVAL_MS_CONFIG_KEY: &str =
    "task_runner.retention.run_event_cleanup_interval_ms";
pub const TASK_RUNNER_RUN_EVENT_CLEANUP_BATCH_SIZE_CONFIG_KEY: &str =
    "task_runner.retention.run_event_cleanup_batch_size";
pub const TASK_RUNNER_TERMINAL_LOG_MAX_ENTRIES_CONFIG_KEY: &str =
    "task_runner.terminal.log_max_entries";
pub const TASK_RUNNER_TERMINAL_MAX_SESSIONS_CONFIG_KEY: &str = "task_runner.terminal.max_sessions";
pub const TASK_RUNNER_TERMINAL_EXITED_SESSION_RETENTION_SECONDS_CONFIG_KEY: &str =
    "task_runner.terminal.exited_session_retention_seconds";
pub const TASK_RUNNER_TERMINAL_CLEANUP_INTERVAL_MS_CONFIG_KEY: &str =
    "task_runner.terminal.cleanup_interval_ms";
pub const TASK_RUNNER_ASK_USER_PROMPT_RETENTION_DAYS_CONFIG_KEY: &str =
    "task_runner.retention.ask_user_prompt_days";
pub const TASK_RUNNER_ASK_USER_PROMPT_CLEANUP_INTERVAL_MS_CONFIG_KEY: &str =
    "task_runner.retention.ask_user_prompt_cleanup_interval_ms";
pub const TASK_RUNNER_ASK_USER_PROMPT_CLEANUP_BATCH_SIZE_CONFIG_KEY: &str =
    "task_runner.retention.ask_user_prompt_cleanup_batch_size";
pub const TASK_RUNNER_AUTO_MEMORY_SUMMARY_CONFIG_KEY: &str =
    "task_runner.memory.auto_summary_enabled";
pub const TASK_RUNNER_USER_SERVICE_BASE_URL_CONFIG_KEY: &str =
    "task_runner.downstream.user_service_base_url";
pub const TASK_RUNNER_USER_SERVICE_REQUEST_TIMEOUT_MS_CONFIG_KEY: &str =
    "task_runner.downstream.user_service_request_timeout_ms";
pub const TASK_RUNNER_PROJECT_SERVICE_BASE_URL_CONFIG_KEY: &str =
    "task_runner.downstream.project_service_base_url";
pub const TASK_RUNNER_PROJECT_SERVICE_INTERNAL_BASE_URL_CONFIG_KEY: &str =
    "task_runner.downstream.project_service_internal_base_url";
pub const TASK_RUNNER_PROJECT_SERVICE_REQUEST_TIMEOUT_MS_CONFIG_KEY: &str =
    "task_runner.downstream.project_service_request_timeout_ms";
pub const TASK_RUNNER_MEMORY_ENGINE_BASE_URL_CONFIG_KEY: &str =
    "task_runner.downstream.memory_engine_base_url";
pub const TASK_RUNNER_SANDBOX_MANAGER_BASE_URL_CONFIG_KEY: &str =
    "task_runner.downstream.sandbox_manager_base_url";
pub const TASK_RUNNER_PROJECT_SERVICE_INTERNAL_API_SECRET_CONFIG_KEY: &str =
    "task_runner.downstream.project_service_internal_api_secret";
pub const TASK_RUNNER_MEMORY_ENGINE_INTERNAL_API_SECRET_CONFIG_KEY: &str =
    "task_runner.downstream.memory_engine_internal_api_secret";
pub const TASK_RUNNER_LOCAL_CONNECTOR_INTERNAL_API_SECRET_CONFIG_KEY: &str =
    "task_runner.downstream.local_connector_internal_api_secret";
pub const TASK_RUNNER_SANDBOX_MANAGER_INTERNAL_API_SECRET_CONFIG_KEY: &str =
    "task_runner.downstream.sandbox_manager_internal_api_secret";
pub const TASK_RUNNER_PROJECT_SERVICE_CALLER_SECRET_CONFIG_KEY: &str =
    "task_runner.security.project_service_internal_api_secret";
pub const TASK_RUNNER_CHATOS_INTERNAL_API_SECRET_CONFIG_KEY: &str =
    "task_runner.security.chatos_internal_api_secret";
pub const TASK_RUNNER_MCP_MANAGEMENT_INTERNAL_API_SECRET_CONFIG_KEY: &str =
    "task_runner.security.mcp_management_internal_api_secret";
pub const TASK_RUNNER_USER_SERVICE_INTERNAL_API_SECRET_CONFIG_KEY: &str =
    "task_runner.security.user_service_internal_api_secret";
pub const TASK_RUNNER_PLUGIN_MANAGEMENT_INTERNAL_API_SECRET_CONFIG_KEY: &str =
    "task_runner.downstream.plugin_management_internal_api_secret";
pub const TASK_RUNNER_LOCAL_CONNECTOR_SERVICE_BASE_URL_CONFIG_KEY: &str =
    "task_runner.downstream.local_connector_service_base_url";
pub const TASK_RUNNER_LOCAL_CONNECTOR_SERVICE_REQUEST_TIMEOUT_MS_CONFIG_KEY: &str =
    "task_runner.downstream.local_connector_service_request_timeout_ms";
pub const TASK_RUNNER_PLUGIN_RELAY_TIMEOUT_MS_CONFIG_KEY: &str =
    "task_runner.downstream.plugin_relay_timeout_ms";
pub const TASK_RUNNER_PLUGIN_HOOK_RELAY_TIMEOUT_MS_CONFIG_KEY: &str =
    "task_runner.downstream.plugin_hook_relay_timeout_ms";
pub const TASK_RUNNER_PLUGIN_CONNECTOR_DISCOVERY_TIMEOUT_MS_CONFIG_KEY: &str =
    "task_runner.downstream.plugin_connector_discovery_timeout_ms";
pub const TASK_RUNNER_CHATOS_CALLBACK_URL_CONFIG_KEY: &str =
    "task_runner.downstream.chatos_callback_url";
pub const TASK_RUNNER_CALLBACK_TIMEOUT_MS_CONFIG_KEY: &str =
    "task_runner.downstream.callback_timeout_ms";
pub const TASK_RUNNER_OTLP_ENDPOINT_CONFIG_KEY: &str = "task_runner.observability.otlp_endpoint";
pub const TASK_RUNNER_OTLP_TRACE_SAMPLE_RATIO_CONFIG_KEY: &str =
    "task_runner.observability.trace_sample_ratio";
pub const TASK_RUNNER_OTLP_EXPORT_TIMEOUT_MS_CONFIG_KEY: &str =
    "task_runner.observability.export_timeout_ms";
pub const TASK_RUNNER_HOST_CONFIG_KEY: &str = "task_runner.runtime.host";
pub const TASK_RUNNER_PORT_CONFIG_KEY: &str = "task_runner.runtime.port";
pub const TASK_RUNNER_INTERNAL_MTLS_PORT_CONFIG_KEY: &str =
    "task_runner.runtime.internal_mtls_port";
pub const TASK_RUNNER_DATABASE_URL_CONFIG_KEY: &str = "task_runner.runtime.database_url";
pub const TASK_RUNNER_MONGODB_DATABASE_CONFIG_KEY: &str = "task_runner.runtime.mongodb_database";
pub const TASK_RUNNER_WORKSPACE_DIR_CONFIG_KEY: &str = "task_runner.runtime.workspace_dir";
pub const TASK_RUNNER_ADMIN_USERNAME_CONFIG_KEY: &str = "task_runner.bootstrap.admin_username";
pub const TASK_RUNNER_ADMIN_PASSWORD_CONFIG_KEY: &str = "task_runner.bootstrap.admin_password";
pub const TASK_RUNNER_ADMIN_DISPLAY_NAME_CONFIG_KEY: &str =
    "task_runner.bootstrap.admin_display_name";
pub const DEFAULT_RABBITMQ_ROOT_VHOST_URI_SEGMENT: &str = "%2f";
pub const DEFAULT_LOCAL_RABBITMQ_URL: &str =
    "amqp://chatos:change_me_rabbitmq_password@127.0.0.1:5672/%2f";
pub const CHATOS_NODE_ENV_CONFIG_KEY: &str = "chatos.runtime.node_env";
pub const CHATOS_HOST_CONFIG_KEY: &str = "chatos.runtime.host";
pub const CHATOS_BACKEND_PORT_CONFIG_KEY: &str = "chatos.runtime.port";
pub const CHATOS_INTERNAL_MTLS_PORT_CONFIG_KEY: &str = "chatos.runtime.internal_mtls_port";
pub const CHATOS_DATABASE_URL_CONFIG_KEY: &str = "chatos.runtime.database_url";
pub const CHATOS_MONGODB_DATABASE_CONFIG_KEY: &str = "chatos.runtime.mongodb_database";
pub const CHATOS_LEGACY_AUTH_DATABASE_URL_CONFIG_KEY: &str =
    "chatos.runtime.legacy_auth_database_url";
pub const CHATOS_LEGACY_AUTH_MONGODB_DATABASE_CONFIG_KEY: &str =
    "chatos.runtime.legacy_auth_mongodb_database";
pub const CHATOS_LOCAL_PROJECT_CREATION_CONFIG_KEY: &str =
    "chatos.ui.local_project_creation_enabled";
pub const CHATOS_USER_SERVICE_BASE_URL_CONFIG_KEY: &str = "chatos.downstream.user_service_base_url";
pub const CHATOS_USER_SERVICE_REQUEST_TIMEOUT_MS_CONFIG_KEY: &str =
    "chatos.downstream.user_service_request_timeout_ms";
pub const CHATOS_PROJECT_SERVICE_BASE_URL_CONFIG_KEY: &str =
    "chatos.downstream.project_service_base_url";
pub const CHATOS_PROJECT_SERVICE_INTERNAL_BASE_URL_CONFIG_KEY: &str =
    "chatos.downstream.project_service_internal_base_url";
pub const CHATOS_PROJECT_SERVICE_REQUEST_TIMEOUT_MS_CONFIG_KEY: &str =
    "chatos.downstream.project_service_request_timeout_ms";
pub const CHATOS_PROJECT_SERVICE_INTERNAL_API_SECRET_CONFIG_KEY: &str =
    "chatos.downstream.project_service_internal_api_secret";
pub const CHATOS_TASK_RUNNER_BASE_URL_CONFIG_KEY: &str = "chatos.downstream.task_runner_base_url";
pub const CHATOS_TASK_RUNNER_INTERNAL_BASE_URL_CONFIG_KEY: &str =
    "chatos.downstream.task_runner_internal_base_url";
pub const CHATOS_TASK_RUNNER_INTERNAL_API_SECRET_CONFIG_KEY: &str =
    "chatos.downstream.task_runner_internal_api_secret";
pub const CHATOS_TASK_RUNNER_REQUEST_TIMEOUT_MS_CONFIG_KEY: &str =
    "chatos.downstream.task_runner_request_timeout_ms";
pub const CHATOS_MCP_MANAGEMENT_INTERNAL_API_SECRET_CONFIG_KEY: &str =
    "chatos.downstream.mcp_management_internal_api_secret";
pub const CHATOS_PLUGIN_MANAGEMENT_INTERNAL_API_SECRET_CONFIG_KEY: &str =
    "chatos.downstream.plugin_management_internal_api_secret";
pub const CHATOS_LOCAL_CONNECTOR_SERVICE_BASE_URL_CONFIG_KEY: &str =
    "chatos.downstream.local_connector_service_base_url";
pub const CHATOS_LOCAL_CONNECTOR_INTERNAL_API_SECRET_CONFIG_KEY: &str =
    "chatos.downstream.local_connector_internal_api_secret";
pub const CHATOS_LOCAL_CONNECTOR_SERVICE_REQUEST_TIMEOUT_MS_CONFIG_KEY: &str =
    "chatos.downstream.local_connector_service_request_timeout_ms";
pub const CHATOS_MEMORY_ENGINE_BASE_URL_CONFIG_KEY: &str =
    "chatos.downstream.memory_engine_base_url";
pub const CHATOS_MEMORY_ENGINE_INTERNAL_API_SECRET_CONFIG_KEY: &str =
    "chatos.downstream.memory_engine_internal_api_secret";
pub const CHATOS_MEMORY_ENGINE_REQUEST_TIMEOUT_MS_CONFIG_KEY: &str =
    "chatos.downstream.memory_engine_request_timeout_ms";
pub const CHATOS_OPENAI_API_KEY_CONFIG_KEY: &str = "chatos.ai.openai_api_key";
pub const CHATOS_OPENAI_BASE_URL_CONFIG_KEY: &str = "chatos.ai.openai_base_url";
pub const CHATOS_SUMMARY_ENABLED_CONFIG_KEY: &str = "chatos.summary.enabled";
pub const CHATOS_SUMMARY_MESSAGE_LIMIT_CONFIG_KEY: &str = "chatos.summary.message_limit";
pub const CHATOS_SUMMARY_MAX_CONTEXT_TOKENS_CONFIG_KEY: &str = "chatos.summary.max_context_tokens";
pub const CHATOS_SUMMARY_KEEP_LAST_N_CONFIG_KEY: &str = "chatos.summary.keep_last_n";
pub const CHATOS_SUMMARY_TARGET_TOKENS_CONFIG_KEY: &str = "chatos.summary.target_tokens";
pub const CHATOS_SUMMARY_MERGE_TARGET_TOKENS_CONFIG_KEY: &str =
    "chatos.summary.merge_target_tokens";
pub const CHATOS_SUMMARY_TEMPERATURE_CONFIG_KEY: &str = "chatos.summary.temperature";
pub const CHATOS_SUMMARY_COOLDOWN_SECONDS_CONFIG_KEY: &str = "chatos.summary.cooldown_seconds";
pub const CHATOS_DYNAMIC_SUMMARY_ENABLED_CONFIG_KEY: &str = "chatos.summary.dynamic_enabled";
pub const CHATOS_SUMMARY_BISECT_ENABLED_CONFIG_KEY: &str = "chatos.summary.bisect_enabled";
pub const CHATOS_SUMMARY_BISECT_MAX_DEPTH_CONFIG_KEY: &str = "chatos.summary.bisect_max_depth";
pub const CHATOS_SUMMARY_BISECT_MIN_MESSAGES_CONFIG_KEY: &str =
    "chatos.summary.bisect_min_messages";
pub const CHATOS_SUMMARY_RETRY_ON_CONTEXT_OVERFLOW_CONFIG_KEY: &str =
    "chatos.summary.retry_on_context_overflow";
pub const CHATOS_AUTH_JWT_SECRET_CONFIG_KEY: &str = "chatos.auth.jwt_secret";
pub const CHATOS_AUTH_COMPAT_SECRET_CONFIG_KEY: &str = "chatos.auth.compat_secret";
pub const CHATOS_AUTH_ACCESS_TOKEN_TTL_SECONDS_CONFIG_KEY: &str =
    "chatos.auth.access_token_ttl_seconds";
pub const CHATOS_LOG_MAX_FILES_CONFIG_KEY: &str = "chatos.logging.max_files";
pub const CHATOS_OTLP_ENDPOINT_CONFIG_KEY: &str = "chatos.observability.otlp_endpoint";
pub const CHATOS_OTLP_TRACE_SAMPLE_RATIO_CONFIG_KEY: &str =
    "chatos.observability.trace_sample_ratio";
pub const CHATOS_OTLP_EXPORT_TIMEOUT_MS_CONFIG_KEY: &str = "chatos.observability.export_timeout_ms";
pub const CHATOS_MCP_RESULT_RABBITMQ_URL_CONFIG_KEY: &str = "chatos.mcp.result_rabbitmq_url";
pub const CHATOS_MCP_RESULT_QUEUE_PREFIX_CONFIG_KEY: &str = "chatos.mcp.result_queue_prefix";
pub const CHATOS_CORS_ORIGINS_CONFIG_KEY: &str = "chatos.http.cors_origins";
pub const CHATOS_PLUGIN_UI_PARENT_ORIGIN_CONFIG_KEY: &str = "chatos.plugin_ui.parent_origin";
pub const CHATOS_PLUGIN_UI_RESOURCE_ORIGIN_CONFIG_KEY: &str = "chatos.plugin_ui.resource_origin";
pub const CHATOS_MEMORY_ENGINE_ACTIVE_SUMMARY_TRIGGER_TIMEOUT_MS_CONFIG_KEY: &str =
    "chatos.memory_engine.active_summary_trigger_timeout_ms";
pub const CHATOS_MEMORY_ENGINE_ACTIVE_SUMMARY_POLL_INTERVAL_MS_CONFIG_KEY: &str =
    "chatos.memory_engine.active_summary_poll_interval_ms";
pub const CHATOS_MEMORY_ENGINE_ACTIVE_SUMMARY_POLL_TIMEOUT_MS_CONFIG_KEY: &str =
    "chatos.memory_engine.active_summary_poll_timeout_ms";
pub const SANDBOX_MANAGER_POOL_MAX_ACTIVE_CONFIG_KEY: &str = "sandbox_manager.pool.max_active";
pub const SANDBOX_MANAGER_POOL_MAX_PENDING_CONFIG_KEY: &str = "sandbox_manager.pool.max_pending";
pub const SANDBOX_MANAGER_LEASE_TTL_SECONDS_CONFIG_KEY: &str =
    "sandbox_manager.pool.lease_ttl_seconds";
pub const SANDBOX_MANAGER_CLEANUP_INTERVAL_SECONDS_CONFIG_KEY: &str =
    "sandbox_manager.pool.cleanup_interval_seconds";
pub const SANDBOX_MANAGER_DOCKER_MAINTENANCE_ENABLED_CONFIG_KEY: &str =
    "sandbox_manager.docker.maintenance_enabled";
pub const SANDBOX_MANAGER_DOCKER_BUILD_CACHE_MAX_USED_SPACE_CONFIG_KEY: &str =
    "sandbox_manager.docker.build_cache_max_used_space";
pub const SANDBOX_MANAGER_DOCKER_BUILD_CACHE_RESERVED_SPACE_CONFIG_KEY: &str =
    "sandbox_manager.docker.build_cache_reserved_space";
pub const SANDBOX_MANAGER_DOCKER_BUILD_CACHE_TIMEOUT_SECS_CONFIG_KEY: &str =
    "sandbox_manager.docker.build_cache_timeout_secs";
pub const SANDBOX_MANAGER_HOST_CONFIG_KEY: &str = "sandbox_manager.runtime.host";
pub const SANDBOX_MANAGER_PORT_CONFIG_KEY: &str = "sandbox_manager.runtime.port";
pub const SANDBOX_MANAGER_INTERNAL_MTLS_PORT_CONFIG_KEY: &str =
    "sandbox_manager.runtime.internal_mtls_port";
pub const SANDBOX_MANAGER_DATABASE_URL_CONFIG_KEY: &str = "sandbox_manager.runtime.database_url";
pub const SANDBOX_MANAGER_MONGODB_DATABASE_CONFIG_KEY: &str =
    "sandbox_manager.runtime.mongodb_database";
pub const SANDBOX_MANAGER_AGENT_PORT_CONFIG_KEY: &str = "sandbox_manager.runtime.agent_port";
pub const LOCAL_CONNECTOR_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY: &str =
    "local_connector.security.require_signed_internal_requests";
pub const LOCAL_CONNECTOR_CHATOS_INTERNAL_API_SECRET_CONFIG_KEY: &str =
    "local_connector.security.chatos_internal_api_secret";
pub const LOCAL_CONNECTOR_TASK_RUNNER_INTERNAL_API_SECRET_CONFIG_KEY: &str =
    "local_connector.security.task_runner_internal_api_secret";
pub const LOCAL_CONNECTOR_PROJECT_SERVICE_INTERNAL_API_SECRET_CONFIG_KEY: &str =
    "local_connector.security.project_service_internal_api_secret";
pub const LOCAL_CONNECTOR_MCP_MANAGEMENT_INTERNAL_API_SECRET_CONFIG_KEY: &str =
    "local_connector.security.mcp_management_internal_api_secret";
pub const LOCAL_CONNECTOR_PLUGIN_MANAGEMENT_INTERNAL_API_SECRET_CONFIG_KEY: &str =
    "local_connector.downstream.plugin_management_internal_api_secret";
pub const LOCAL_CONNECTOR_HOST_CONFIG_KEY: &str = "local_connector.runtime.host";
pub const LOCAL_CONNECTOR_PORT_CONFIG_KEY: &str = "local_connector.runtime.port";
pub const LOCAL_CONNECTOR_INTERNAL_MTLS_PORT_CONFIG_KEY: &str =
    "local_connector.runtime.internal_mtls_port";
pub const LOCAL_CONNECTOR_DATABASE_URL_CONFIG_KEY: &str = "local_connector.runtime.database_url";
pub const LOCAL_CONNECTOR_USER_SERVICE_BASE_URL_CONFIG_KEY: &str =
    "local_connector.downstream.user_service_base_url";
pub const LOCAL_CONNECTOR_USER_SERVICE_REQUEST_TIMEOUT_MS_CONFIG_KEY: &str =
    "local_connector.downstream.user_service_request_timeout_ms";
pub const LOCAL_CONNECTOR_RELAY_REQUEST_TIMEOUT_MS_CONFIG_KEY: &str =
    "local_connector.relay.request_timeout_ms";
pub const LOCAL_CONNECTOR_PLUGIN_HOOK_RELAY_REQUEST_TIMEOUT_MS_CONFIG_KEY: &str =
    "local_connector.relay.plugin_hook_request_timeout_ms";
pub const LOCAL_CONNECTOR_SANDBOX_IMAGE_RELAY_REQUEST_TIMEOUT_MS_CONFIG_KEY: &str =
    "local_connector.relay.sandbox_image_request_timeout_ms";
pub const LOCAL_CONNECTOR_PUBLIC_BASE_URL_CONFIG_KEY: &str = "local_connector.public.base_url";
pub const LOCAL_CONNECTOR_REQUIRE_DEVICE_CONNECT_SIGNATURE_CONFIG_KEY: &str =
    "local_connector.device_connect.require_signature";
pub const LOCAL_CONNECTOR_DEVICE_CONNECT_SIGNATURE_MAX_SKEW_SECONDS_CONFIG_KEY: &str =
    "local_connector.device_connect.signature_max_skew_seconds";
pub const LOCAL_CONNECTOR_ACTIVE_SESSION_LEASE_TTL_SECONDS_CONFIG_KEY: &str =
    "local_connector.device_connect.active_session_lease_ttl_seconds";
pub const LOCAL_CONNECTOR_VALKEY_URL_CONFIG_KEY: &str = "local_connector.coordination.valkey_url";
pub const LOCAL_CONNECTOR_VALKEY_KEY_PREFIX_CONFIG_KEY: &str =
    "local_connector.coordination.key_prefix";
pub const LOCAL_CONNECTOR_DEVICE_PRESENCE_TTL_SECONDS_CONFIG_KEY: &str =
    "local_connector.coordination.device_presence_ttl_seconds";
pub const LOCAL_CONNECTOR_VALKEY_RECONNECT_MS_CONFIG_KEY: &str =
    "local_connector.coordination.valkey_reconnect_ms";
pub const LOCAL_CONNECTOR_RELAY_CORRELATION_GRACE_SECONDS_CONFIG_KEY: &str =
    "local_connector.coordination.relay_correlation_grace_seconds";
pub const LOCAL_CONNECTOR_RELAY_DELIVERY_ACK_TIMEOUT_MS_CONFIG_KEY: &str =
    "local_connector.coordination.relay_delivery_ack_timeout_ms";
pub const LOCAL_CONNECTOR_TERMINAL_SUBSCRIBER_TTL_SECONDS_CONFIG_KEY: &str =
    "local_connector.coordination.terminal_subscriber_ttl_seconds";
pub const LOCAL_CONNECTOR_TERMINAL_SUBSCRIBER_REFRESH_SECONDS_CONFIG_KEY: &str =
    "local_connector.coordination.terminal_subscriber_refresh_seconds";
pub const LOCAL_CONNECTOR_MANAGED_REQUIREMENTS_BUNDLE_TTL_SECONDS_CONFIG_KEY: &str =
    "local_connector.managed_requirements.bundle_ttl_seconds";
pub const LOCAL_CONNECTOR_MANAGED_REQUIREMENTS_TOML_PATH_CONFIG_KEY: &str =
    "local_connector.managed_requirements.toml_path";
pub const LOCAL_CONNECTOR_MANAGED_REQUIREMENTS_SIGNING_KEY_PATH_CONFIG_KEY: &str =
    "local_connector.managed_requirements.signing_key_path";
pub const LOCAL_CONNECTOR_MANAGED_REQUIREMENTS_SIGNING_KEY_ID_CONFIG_KEY: &str =
    "local_connector.managed_requirements.signing_key_id";
pub const MCP_MANAGEMENT_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY: &str =
    "mcp_management.security.require_signed_internal_requests";
pub const PROJECT_SERVICE_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY: &str =
    "project_service.security.require_signed_internal_requests";
pub const PROJECT_SERVICE_CHATOS_INTERNAL_API_SECRET_CONFIG_KEY: &str =
    "project_service.security.chatos_internal_api_secret";
pub const PROJECT_SERVICE_TASK_RUNNER_INTERNAL_API_SECRET_CONFIG_KEY: &str =
    "project_service.security.task_runner_internal_api_secret";
pub const PROJECT_SERVICE_SELF_INTERNAL_API_SECRET_CONFIG_KEY: &str =
    "project_service.security.self_internal_api_secret";
pub const PROJECT_SERVICE_MCP_MANAGEMENT_INTERNAL_API_SECRET_CONFIG_KEY: &str =
    "project_service.security.mcp_management_internal_api_secret";
pub const PROJECT_SERVICE_PLUGIN_MANAGEMENT_INTERNAL_API_SECRET_CONFIG_KEY: &str =
    "project_service.downstream.plugin_management_internal_api_secret";
pub const PROJECT_SERVICE_OTLP_ENDPOINT_CONFIG_KEY: &str =
    "project_service.observability.otlp_endpoint";
pub const PROJECT_SERVICE_OTLP_TRACE_SAMPLE_RATIO_CONFIG_KEY: &str =
    "project_service.observability.trace_sample_ratio";
pub const PROJECT_SERVICE_OTLP_EXPORT_TIMEOUT_MS_CONFIG_KEY: &str =
    "project_service.observability.export_timeout_ms";
pub const PROJECT_SERVICE_HOST_CONFIG_KEY: &str = "project_service.runtime.host";
pub const PROJECT_SERVICE_PORT_CONFIG_KEY: &str = "project_service.runtime.port";
pub const PROJECT_SERVICE_INTERNAL_MTLS_PORT_CONFIG_KEY: &str =
    "project_service.runtime.internal_mtls_port";
pub const PROJECT_SERVICE_DATABASE_URL_CONFIG_KEY: &str = "project_service.runtime.database_url";
pub const PROJECT_SERVICE_MCP_RESULT_RABBITMQ_URL_CONFIG_KEY: &str =
    "project_service.mcp.result_rabbitmq_url";
pub const PROJECT_SERVICE_MCP_RESULT_QUEUE_PREFIX_CONFIG_KEY: &str =
    "project_service.mcp.result_queue_prefix";
pub const PROJECT_SERVICE_USER_SERVICE_INTERNAL_SECRET_CONFIG_KEY: &str =
    "project_service.downstream.user_service_internal_secret";
pub const PROJECT_SERVICE_USER_SERVICE_BASE_URL_CONFIG_KEY: &str =
    "project_service.downstream.user_service_base_url";
pub const PROJECT_SERVICE_USER_SERVICE_INTERNAL_BASE_URL_CONFIG_KEY: &str =
    "project_service.downstream.user_service_internal_base_url";
pub const PROJECT_SERVICE_USER_SERVICE_REQUEST_TIMEOUT_MS_CONFIG_KEY: &str =
    "project_service.downstream.user_service_request_timeout_ms";
pub const PROJECT_SERVICE_TASK_RUNNER_INTERNAL_SECRET_CONFIG_KEY: &str =
    "project_service.downstream.task_runner_internal_secret";
pub const PROJECT_SERVICE_TASK_RUNNER_BASE_URL_CONFIG_KEY: &str =
    "project_service.downstream.task_runner_base_url";
pub const PROJECT_SERVICE_TASK_RUNNER_REQUEST_TIMEOUT_MS_CONFIG_KEY: &str =
    "project_service.downstream.task_runner_request_timeout_ms";
pub const PROJECT_SERVICE_SYNC_SECRET_CONFIG_KEY: &str = "project_service.security.sync_secret";
pub const PROJECT_SERVICE_LOCAL_CONNECTOR_SERVICE_BASE_URL_CONFIG_KEY: &str =
    "project_service.downstream.local_connector_service_base_url";
pub const PROJECT_SERVICE_LOCAL_CONNECTOR_SERVICE_REQUEST_TIMEOUT_MS_CONFIG_KEY: &str =
    "project_service.downstream.local_connector_service_request_timeout_ms";
pub const PROJECT_SERVICE_MEMORY_ENGINE_BASE_URL_CONFIG_KEY: &str =
    "project_service.downstream.memory_engine_base_url";
pub const PROJECT_SERVICE_MEMORY_ENGINE_INTERNAL_API_SECRET_CONFIG_KEY: &str =
    "project_service.downstream.memory_engine_internal_api_secret";
pub const PROJECT_SERVICE_MEMORY_ENGINE_REQUEST_TIMEOUT_MS_CONFIG_KEY: &str =
    "project_service.downstream.memory_engine_request_timeout_ms";
pub const PROJECT_SERVICE_SANDBOX_MANAGER_BASE_URL_CONFIG_KEY: &str =
    "project_service.downstream.sandbox_manager_base_url";
pub const PROJECT_SERVICE_SANDBOX_MANAGER_CLIENT_ID_CONFIG_KEY: &str =
    "project_service.downstream.sandbox_manager_client_id";
pub const PROJECT_SERVICE_SANDBOX_MANAGER_CLIENT_KEY_CONFIG_KEY: &str =
    "project_service.downstream.sandbox_manager_client_key";
pub const PROJECT_SERVICE_SANDBOX_IMAGE_MCP_REQUEST_TIMEOUT_MS_CONFIG_KEY: &str =
    "project_service.downstream.sandbox_image_mcp_request_timeout_ms";
pub const PROJECT_SERVICE_CLOUD_PROJECT_IMPORT_ENABLED_CONFIG_KEY: &str =
    "project_service.cloud_import.enabled";
pub const PROJECT_SERVICE_CLOUD_PROJECT_MAX_ZIP_BYTES_CONFIG_KEY: &str =
    "project_service.cloud_import.max_zip_bytes";
pub const PROJECT_SERVICE_CLOUD_PROJECT_MAX_UNPACKED_BYTES_CONFIG_KEY: &str =
    "project_service.cloud_import.max_unpacked_bytes";
pub const PROJECT_SERVICE_CLOUD_PROJECT_MAX_FILES_CONFIG_KEY: &str =
    "project_service.cloud_import.max_files";
pub const PROJECT_SERVICE_CLOUD_PROJECT_GIT_TIMEOUT_MS_CONFIG_KEY: &str =
    "project_service.cloud_import.git_timeout_ms";
pub const PROJECT_SERVICE_ENVIRONMENT_ANALYSIS_TIMEOUT_MS_CONFIG_KEY: &str =
    "project_service.environment_analysis.timeout_ms";
pub const PROJECT_SERVICE_ENVIRONMENT_ANALYSIS_STALE_AFTER_MS_CONFIG_KEY: &str =
    "project_service.environment_analysis.stale_after_ms";
pub const MEMORY_ENGINE_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY: &str =
    "memory_engine.security.require_signed_internal_requests";
pub const CONFIGURATION_CENTER_MEMORY_ENGINE_BASE_URL_CONFIG_KEY: &str =
    "configuration_center.downstream.memory_engine_base_url";
pub const CONFIGURATION_CENTER_PLUGIN_MANAGEMENT_BASE_URL_CONFIG_KEY: &str =
    "configuration_center.downstream.plugin_management_base_url";
pub const CONFIGURATION_CENTER_MCP_MANAGEMENT_BASE_URL_CONFIG_KEY: &str =
    "configuration_center.downstream.mcp_management_base_url";
pub const CONFIGURATION_CENTER_MCP_MANAGEMENT_INTERNAL_API_SECRET_CONFIG_KEY: &str =
    "configuration_center.downstream.mcp_management_internal_api_secret";
pub const MEMORY_ENGINE_CHATOS_INTERNAL_API_SECRET_CONFIG_KEY: &str =
    "memory_engine.security.chatos_internal_api_secret";
pub const MEMORY_ENGINE_TASK_RUNNER_INTERNAL_API_SECRET_CONFIG_KEY: &str =
    "memory_engine.security.task_runner_internal_api_secret";
pub const MEMORY_ENGINE_PROJECT_SERVICE_INTERNAL_API_SECRET_CONFIG_KEY: &str =
    "memory_engine.security.project_service_internal_api_secret";
pub const MEMORY_ENGINE_USER_SERVICE_INTERNAL_API_SECRET_CONFIG_KEY: &str =
    "memory_engine.security.user_service_internal_api_secret";
pub const MEMORY_ENGINE_CONFIGURATION_CENTER_INTERNAL_API_SECRET_CONFIG_KEY: &str =
    "memory_engine.security.configuration_center_internal_api_secret";
pub const MEMORY_ENGINE_PLUGIN_MANAGEMENT_INTERNAL_API_SECRET_CONFIG_KEY: &str =
    "memory_engine.downstream.plugin_management_internal_api_secret";
pub const MEMORY_ENGINE_HOST_CONFIG_KEY: &str = "memory_engine.runtime.host";
pub const MEMORY_ENGINE_PORT_CONFIG_KEY: &str = "memory_engine.runtime.port";
pub const MEMORY_ENGINE_INTERNAL_MTLS_PORT_CONFIG_KEY: &str =
    "memory_engine.runtime.internal_mtls_port";
pub const MEMORY_ENGINE_MONGODB_URI_CONFIG_KEY: &str = "memory_engine.runtime.mongodb_uri";
pub const MEMORY_ENGINE_MONGODB_DATABASE_CONFIG_KEY: &str =
    "memory_engine.runtime.mongodb_database";
pub const MEMORY_ENGINE_USER_SERVICE_BASE_URL_CONFIG_KEY: &str =
    "memory_engine.downstream.user_service_base_url";
pub const MEMORY_ENGINE_USER_SERVICE_REQUEST_TIMEOUT_MS_CONFIG_KEY: &str =
    "memory_engine.downstream.user_service_request_timeout_ms";
pub const MEMORY_ENGINE_AI_REQUEST_TIMEOUT_SECS_CONFIG_KEY: &str =
    "memory_engine.ai.request_timeout_secs";
pub const MEMORY_ENGINE_OPENAI_API_KEY_CONFIG_KEY: &str = "memory_engine.ai.openai_api_key";
pub const MEMORY_ENGINE_OPENAI_BASE_URL_CONFIG_KEY: &str = "memory_engine.ai.openai_base_url";
pub const MEMORY_ENGINE_OPENAI_MODEL_CONFIG_KEY: &str = "memory_engine.ai.openai_model";
pub const MEMORY_ENGINE_OPENAI_TEMPERATURE_CONFIG_KEY: &str = "memory_engine.ai.openai_temperature";
pub const MEMORY_ENGINE_WORKER_ENABLED_CONFIG_KEY: &str = "memory_engine.worker.enabled";
pub const MEMORY_ENGINE_WORKER_INTERVAL_SECS_CONFIG_KEY: &str =
    "memory_engine.worker.interval_secs";
pub const MEMORY_ENGINE_WORKER_MAX_THREADS_PER_TICK_CONFIG_KEY: &str =
    "memory_engine.worker.max_threads_per_tick";
pub const MEMORY_ENGINE_WORKER_SUMMARY_CONCURRENCY_CONFIG_KEY: &str =
    "memory_engine.worker.summary_concurrency";
pub const MEMORY_ENGINE_WORKER_PRESSURE_SUMMARY_CONCURRENCY_CONFIG_KEY: &str =
    "memory_engine.worker.pressure_summary_concurrency";
pub const MEMORY_ENGINE_WORKER_PRESSURE_REFRESH_INTERVAL_MS_CONFIG_KEY: &str =
    "memory_engine.worker.pressure_refresh_interval_ms";
pub const MEMORY_ENGINE_PRESSURE_QUEUE_ELEVATED_MESSAGES_CONFIG_KEY: &str =
    "memory_engine.pressure.queue_elevated_messages";
pub const MEMORY_ENGINE_PRESSURE_QUEUE_CRITICAL_MESSAGES_CONFIG_KEY: &str =
    "memory_engine.pressure.queue_critical_messages";
pub const MEMORY_ENGINE_WORKER_ROLLUP_CONCURRENCY_CONFIG_KEY: &str =
    "memory_engine.worker.rollup_concurrency";
pub const MEMORY_ENGINE_WORKER_SUBJECT_MEMORY_CONCURRENCY_CONFIG_KEY: &str =
    "memory_engine.worker.subject_memory_concurrency";
pub const MEMORY_ENGINE_WORKER_RECONCILE_CONCURRENCY_CONFIG_KEY: &str =
    "memory_engine.worker.reconcile_concurrency";
pub const MEMORY_ENGINE_RABBITMQ_URL_CONFIG_KEY: &str = "memory_engine.queue.rabbitmq_url";
pub const MEMORY_ENGINE_RABBITMQ_EXCHANGE_CONFIG_KEY: &str =
    "memory_engine.queue.rabbitmq_exchange";
pub const MEMORY_ENGINE_RABBITMQ_RECONNECT_DELAY_MS_CONFIG_KEY: &str =
    "memory_engine.queue.rabbitmq_reconnect_delay_ms";
pub const MEMORY_ENGINE_SUMMARY_QUEUE_CONFIG_KEY: &str = "memory_engine.queue.summary_queue";
pub const MEMORY_ENGINE_SUMMARY_RETRY_QUEUE_CONFIG_KEY: &str =
    "memory_engine.queue.summary_retry_queue";
pub const MEMORY_ENGINE_SUMMARY_DEAD_LETTER_QUEUE_CONFIG_KEY: &str =
    "memory_engine.queue.summary_dead_letter_queue";
pub const MEMORY_ENGINE_SUMMARY_MAX_DELIVERY_ATTEMPTS_CONFIG_KEY: &str =
    "memory_engine.queue.summary_max_delivery_attempts";
pub const MEMORY_ENGINE_SUMMARY_RETRY_DELAY_MS_CONFIG_KEY: &str =
    "memory_engine.queue.summary_retry_delay_ms";
pub const MEMORY_ENGINE_SUMMARY_OUTBOX_RECONCILE_MS_CONFIG_KEY: &str =
    "memory_engine.queue.summary_outbox_reconcile_ms";
pub const MEMORY_ENGINE_SUMMARY_OUTBOX_BATCH_SIZE_CONFIG_KEY: &str =
    "memory_engine.queue.summary_outbox_batch_size";
pub const MEMORY_ENGINE_ROLLUP_QUEUE_CONFIG_KEY: &str = "memory_engine.queue.rollup_queue";
pub const MEMORY_ENGINE_ROLLUP_RETRY_QUEUE_CONFIG_KEY: &str =
    "memory_engine.queue.rollup_retry_queue";
pub const MEMORY_ENGINE_ROLLUP_DEAD_LETTER_QUEUE_CONFIG_KEY: &str =
    "memory_engine.queue.rollup_dead_letter_queue";
pub const MEMORY_ENGINE_ROLLUP_MAX_DELIVERY_ATTEMPTS_CONFIG_KEY: &str =
    "memory_engine.queue.rollup_max_delivery_attempts";
pub const MEMORY_ENGINE_ROLLUP_RETRY_DELAY_MS_CONFIG_KEY: &str =
    "memory_engine.queue.rollup_retry_delay_ms";
pub const MEMORY_ENGINE_ROLLUP_OUTBOX_RECONCILE_MS_CONFIG_KEY: &str =
    "memory_engine.queue.rollup_outbox_reconcile_ms";
pub const MEMORY_ENGINE_ROLLUP_OUTBOX_BATCH_SIZE_CONFIG_KEY: &str =
    "memory_engine.queue.rollup_outbox_batch_size";
pub const MEMORY_ENGINE_SUBJECT_MEMORY_QUEUE_CONFIG_KEY: &str =
    "memory_engine.queue.subject_memory_queue";
pub const MEMORY_ENGINE_SUBJECT_MEMORY_RETRY_QUEUE_CONFIG_KEY: &str =
    "memory_engine.queue.subject_memory_retry_queue";
pub const MEMORY_ENGINE_SUBJECT_MEMORY_DEAD_LETTER_QUEUE_CONFIG_KEY: &str =
    "memory_engine.queue.subject_memory_dead_letter_queue";
pub const MEMORY_ENGINE_SUBJECT_MEMORY_MAX_DELIVERY_ATTEMPTS_CONFIG_KEY: &str =
    "memory_engine.queue.subject_memory_max_delivery_attempts";
pub const MEMORY_ENGINE_SUBJECT_MEMORY_RETRY_DELAY_MS_CONFIG_KEY: &str =
    "memory_engine.queue.subject_memory_retry_delay_ms";
pub const MEMORY_ENGINE_SUBJECT_MEMORY_OUTBOX_RECONCILE_MS_CONFIG_KEY: &str =
    "memory_engine.queue.subject_memory_outbox_reconcile_ms";
pub const MEMORY_ENGINE_SUBJECT_MEMORY_OUTBOX_BATCH_SIZE_CONFIG_KEY: &str =
    "memory_engine.queue.subject_memory_outbox_batch_size";
pub const MEMORY_ENGINE_SUBJECT_MEMORY_LOCK_TIMEOUT_SECS_CONFIG_KEY: &str =
    "memory_engine.queue.subject_memory_lock_timeout_secs";
pub const MEMORY_ENGINE_RECORD_SYNC_LEASE_TIMEOUT_SECS_CONFIG_KEY: &str =
    "memory_engine.queue.record_sync_lease_timeout_secs";
pub const MEMORY_ENGINE_ROLLUP_LOCK_TIMEOUT_SECS_CONFIG_KEY: &str =
    "memory_engine.queue.rollup_lock_timeout_secs";
pub const SANDBOX_MANAGER_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY: &str =
    "sandbox_manager.security.require_signed_internal_requests";
pub const SANDBOX_MANAGER_TASK_RUNNER_INTERNAL_API_SECRET_CONFIG_KEY: &str =
    "sandbox_manager.security.task_runner_internal_api_secret";
pub const SANDBOX_MANAGER_PROJECT_SERVICE_INTERNAL_API_SECRET_CONFIG_KEY: &str =
    "sandbox_manager.security.project_service_internal_api_secret";
pub const SANDBOX_MANAGER_MCP_MANAGEMENT_INTERNAL_API_SECRET_CONFIG_KEY: &str =
    "sandbox_manager.security.mcp_management_internal_api_secret";
pub const SANDBOX_MANAGER_AGENT_TOKEN_SECRET_CONFIG_KEY: &str =
    "sandbox_manager.security.agent_token_secret";
pub const SANDBOX_MANAGER_FRONTEND_PROXY_CLIENT_ID_CONFIG_KEY: &str =
    "sandbox_manager.frontend.proxy_client_id";
pub const SANDBOX_MANAGER_FRONTEND_PROXY_CLIENT_KEY_CONFIG_KEY: &str =
    "sandbox_manager.frontend.proxy_client_key";
pub const SANDBOX_MANAGER_REQUIRE_AUTH_CONFIG_KEY: &str = "sandbox_manager.security.require_auth";
pub const SANDBOX_MANAGER_USER_SERVICE_BASE_URL_CONFIG_KEY: &str =
    "sandbox_manager.downstream.user_service_base_url";
pub const SANDBOX_MANAGER_USER_SERVICE_REQUEST_TIMEOUT_MS_CONFIG_KEY: &str =
    "sandbox_manager.downstream.user_service_request_timeout_ms";
pub const SANDBOX_MANAGER_SYSTEM_CLIENT_MAX_LEASE_TTL_SECONDS_CONFIG_KEY: &str =
    "sandbox_manager.security.system_client_max_lease_ttl_seconds";
pub const LOCAL_CONNECTOR_RELAY_SIGNING_KEY_PATH_CONFIG_KEY: &str =
    "local_connector.security.relay_signing.key_path";
pub const LOCAL_CONNECTOR_RELAY_SIGNING_KEY_ID_CONFIG_KEY: &str =
    "local_connector.security.relay_signing.key_id";
pub const LOCAL_CONNECTOR_REMOTE_CONTROL_REQUIRE_SIGNED_CONFIG_KEY: &str =
    "local_connector.remote_control.require_signed_messages";
pub const LOCAL_CONNECTOR_REMOTE_CONTROL_SIGNATURE_MAX_SKEW_SECONDS_CONFIG_KEY: &str =
    "local_connector.remote_control.signature_max_skew_seconds";
pub const LOCAL_CONNECTOR_REMOTE_CONTROL_TRUSTED_RELAY_PUBLIC_KEYS_CONFIG_KEY: &str =
    "local_connector.remote_control.trusted_relay_public_keys";
pub const LOCAL_CONNECTOR_RELAY_MAX_PENDING_REQUESTS_PER_DEVICE_CONFIG_KEY: &str =
    "local_connector.relay.max_pending_requests_per_device";
pub const LOCAL_CONNECTOR_TERMINAL_MAX_EVENT_BYTES_CONFIG_KEY: &str =
    "local_connector.terminal.max_event_bytes";
pub const LOCAL_CONNECTOR_TERMINAL_EVENT_CHANNEL_CAPACITY_CONFIG_KEY: &str =
    "local_connector.terminal.event_channel_capacity";
pub const LOCAL_CONNECTOR_TERMINAL_MAX_ACTIVE_SESSIONS_CONFIG_KEY: &str =
    "local_connector.terminal.max_active_sessions";
pub const LOCAL_CONNECTOR_TERMINAL_NEW_SESSION_SOFT_LIMIT_CONFIG_KEY: &str =
    "local_connector.terminal.new_session_soft_limit";
pub const LOCAL_CONNECTOR_TERMINAL_MAX_SUBSCRIBERS_PER_SESSION_CONFIG_KEY: &str =
    "local_connector.terminal.max_subscribers_per_session";
pub const LOCAL_CONNECTOR_PRESSURE_PENDING_RELAY_ELEVATED_CONFIG_KEY: &str =
    "local_connector.pressure.pending_relay_elevated_requests";
pub const LOCAL_CONNECTOR_PRESSURE_PENDING_RELAY_CRITICAL_CONFIG_KEY: &str =
    "local_connector.pressure.pending_relay_critical_requests";
pub const LOCAL_CONNECTOR_PRESSURE_REPORT_INTERVAL_MS_CONFIG_KEY: &str =
    "local_connector.pressure.report_interval_ms";
pub const MCP_MANAGEMENT_ASYNC_TOOL_DISPATCH_MODE_CONFIG_KEY: &str =
    "mcp_management.async_tool.dispatch_mode";
pub const MCP_MANAGEMENT_ASYNC_TOOL_WORKER_CONCURRENCY_CONFIG_KEY: &str =
    "mcp_management.async_tool.worker_concurrency";
pub const MCP_MANAGEMENT_ASYNC_TOOL_LOCAL_QUEUE_BUFFER_CONFIG_KEY: &str =
    "mcp_management.async_tool.local_queue_buffer";
pub const MCP_MANAGEMENT_ASYNC_TOOL_RABBITMQ_URL_CONFIG_KEY: &str =
    "mcp_management.async_tool.rabbitmq_url";
pub const MCP_MANAGEMENT_ASYNC_TOOL_RABBITMQ_EXCHANGE_CONFIG_KEY: &str =
    "mcp_management.async_tool.rabbitmq_exchange";
pub const MCP_MANAGEMENT_INVOCATION_CANCELLATION_EXCHANGE_CONFIG_KEY: &str =
    "mcp_management.invocation.cancellation_exchange";
pub const MCP_MANAGEMENT_ASYNC_TOOL_DISPATCH_QUEUE_CONFIG_KEY: &str =
    "mcp_management.async_tool.dispatch_queue";
pub const MCP_MANAGEMENT_ASYNC_TOOL_QUEUE_MAX_LENGTH_CONFIG_KEY: &str =
    "mcp_management.async_tool.queue_max_length";
pub const MCP_MANAGEMENT_ASYNC_TOOL_QUEUE_MAX_BYTES_CONFIG_KEY: &str =
    "mcp_management.async_tool.queue_max_bytes";
pub const MCP_MANAGEMENT_PRESSURE_QUEUE_ELEVATED_PERCENT_CONFIG_KEY: &str =
    "mcp_management.pressure.queue_elevated_percent";
pub const MCP_MANAGEMENT_PRESSURE_QUEUE_CRITICAL_PERCENT_CONFIG_KEY: &str =
    "mcp_management.pressure.queue_critical_percent";
pub const MCP_MANAGEMENT_PRESSURE_REPORT_INTERVAL_MS_CONFIG_KEY: &str =
    "mcp_management.pressure.report_interval_ms";
pub const MCP_MANAGEMENT_ASYNC_TOOL_RABBITMQ_RECONNECT_MS_CONFIG_KEY: &str =
    "mcp_management.async_tool.rabbitmq_reconnect_ms";
pub const MCP_MANAGEMENT_ASYNC_TOOL_RESULT_OUTBOX_RECONCILE_MS_CONFIG_KEY: &str =
    "mcp_management.async_tool.result_outbox_reconcile_ms";
pub const MCP_MANAGEMENT_ASYNC_TOOL_RESULT_OUTBOX_BATCH_SIZE_CONFIG_KEY: &str =
    "mcp_management.async_tool.result_outbox_batch_size";
pub const MCP_MANAGEMENT_ASYNC_TOOL_MAX_DELIVERY_ATTEMPTS_CONFIG_KEY: &str =
    "mcp_management.async_tool.max_delivery_attempts";
pub const MCP_MANAGEMENT_ASYNC_TOOL_RETRY_DELAY_MS_CONFIG_KEY: &str =
    "mcp_management.async_tool.retry_delay_ms";
pub const MCP_MANAGEMENT_ASYNC_TOOL_RETRY_QUEUE_CONFIG_KEY: &str =
    "mcp_management.async_tool.retry_queue";
pub const MCP_MANAGEMENT_ASYNC_TOOL_DEAD_LETTER_QUEUE_CONFIG_KEY: &str =
    "mcp_management.async_tool.dead_letter_queue";
pub const MCP_MANAGEMENT_ALLOWED_INTERNAL_CALLERS_CONFIG_KEY: &str =
    "mcp_management.security.allowed_internal_callers";
pub const MCP_MANAGEMENT_CONFIGURATION_CENTER_INTERNAL_API_SECRET_CONFIG_KEY: &str =
    "mcp_management.security.configuration_center_internal_api_secret";
pub const MCP_MANAGEMENT_PLUGIN_MANAGEMENT_INTERNAL_API_SECRET_CONFIG_KEY: &str =
    "mcp_management.downstream.plugin_management_internal_api_secret";
pub const MCP_MANAGEMENT_PROJECT_SERVICE_INTERNAL_API_SECRET_CONFIG_KEY: &str =
    "mcp_management.downstream.project_service_internal_api_secret";
pub const MCP_MANAGEMENT_TASK_RUNNER_INTERNAL_API_SECRET_CONFIG_KEY: &str =
    "mcp_management.downstream.task_runner_internal_api_secret";
pub const MCP_MANAGEMENT_CHATOS_INTERNAL_API_SECRET_CONFIG_KEY: &str =
    "mcp_management.downstream.chatos_internal_api_secret";
pub const MCP_MANAGEMENT_LOCAL_CONNECTOR_INTERNAL_API_SECRET_CONFIG_KEY: &str =
    "mcp_management.downstream.local_connector_internal_api_secret";
pub const MCP_MANAGEMENT_SANDBOX_MANAGER_INTERNAL_API_SECRET_CONFIG_KEY: &str =
    "mcp_management.downstream.sandbox_manager_internal_api_secret";
pub const MCP_MANAGEMENT_HOST_CONFIG_KEY: &str = "mcp_management.runtime.host";
pub const MCP_MANAGEMENT_PORT_CONFIG_KEY: &str = "mcp_management.runtime.port";
pub const MCP_MANAGEMENT_INTERNAL_MTLS_PORT_CONFIG_KEY: &str =
    "mcp_management.runtime.internal_mtls_port";
pub const MCP_MANAGEMENT_OTLP_ENDPOINT_CONFIG_KEY: &str =
    "mcp_management.observability.otlp_endpoint";
pub const MCP_MANAGEMENT_OTLP_TRACE_SAMPLE_RATIO_CONFIG_KEY: &str =
    "mcp_management.observability.trace_sample_ratio";
pub const MCP_MANAGEMENT_OTLP_EXPORT_TIMEOUT_MS_CONFIG_KEY: &str =
    "mcp_management.observability.export_timeout_ms";
pub const MCP_MANAGEMENT_DATABASE_URL_CONFIG_KEY: &str = "mcp_management.runtime.database_url";
pub const MCP_MANAGEMENT_RUNTIME_GRANT_SECRET_CONFIG_KEY: &str =
    "mcp_management.runtime.grant_secret";
pub const MCP_MANAGEMENT_RUNTIME_SESSION_ENCRYPTION_SECRET_CONFIG_KEY: &str =
    "mcp_management.runtime.session_encryption_secret";
pub const MCP_MANAGEMENT_EMBEDDED_WORK_DIR_CONFIG_KEY: &str =
    "mcp_management.runtime.embedded_work_dir";
pub const MCP_MANAGEMENT_DOWNSTREAM_REQUEST_TIMEOUT_MS_CONFIG_KEY: &str =
    "mcp_management.runtime.downstream_request_timeout_ms";
pub const MCP_MANAGEMENT_EXTERNAL_HTTP_TOOL_TIMEOUT_MS_CONFIG_KEY: &str =
    "mcp_management.runtime.external_http_tool_timeout_ms";
pub const MCP_MANAGEMENT_RUNTIME_SESSION_TTL_SECONDS_CONFIG_KEY: &str =
    "mcp_management.runtime.session_ttl_seconds";
pub const MCP_MANAGEMENT_RUNTIME_SESSION_CACHE_MAX_ENTRIES_CONFIG_KEY: &str =
    "mcp_management.runtime.session_cache_max_entries";
pub const MCP_MANAGEMENT_RUNTIME_SESSION_CACHE_MAX_BYTES_CONFIG_KEY: &str =
    "mcp_management.runtime.session_cache_max_bytes";
pub const MCP_MANAGEMENT_INVOCATION_QUOTA_VALKEY_URL_CONFIG_KEY: &str =
    "mcp_management.invocation.quota_valkey_url";
pub const MCP_MANAGEMENT_INVOCATION_QUOTA_KEY_PREFIX_CONFIG_KEY: &str =
    "mcp_management.invocation.quota_key_prefix";
pub const MCP_MANAGEMENT_INVOCATION_TENANT_ACTIVE_LIMIT_CONFIG_KEY: &str =
    "mcp_management.invocation.tenant_active_limit";
pub const MCP_MANAGEMENT_INVOCATION_USER_ACTIVE_LIMIT_CONFIG_KEY: &str =
    "mcp_management.invocation.user_active_limit";
pub const MCP_MANAGEMENT_INVOCATION_PROJECT_ACTIVE_LIMIT_CONFIG_KEY: &str =
    "mcp_management.invocation.project_active_limit";
pub const MCP_MANAGEMENT_INVOCATION_DEVICE_ACTIVE_LIMIT_CONFIG_KEY: &str =
    "mcp_management.invocation.device_active_limit";
pub const MCP_MANAGEMENT_SANDBOX_TOOL_TIMEOUT_MS_CONFIG_KEY: &str =
    "mcp_management.runtime.sandbox_tool_timeout_ms";
pub const MCP_MANAGEMENT_SANDBOX_IMAGE_TOOL_TIMEOUT_MS_CONFIG_KEY: &str =
    "mcp_management.runtime.sandbox_image_tool_timeout_ms";
pub const MCP_MANAGEMENT_TASK_RUNNER_TOOL_TIMEOUT_MS_CONFIG_KEY: &str =
    "mcp_management.runtime.task_runner_tool_timeout_ms";
pub const MCP_MANAGEMENT_TASK_RUNNER_ASK_USER_TOOL_TIMEOUT_MS_CONFIG_KEY: &str =
    "mcp_management.runtime.task_runner_ask_user_tool_timeout_ms";
pub const MCP_MANAGEMENT_CHATOS_ASK_USER_TOOL_TIMEOUT_MS_CONFIG_KEY: &str =
    "mcp_management.runtime.chatos_ask_user_tool_timeout_ms";
pub const MCP_MANAGEMENT_CHATOS_BROWSER_TOOL_TIMEOUT_MS_CONFIG_KEY: &str =
    "mcp_management.runtime.chatos_browser_tool_timeout_ms";
pub const MCP_MANAGEMENT_PROVIDER_RESPONSE_LIMIT_BYTES_CONFIG_KEY: &str =
    "mcp_management.runtime.provider_response_limit_bytes";
pub const MCP_MANAGEMENT_PUBLIC_BASE_URL_CONFIG_KEY: &str =
    "mcp_management.runtime.public_base_url";
pub const MCP_MANAGEMENT_PLUGIN_MANAGEMENT_SERVICE_BASE_URL_CONFIG_KEY: &str =
    "mcp_management.downstream.plugin_management_service_base_url";
pub const MCP_MANAGEMENT_PROJECT_SERVICE_BASE_URL_CONFIG_KEY: &str =
    "mcp_management.downstream.project_service_base_url";
pub const MCP_MANAGEMENT_TASK_RUNNER_SERVICE_BASE_URL_CONFIG_KEY: &str =
    "mcp_management.downstream.task_runner_service_base_url";
pub const MCP_MANAGEMENT_CHATOS_SERVICE_BASE_URL_CONFIG_KEY: &str =
    "mcp_management.downstream.chatos_service_base_url";
pub const MCP_MANAGEMENT_LOCAL_CONNECTOR_SERVICE_BASE_URL_CONFIG_KEY: &str =
    "mcp_management.downstream.local_connector_service_base_url";
pub const MCP_MANAGEMENT_SANDBOX_MANAGER_SERVICE_BASE_URL_CONFIG_KEY: &str =
    "mcp_management.downstream.sandbox_manager_service_base_url";
pub const PLUGIN_MANAGEMENT_TASK_RUNNER_INTERNAL_API_SECRET_CONFIG_KEY: &str =
    "plugin_management.downstream.task_runner_internal_api_secret";
pub const PLUGIN_MANAGEMENT_CHATOS_INTERNAL_API_SECRET_CONFIG_KEY: &str =
    "plugin_management.downstream.chatos_internal_api_secret";
pub const PLUGIN_MANAGEMENT_PROJECT_SERVICE_INTERNAL_API_SECRET_CONFIG_KEY: &str =
    "plugin_management.downstream.project_service_internal_api_secret";
pub const PLUGIN_MANAGEMENT_LOCAL_CONNECTOR_INTERNAL_API_SECRET_CONFIG_KEY: &str =
    "plugin_management.downstream.local_connector_internal_api_secret";
pub const PLUGIN_MANAGEMENT_MEMORY_ENGINE_INTERNAL_API_SECRET_CONFIG_KEY: &str =
    "plugin_management.downstream.memory_engine_internal_api_secret";
pub const PLUGIN_MANAGEMENT_MCP_MANAGEMENT_INTERNAL_API_SECRET_CONFIG_KEY: &str =
    "plugin_management.downstream.mcp_management_internal_api_secret";
pub const PLUGIN_MANAGEMENT_CLOUD_CREDENTIAL_ENCRYPTION_SECRET_CONFIG_KEY: &str =
    "plugin_management.security.cloud_credential_encryption_secret";
pub const PLUGIN_MANAGEMENT_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY: &str =
    "plugin_management.security.require_signed_internal_requests";
pub const PLUGIN_MANAGEMENT_SERVICE_USER_SERVICE_BASE_URL_CONFIG_KEY: &str =
    "plugin_management.downstream.user_service_base_url";
pub const PLUGIN_MANAGEMENT_SERVICE_USER_SERVICE_REQUEST_TIMEOUT_MS_CONFIG_KEY: &str =
    "plugin_management.downstream.user_service_request_timeout_ms";
pub const PLUGIN_MANAGEMENT_TASK_RUNNER_BASE_URL_CONFIG_KEY: &str =
    "plugin_management.downstream.task_runner_base_url";
pub const PLUGIN_MANAGEMENT_HOST_CONFIG_KEY: &str = "plugin_management.runtime.host";
pub const PLUGIN_MANAGEMENT_PORT_CONFIG_KEY: &str = "plugin_management.runtime.port";
pub const PLUGIN_MANAGEMENT_INTERNAL_MTLS_PORT_CONFIG_KEY: &str =
    "plugin_management.runtime.internal_mtls_port";
pub const PLUGIN_MANAGEMENT_DATABASE_URL_CONFIG_KEY: &str =
    "plugin_management.runtime.database_url";
pub const PLUGIN_MANAGEMENT_MONGODB_DATABASE_CONFIG_KEY: &str =
    "plugin_management.runtime.mongodb_database";
pub const PLUGIN_MANAGEMENT_CORS_ORIGINS_CONFIG_KEY: &str = "plugin_management.http.cors_origins";
pub const PLUGIN_MANAGEMENT_PUBLIC_BASE_URL_CONFIG_KEY: &str =
    "plugin_management.oauth.public_base_url";
pub const PLUGIN_MANAGEMENT_FRONTEND_ORIGIN_CONFIG_KEY: &str =
    "plugin_management.oauth.frontend_origin";
pub const PLUGIN_MANAGEMENT_OAUTH_FLOW_TTL_SECONDS_CONFIG_KEY: &str =
    "plugin_management.oauth.flow_ttl_seconds";
pub const PLUGIN_MANAGEMENT_OAUTH_REFRESH_SKEW_SECONDS_CONFIG_KEY: &str =
    "plugin_management.oauth.refresh_skew_seconds";
pub const PLUGIN_MANAGEMENT_OAUTH_REQUEST_TIMEOUT_MS_CONFIG_KEY: &str =
    "plugin_management.oauth.request_timeout_ms";
pub const PLUGIN_MANAGEMENT_OAUTH_MAX_RESPONSE_BYTES_CONFIG_KEY: &str =
    "plugin_management.oauth.max_response_bytes";
pub const PLUGIN_MANAGEMENT_LOCAL_CONNECTOR_CHECK_TTL_SECONDS_CONFIG_KEY: &str =
    "plugin_management.local_connector.check_ttl_seconds";
pub const PLUGIN_MANAGEMENT_LOCAL_CONNECTOR_MAX_TOOL_SNAPSHOT_BYTES_CONFIG_KEY: &str =
    "plugin_management.local_connector.max_tool_snapshot_bytes";
pub const PLUGIN_MANAGEMENT_CATALOG_SYNC_ENABLED_CONFIG_KEY: &str =
    "plugin_management.catalog.sync_enabled";
pub const PLUGIN_MANAGEMENT_CATALOG_SYNC_INTERVAL_SECONDS_CONFIG_KEY: &str =
    "plugin_management.catalog.sync_interval_seconds";
pub const PLUGIN_MANAGEMENT_PRESSURE_QUEUE_ELEVATED_MESSAGES_CONFIG_KEY: &str =
    "plugin_management.pressure.queue_elevated_messages";
pub const PLUGIN_MANAGEMENT_PRESSURE_QUEUE_CRITICAL_MESSAGES_CONFIG_KEY: &str =
    "plugin_management.pressure.queue_critical_messages";
pub const PLUGIN_MANAGEMENT_PRESSURE_REPORT_INTERVAL_MS_CONFIG_KEY: &str =
    "plugin_management.pressure.report_interval_ms";
pub const PLUGIN_MANAGEMENT_CATALOG_RABBITMQ_URL_CONFIG_KEY: &str =
    "plugin_management.catalog.rabbitmq_url";
pub const PLUGIN_MANAGEMENT_CATALOG_RABBITMQ_EXCHANGE_CONFIG_KEY: &str =
    "plugin_management.catalog.rabbitmq_exchange";
pub const PLUGIN_MANAGEMENT_CATALOG_QUEUE_CONFIG_KEY: &str = "plugin_management.catalog.queue";
pub const PLUGIN_MANAGEMENT_CATALOG_RETRY_QUEUE_CONFIG_KEY: &str =
    "plugin_management.catalog.retry_queue";
pub const PLUGIN_MANAGEMENT_CATALOG_SCHEDULE_QUEUE_CONFIG_KEY: &str =
    "plugin_management.catalog.schedule_queue";
pub const PLUGIN_MANAGEMENT_CATALOG_DEAD_LETTER_QUEUE_CONFIG_KEY: &str =
    "plugin_management.catalog.dead_letter_queue";
pub const PLUGIN_MANAGEMENT_CATALOG_MAX_DELIVERY_ATTEMPTS_CONFIG_KEY: &str =
    "plugin_management.catalog.max_delivery_attempts";
pub const PLUGIN_MANAGEMENT_CATALOG_RETRY_DELAY_MS_CONFIG_KEY: &str =
    "plugin_management.catalog.retry_delay_ms";
pub const PLUGIN_MANAGEMENT_CATALOG_RABBITMQ_RECONNECT_MS_CONFIG_KEY: &str =
    "plugin_management.catalog.rabbitmq_reconnect_ms";
pub const PLUGIN_MANAGEMENT_CATALOG_CONSUMER_CONCURRENCY_CONFIG_KEY: &str =
    "plugin_management.catalog.consumer_concurrency";
pub const PLUGIN_MANAGEMENT_CATALOG_OUTBOX_RECONCILE_MS_CONFIG_KEY: &str =
    "plugin_management.catalog.outbox_reconcile_ms";
pub const PLUGIN_MANAGEMENT_CATALOG_OUTBOX_BATCH_SIZE_CONFIG_KEY: &str =
    "plugin_management.catalog.outbox_batch_size";
pub const PLUGIN_MANAGEMENT_CATALOG_SYNC_LOCK_TIMEOUT_SECONDS_CONFIG_KEY: &str =
    "plugin_management.catalog.sync_lock_timeout_seconds";
pub const PLUGIN_MANAGEMENT_CATALOG_REQUEST_TIMEOUT_MS_CONFIG_KEY: &str =
    "plugin_management.catalog.request_timeout_ms";
pub const PLUGIN_MANAGEMENT_CATALOG_MAX_BYTES_CONFIG_KEY: &str =
    "plugin_management.catalog.max_bytes";
pub const PLUGIN_MANAGEMENT_SUPER_ADMIN_USERNAME_CONFIG_KEY: &str =
    "plugin_management.bootstrap.super_admin_username";
pub const PLUGIN_MANAGEMENT_SUPER_ADMIN_PASSWORD_CONFIG_KEY: &str =
    "plugin_management.bootstrap.super_admin_password";
pub const PLUGIN_MANAGEMENT_SEED_SYSTEM_RESOURCES_CONFIG_KEY: &str =
    "plugin_management.bootstrap.seed_system_resources";
pub const USER_SERVICE_PORT_CONFIG_KEY: &str = "user_service.runtime.port";
pub const USER_SERVICE_INTERNAL_MTLS_PORT_CONFIG_KEY: &str =
    "user_service.runtime.internal_mtls_port";
pub const USER_SERVICE_OTLP_ENDPOINT_CONFIG_KEY: &str = "user_service.observability.otlp_endpoint";
pub const USER_SERVICE_OTLP_TRACE_SAMPLE_RATIO_CONFIG_KEY: &str =
    "user_service.observability.trace_sample_ratio";
pub const USER_SERVICE_OTLP_EXPORT_TIMEOUT_MS_CONFIG_KEY: &str =
    "user_service.observability.export_timeout_ms";
pub const USER_SERVICE_JWT_SECRET_CONFIG_KEY: &str = "user_service.security.jwt_secret";
pub const USER_SERVICE_SECRET_KEY_CONFIG_KEY: &str = "user_service.security.secret_key";
pub const USER_SERVICE_PREVIOUS_SECRET_KEYS_CONFIG_KEY: &str =
    "user_service.security.previous_secret_keys";
pub const USER_SERVICE_PROJECT_SERVICE_INTERNAL_SECRET_CONFIG_KEY: &str =
    "user_service.security.project_service_internal_secret";
pub const USER_SERVICE_MEMORY_ENGINE_INTERNAL_API_SECRET_CONFIG_KEY: &str =
    "user_service.downstream.memory_engine_internal_api_secret";
pub const USER_SERVICE_SUPER_ADMIN_USERNAME_CONFIG_KEY: &str =
    "user_service.bootstrap.super_admin_username";
pub const USER_SERVICE_SUPER_ADMIN_PASSWORD_CONFIG_KEY: &str =
    "user_service.bootstrap.super_admin_password";
pub const USER_SERVICE_SUPER_ADMIN_DISPLAY_NAME_CONFIG_KEY: &str =
    "user_service.bootstrap.super_admin_display_name";
pub const USER_SERVICE_JWT_ISSUER_CONFIG_KEY: &str = "user_service.auth.jwt_issuer";
pub const USER_SERVICE_USER_AUDIENCE_CONFIG_KEY: &str = "user_service.auth.user_audience";
pub const USER_SERVICE_TASK_RUNNER_AUDIENCE_CONFIG_KEY: &str =
    "user_service.auth.task_runner_audience";
pub const USER_SERVICE_USER_ACCESS_TTL_SECONDS_CONFIG_KEY: &str =
    "user_service.auth.user_access_ttl_seconds";
pub const USER_SERVICE_TASK_RUNNER_ACCESS_TTL_SECONDS_CONFIG_KEY: &str =
    "user_service.auth.task_runner_access_ttl_seconds";
pub const USER_SERVICE_REGISTER_CODE_TTL_SECONDS_CONFIG_KEY: &str =
    "user_service.registration.code_ttl_seconds";
pub const USER_SERVICE_REGISTER_CODE_RESEND_SECONDS_CONFIG_KEY: &str =
    "user_service.registration.code_resend_seconds";
pub const USER_SERVICE_REGISTER_CODE_HOURLY_LIMIT_CONFIG_KEY: &str =
    "user_service.registration.code_hourly_limit";
pub const USER_SERVICE_REGISTER_CODE_MAX_ATTEMPTS_CONFIG_KEY: &str =
    "user_service.registration.code_max_attempts";
pub const USER_SERVICE_LOGIN_MAX_FAILED_ATTEMPTS_CONFIG_KEY: &str =
    "user_service.login.max_failed_attempts";
pub const USER_SERVICE_LOGIN_FAILURE_WINDOW_SECONDS_CONFIG_KEY: &str =
    "user_service.login.failure_window_seconds";
pub const USER_SERVICE_LOGIN_LOCKOUT_SECONDS_CONFIG_KEY: &str =
    "user_service.login.lockout_seconds";
pub const USER_SERVICE_MEMORY_ENGINE_BASE_URL_CONFIG_KEY: &str =
    "user_service.downstream.memory_engine_base_url";
pub const USER_SERVICE_TASK_RUNNER_BASE_URL_CONFIG_KEY: &str =
    "user_service.downstream.task_runner_base_url";
pub const USER_SERVICE_TASK_RUNNER_INTERNAL_API_SECRET_CONFIG_KEY: &str =
    "user_service.downstream.task_runner_internal_api_secret";
pub const USER_SERVICE_DOWNSTREAM_REQUEST_TIMEOUT_MS_CONFIG_KEY: &str =
    "user_service.downstream.request_timeout_ms";
pub const USER_SERVICE_HARNESS_PROVISIONING_ENABLED_CONFIG_KEY: &str =
    "user_service.harness.provisioning_enabled";
pub const USER_SERVICE_HARNESS_BASE_URL_CONFIG_KEY: &str = "user_service.harness.base_url";
pub const USER_SERVICE_HARNESS_SYNTHETIC_EMAIL_DOMAIN_CONFIG_KEY: &str =
    "user_service.harness.synthetic_email_domain";
pub const USER_SERVICE_HARNESS_SPACE_PREFIX_CONFIG_KEY: &str = "user_service.harness.space_prefix";
pub const USER_SERVICE_HARNESS_REQUEST_TIMEOUT_MS_CONFIG_KEY: &str =
    "user_service.harness.request_timeout_ms";
pub const USER_SERVICE_HARNESS_PROJECT_PAT_PREFIX_CONFIG_KEY: &str =
    "user_service.harness.project_pat_prefix";
pub const USER_SERVICE_SMTP_HOST_CONFIG_KEY: &str = "user_service.smtp.host";
pub const USER_SERVICE_SMTP_PORT_CONFIG_KEY: &str = "user_service.smtp.port";
pub const USER_SERVICE_SMTP_USERNAME_CONFIG_KEY: &str = "user_service.smtp.username";
pub const USER_SERVICE_SMTP_PASSWORD_CONFIG_KEY: &str = "user_service.smtp.password";
pub const USER_SERVICE_EMAIL_FROM_CONFIG_KEY: &str = "user_service.smtp.email_from";
pub const USER_SERVICE_EMAIL_FROM_NAME_CONFIG_KEY: &str = "user_service.smtp.email_from_name";
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

pub fn default_task_runner_execution_environment_mode() -> &'static str {
    if cfg!(target_os = "linux") {
        "cloud"
    } else {
        "local"
    }
}
