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
