// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;

use crate::models::{
    now_rfc3339, TaskRunnerQueueStatsSnapshot, TaskRunnerRunStatsSnapshot,
    TaskRunnerRuntimeStatsSnapshot, TaskRunnerSystemStatsResponse,
};
use serde::Serialize;

use super::internal_auth::{
    require_task_runner_internal_request, EXECUTION_OPTIONS_READ_SCOPE, MCP_MANAGEMENT_CALLER,
    PROJECT_SERVICE_CALLER, SYSTEM_STATS_READ_SCOPE,
};
use super::*;

#[derive(Debug, Serialize)]
pub(super) struct InternalExecutionOptionsResponse {
    pub model_config_ids: Vec<String>,
}

pub(super) async fn get_user_execution_options(
    Path(owner_user_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<InternalExecutionOptionsResponse>, ApiError> {
    require_task_runner_internal_request(
        &state.config,
        &headers,
        &[PROJECT_SERVICE_CALLER],
        EXECUTION_OPTIONS_READ_SCOPE,
    )
    .map_err(|err| ApiError {
        status: err.status,
        message: err.message,
    })?;
    let owner_user_id = owner_user_id.trim();
    if owner_user_id.is_empty() {
        return Err(ApiError::bad_request("owner_user_id is required"));
    }

    let model_config_ids = state
        .model_config_service
        .list_model_configs()
        .await
        .map_err(ApiError::bad_request)?
        .into_iter()
        .filter(|model| model.enabled)
        .filter(|model| owns_resource(model.owner_user_id.as_deref(), owner_user_id))
        .map(|model| model.id)
        .collect::<BTreeSet<_>>();

    Ok(Json(InternalExecutionOptionsResponse {
        model_config_ids: model_config_ids.into_iter().collect(),
    }))
}

pub(super) async fn get_system_stats(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<TaskRunnerSystemStatsResponse>, ApiError> {
    require_task_runner_internal_request(
        &state.config,
        &headers,
        &[PROJECT_SERVICE_CALLER, MCP_MANAGEMENT_CALLER],
        SYSTEM_STATS_READ_SCOPE,
    )
    .map_err(|err| ApiError {
        status: err.status,
        message: err.message,
    })?;
    let run_stats = state
        .run_service
        .execution_stats()
        .await
        .map_err(ApiError::internal)?;
    let sse_ticket_stats = state.sse_tickets.stats();
    Ok(Json(TaskRunnerSystemStatsResponse {
        ok: true,
        service: "task_runner_service_backend",
        now: now_rfc3339(),
        runtime: TaskRunnerRuntimeStatsSnapshot {
            worker_claim_failures_total: state.runtime_stats.worker_claim_failures_total(),
            active_run_event_streams: state.runtime_stats.active_run_event_streams(),
            pending_sse_tickets: sse_ticket_stats.active_ticket_count,
        },
        queue: TaskRunnerQueueStatsSnapshot {
            rabbitmq_enabled: state.task_queue_topology.uses_rabbitmq(),
            run_dispatch_mode: state
                .task_queue_topology
                .run_dispatch_mode
                .as_str()
                .to_string(),
            callback_delivery_mode: state
                .task_queue_topology
                .callback_delivery_mode
                .as_str()
                .to_string(),
            run_events_publish_mode: state
                .task_queue_topology
                .run_events_publish_mode
                .as_str()
                .to_string(),
            rabbitmq_exchange: state.task_queue_topology.rabbitmq_exchange.clone(),
            run_dispatch_queue: state.task_queue_topology.run_dispatch_queue.clone(),
            callback_delivery_queue: state.task_queue_topology.callback_delivery_queue.clone(),
            run_events_queue: state.task_queue_topology.run_events_queue.clone(),
        },
        runs: TaskRunnerRunStatsSnapshot {
            total: run_stats.total,
            active: run_stats.active,
            queued: run_stats.queued,
            running: run_stats.running,
            succeeded: run_stats.succeeded,
            failed: run_stats.failed,
            cancelled: run_stats.cancelled,
            blocked: run_stats.blocked,
            dispatch_paused: run_stats.dispatch_paused,
            callback_pending: run_stats.callback_pending,
            callback_enqueued: run_stats.callback_enqueued,
        },
    }))
}

fn owns_resource(owner_user_id: Option<&str>, expected_owner_user_id: &str) -> bool {
    owner_user_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        == Some(expected_owner_user_id)
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};
    use std::time::Duration;

    use axum::http::HeaderValue;

    use super::*;
    use crate::ask_user_prompt_service::AskUserPromptService;
    use crate::auth::AuthService;
    use crate::config::{AppConfig, StoreMode};
    use crate::mcp_server::TaskRunnerMcpService;
    use crate::models::ModelConfigRecord;
    use crate::services::{
        McpCatalogService, ModelConfigService, RemoteServerService, RunService, TaskProjectService,
        TaskService, ToolingStateService,
    };
    use crate::store::AppStore;

    #[tokio::test]
    async fn user_execution_options_filters_owner_scoped_configs() {
        let state = test_state().await;
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-task-runner-internal-secret",
            HeaderValue::from_static("internal-secret"),
        );

        let Json(response) =
            get_user_execution_options(Path("owner-1".to_string()), State(state), headers)
                .await
                .expect("execution options");

        assert_eq!(response.model_config_ids, vec!["model-owner"]);
    }

    #[tokio::test]
    async fn user_execution_options_requires_internal_secret() {
        let state = test_state().await;

        let err =
            get_user_execution_options(Path("owner-1".to_string()), State(state), HeaderMap::new())
                .await
                .expect_err("missing secret should fail");

        assert_eq!(err.status, StatusCode::UNAUTHORIZED);
        assert_eq!(err.message, "missing task runner internal api secret");
    }

    #[tokio::test]
    async fn user_execution_options_accepts_project_service_scoped_token() {
        let state = test_state().await;
        let token = chatos_service_runtime::issue_internal_service_token(
            "internal-secret",
            PROJECT_SERVICE_CALLER,
            super::super::internal_auth::TASK_RUNNER_TOKEN_AUDIENCE,
            EXECUTION_OPTIONS_READ_SCOPE,
            60,
        )
        .expect("issue token");
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-task-runner-caller",
            HeaderValue::from_static(PROJECT_SERVICE_CALLER),
        );
        headers.insert(
            "x-task-runner-internal-token",
            HeaderValue::from_str(token.as_str()).expect("token header"),
        );

        let _ = get_user_execution_options(Path("owner-1".to_string()), State(state), headers)
            .await
            .expect("signed execution options request");
    }

    #[tokio::test]
    async fn system_stats_returns_queue_run_and_runtime_counters() {
        let state = test_state().await;
        let queued_run = crate::models::TaskRunRecord::queued(
            "run-queued".to_string(),
            "task-queued".to_string(),
            "model-owner".to_string(),
            "thread-queued".to_string(),
            serde_json::json!({}),
            Vec::new(),
            now_rfc3339(),
        );
        state
            .run_service
            .store()
            .save_run(queued_run)
            .await
            .expect("save queued run");
        state.sse_tickets.issue("test-access-token");
        state.runtime_stats.record_worker_claim_failure();
        let token = chatos_service_runtime::issue_internal_service_token(
            "internal-secret",
            MCP_MANAGEMENT_CALLER,
            super::super::internal_auth::TASK_RUNNER_TOKEN_AUDIENCE,
            SYSTEM_STATS_READ_SCOPE,
            60,
        )
        .expect("issue token");
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-task-runner-caller",
            HeaderValue::from_static(MCP_MANAGEMENT_CALLER),
        );
        headers.insert(
            "x-task-runner-internal-token",
            HeaderValue::from_str(token.as_str()).expect("token header"),
        );

        let Json(response) = get_system_stats(State(state), headers)
            .await
            .expect("system stats");

        assert!(response.ok);
        assert_eq!(response.runtime.worker_claim_failures_total, 1);
        assert_eq!(response.runtime.pending_sse_tickets, 1);
        assert_eq!(response.runs.total, 1);
        assert_eq!(response.runs.active, 1);
        assert_eq!(response.runs.queued, 1);
        assert_eq!(response.queue.run_dispatch_mode, "inline");
    }

    #[test]
    fn chatos_internal_auth_uses_dedicated_secret_and_scope() {
        let config = test_config();
        let token = chatos_service_runtime::issue_internal_service_token(
            "chatos-internal-secret",
            super::super::internal_auth::CHATOS_CALLER,
            super::super::internal_auth::TASK_RUNNER_TOKEN_AUDIENCE,
            super::super::internal_auth::CHATOS_MESSAGES_READ_SCOPE,
            60,
        )
        .expect("issue token");
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-task-runner-caller",
            HeaderValue::from_static(super::super::internal_auth::CHATOS_CALLER),
        );
        headers.insert(
            "x-task-runner-internal-token",
            HeaderValue::from_str(token.as_str()).expect("token header"),
        );

        super::super::internal_auth::require_task_runner_internal_request(
            &config,
            &headers,
            &[super::super::internal_auth::CHATOS_CALLER],
            super::super::internal_auth::CHATOS_MESSAGES_READ_SCOPE,
        )
        .expect("chatos signed request");
        let err = super::super::internal_auth::require_task_runner_internal_request(
            &config,
            &headers,
            &[super::super::internal_auth::CHATOS_CALLER],
            EXECUTION_OPTIONS_READ_SCOPE,
        )
        .expect_err("scope mismatch must fail");
        assert_eq!(err.message, "invalid task runner internal API token");
    }

    async fn test_state() -> AppState {
        let config = test_config();
        let store = AppStore::new(&config).await.expect("store");
        store
            .save_model_config(model_config("model-owner", Some("owner-1"), true))
            .await
            .expect("save owner model");
        store
            .save_model_config(model_config("model-other", Some("owner-2"), true))
            .await
            .expect("save other model");
        store
            .save_model_config(model_config("model-disabled", Some("owner-1"), false))
            .await
            .expect("save disabled model");
        let auth_service = AuthService::new(config.clone(), store.clone());
        let task_service = TaskService::new(config.clone(), store.clone());
        let model_config_service = ModelConfigService::new(store.clone());
        let remote_server_service = RemoteServerService::new(store.clone());
        let task_project_service = TaskProjectService::new(store.clone());
        let ask_user_prompt_service = AskUserPromptService::new(store.clone());
        let run_service = RunService::new(
            config.clone(),
            store.clone(),
            ask_user_prompt_service.clone(),
        );
        let mcp_catalog_service =
            McpCatalogService::new(task_service.clone(), ask_user_prompt_service.clone());
        let tooling_state_service = ToolingStateService::new(config.clone());
        let task_runner_mcp_service = TaskRunnerMcpService::new(
            task_service.clone(),
            model_config_service.clone(),
            run_service.clone(),
            ask_user_prompt_service.clone(),
        );
        let task_queue_topology = crate::platform_queue::TaskQueueTopology::inline_defaults();

        AppState {
            config,
            task_queue_topology,
            task_service,
            model_config_service,
            remote_server_service,
            task_project_service,
            run_service,
            ask_user_prompt_service,
            mcp_catalog_service,
            tooling_state_service,
            task_runner_mcp_service,
            auth_service,
            sse_tickets: crate::auth::SseTicketStore::default(),
            runtime_stats: crate::state::TaskRunnerRuntimeStats::default(),
        }
    }

    fn test_config() -> AppConfig {
        let default_workspace_dir = std::env::temp_dir()
            .join("chatos-task-runner-internal-options-test")
            .to_string_lossy()
            .into_owned();
        AppConfig {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 0,
            role: crate::config::TaskRunnerRole::All,
            store_mode: StoreMode::Memory,
            database_url: "memory://internal-execution-options-test".to_string(),
            memory_engine_base_url: None,
            memory_engine_source_id: "task".to_string(),
            memory_engine_operator_token: None,
            default_tenant_id: "tenant".to_string(),
            default_subject_id: "subject".to_string(),
            default_workspace_dir,
            memory_timeout: Duration::from_millis(1_000),
            execution_timeout: Duration::from_millis(1_000),
            scheduler_poll_interval: Duration::from_millis(1_000),
            worker_id: "test-worker".to_string(),
            worker_poll_interval: Duration::from_millis(1_000),
            worker_claim_ttl: Duration::from_millis(120_000),
            worker_concurrency: 4,
            auto_memory_summary: false,
            default_task_execution_max_iterations: 1,
            default_tool_result_model_max_chars: 1_000,
            default_tool_results_model_total_max_chars: 2_000,
            default_execution_environment_mode: "local".to_string(),
            default_sandbox_manager_base_url: "http://127.0.0.1:8095".to_string(),
            sandbox_manager_client_id: None,
            sandbox_manager_client_key: None,
            default_sandbox_lease_ttl_seconds: 7_200,
            chatos_callback_url: None,
            chatos_callback_secret: None,
            internal_api_secret: Some("internal-secret".to_string()),
            chatos_internal_api_secret: Some("chatos-internal-secret".to_string()),
            mcp_management_internal_api_secret: Some("internal-secret".to_string()),
            local_connector_internal_api_secret: None,
            local_connector_service_base_url: Some("http://127.0.0.1:39230".to_string()),
            local_connector_service_request_timeout: Duration::from_millis(5_000),
            plugin_relay_request_timeout: Duration::from_millis(60_000),
            plugin_hook_relay_timeout: Duration::from_millis(330_000),
            plugin_connector_discovery_timeout: Duration::from_millis(10_000),
            callback_timeout: Duration::from_millis(1_000),
            admin_username: "admin".to_string(),
            admin_password: "admin".to_string(),
            admin_display_name: "Admin".to_string(),
            user_service_base_url: "http://127.0.0.1:39190".to_string(),
            user_service_request_timeout: Duration::from_millis(5_000),
            project_service_base_url: None,
            project_service_sync_secret: None,
            project_service_request_timeout: Duration::from_millis(5_000),
        }
    }

    fn model_config(id: &str, owner_user_id: Option<&str>, enabled: bool) -> ModelConfigRecord {
        ModelConfigRecord {
            id: id.to_string(),
            owner_user_id: owner_user_id.map(ToOwned::to_owned),
            owner_username: None,
            owner_display_name: None,
            name: id.to_string(),
            provider: "openai".to_string(),
            prompt_vendor: Some("gpt".to_string()),
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: "secret".to_string(),
            model: "gpt-test".to_string(),
            usage_scenario: None,
            temperature: None,
            max_output_tokens: None,
            model_request_max_retries: 5,
            thinking_level: None,
            supports_responses: true,
            instructions: None,
            request_cwd: None,
            include_prompt_cache_retention: false,
            request_body_limit_bytes: None,
            enabled,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }
}
