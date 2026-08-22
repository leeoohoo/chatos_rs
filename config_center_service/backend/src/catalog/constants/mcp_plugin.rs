pub const MCP_MANAGEMENT_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY: &str =
    "mcp_management.security.require_signed_internal_requests";
pub const MCP_MANAGEMENT_ASYNC_TOOL_DISPATCH_MODE_CONFIG_KEY: &str =
    "mcp_management.async_tool.dispatch_mode";
pub const MCP_MANAGEMENT_ASYNC_TOOL_WORKER_CONCURRENCY_CONFIG_KEY: &str =
    "mcp_management.async_tool.worker_concurrency";
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
pub const MCP_MANAGEMENT_PROJECT_SERVICE_TOOL_TIMEOUT_MS_CONFIG_KEY: &str =
    "mcp_management.runtime.project_service_tool_timeout_ms";
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
pub const PLUGIN_MANAGEMENT_ARTIFACT_STORAGE_DIR_CONFIG_KEY: &str =
    "plugin_management.artifact.storage_dir";
pub const PLUGIN_MANAGEMENT_ARTIFACT_PUBLIC_BASE_URL_CONFIG_KEY: &str =
    "plugin_management.artifact.public_base_url";
pub const PLUGIN_MANAGEMENT_ARTIFACT_MAX_BYTES_CONFIG_KEY: &str =
    "plugin_management.artifact.max_bytes";
pub const PLUGIN_MANAGEMENT_SUPER_ADMIN_USERNAME_CONFIG_KEY: &str =
    "plugin_management.bootstrap.super_admin_username";
pub const PLUGIN_MANAGEMENT_SUPER_ADMIN_PASSWORD_CONFIG_KEY: &str =
    "plugin_management.bootstrap.super_admin_password";
pub const PLUGIN_MANAGEMENT_SEED_SYSTEM_RESOURCES_CONFIG_KEY: &str =
    "plugin_management.bootstrap.seed_system_resources";
