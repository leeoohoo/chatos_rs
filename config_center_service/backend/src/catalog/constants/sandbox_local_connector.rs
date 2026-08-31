pub const LOCAL_CONNECTOR_REQUIRE_SIGNED_INTERNAL_REQUESTS_CONFIG_KEY: &str =
    "local_connector.security.require_signed_internal_requests";
pub const LOCAL_CONNECTOR_CHATOS_INTERNAL_API_SECRET_CONFIG_KEY: &str =
    "local_connector.security.chatos_internal_api_secret";
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
pub const LOCAL_CONNECTOR_CONTROLLED_NETWORK_POLICY_TTL_SECONDS_CONFIG_KEY: &str =
    "local_connector.controlled_network.policy_ttl_seconds";
pub const LOCAL_CONNECTOR_CONTROLLED_NETWORK_SIGNING_KEY_PATH_CONFIG_KEY: &str =
    "local_connector.controlled_network.signing_key_path";
pub const LOCAL_CONNECTOR_CONTROLLED_NETWORK_SIGNING_KEY_ID_CONFIG_KEY: &str =
    "local_connector.controlled_network.signing_key_id";
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
