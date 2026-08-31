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
pub const CHATOS_USER_SERVICE_BASE_URL_CONFIG_KEY: &str = "chatos.downstream.user_service_base_url";
pub const CHATOS_USER_SERVICE_INTERNAL_BASE_URL_CONFIG_KEY: &str =
    "chatos.downstream.user_service_internal_base_url";
pub const CHATOS_USER_SERVICE_INTERNAL_API_SECRET_CONFIG_KEY: &str =
    "chatos.security.user_service_internal_api_secret";
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
