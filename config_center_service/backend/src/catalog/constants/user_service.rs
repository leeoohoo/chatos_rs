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
pub const USER_SERVICE_CHATOS_INTERNAL_SECRET_CONFIG_KEY: &str =
    "user_service.security.chatos_internal_secret";
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
