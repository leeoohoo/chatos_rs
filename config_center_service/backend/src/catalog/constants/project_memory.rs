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
