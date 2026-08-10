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
