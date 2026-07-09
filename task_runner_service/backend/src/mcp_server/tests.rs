// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::chatos_async_planner;
use super::support::{
    agent_tool_allowed, create_task_schema, enrich_tool_schemas_with_model_configs,
    filter_model_configs_for_user, model_configs_for_user, normalize_mcp_builtin_kind_names,
    task_mcp_config_schema, update_task_schema,
};
use super::{CreateTaskArgs, McpRequestContext, McpToolProfile, TaskRunnerMcpService};
use crate::ask_user_prompt_service::AskUserPromptService;
use crate::auth::CurrentUser;
use crate::config::{AppConfig, StoreMode};
use crate::models::{
    ChatosSyncedModelConfigRequest, CreateTaskProjectRequest, CreateTaskRequest, ModelConfigRecord,
    TaskMcpConfig, TaskScheduleMode, TaskSourceContext, TaskStatus, UpdateTaskRequest, UserRole,
    PUBLIC_PROJECT_ID, TASK_PROFILE_CHATOS_PLAN, TASK_PROFILE_DEFAULT,
};
use crate::services::{
    ExternalMcpConfigService, McpCatalogService, ModelConfigService, RunService,
    TaskProjectService, TaskService,
};
use crate::store::AppStore;
use serde_json::json;
use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

mod plan_profile;
mod plan_profile_async;
mod plan_profile_scope;
mod schema;
fn valid_planner_create_request() -> CreateTaskRequest {
    CreateTaskRequest {
        title: "task".to_string(),
        description: None,
        objective: "objective".to_string(),
        input_payload: None,
        status: None,
        priority: None,
        tags: None,
        default_model_config_id: Some("model-1".to_string()),
        project_id: None,
        task_profile: None,
        tenant_id: None,
        subject_id: None,
        schedule: None,
        mcp_config: Some(TaskMcpConfig {
            enabled_builtin_kinds: vec!["CodeMaintainerRead".to_string()],
            ..TaskMcpConfig::default()
        }),
        prerequisite_task_ids: None,
    }
}

async fn test_mcp_service() -> (TaskRunnerMcpService, TaskService, TaskProjectService) {
    let config = test_config();
    let store = AppStore::new(&config).await.expect("store");
    let task_service = TaskService::new(config.clone(), store.clone());
    let model_config_service = ModelConfigService::new(store.clone());
    let external_mcp_config_service = ExternalMcpConfigService::new(store.clone());
    let ask_user_prompt_service = AskUserPromptService::new(store.clone());
    let run_service = RunService::new(config, store.clone(), ask_user_prompt_service.clone());
    let mcp_catalog_service =
        McpCatalogService::new(task_service.clone(), ask_user_prompt_service.clone());
    let task_project_service = TaskProjectService::new(store);
    (
        TaskRunnerMcpService::new(
            task_service.clone(),
            model_config_service,
            external_mcp_config_service,
            run_service,
            ask_user_prompt_service,
            mcp_catalog_service,
        ),
        task_service,
        task_project_service,
    )
}

fn test_config() -> AppConfig {
    AppConfig {
        host: IpAddr::V4(Ipv4Addr::LOCALHOST),
        port: 0,
        role: crate::config::TaskRunnerRole::All,
        store_mode: StoreMode::Memory,
        database_url: "memory://mcp-project-scope-test".to_string(),
        memory_engine_base_url: None,
        memory_engine_source_id: "task".to_string(),
        memory_engine_operator_token: None,
        default_tenant_id: "tenant".to_string(),
        default_subject_id: "subject".to_string(),
        default_workspace_dir: ".".to_string(),
        memory_timeout: Duration::from_millis(1000),
        execution_timeout: Duration::from_millis(1000),
        scheduler_poll_interval: Duration::from_millis(1000),
        worker_id: "test-worker".to_string(),
        worker_poll_interval: Duration::from_millis(1_000),
        worker_claim_ttl: Duration::from_millis(120_000),
        worker_concurrency: 4,
        auto_memory_summary: false,
        default_task_execution_max_iterations: 1,
        default_tool_result_model_max_chars: 1000,
        default_tool_results_model_total_max_chars: 2000,
        default_execution_environment_mode: "local".to_string(),
        default_sandbox_manager_base_url: "http://127.0.0.1:8095".to_string(),
        sandbox_manager_client_id: None,
        sandbox_manager_client_key: None,
        default_sandbox_lease_ttl_seconds: 7_200,
        chatos_callback_url: None,
        chatos_callback_secret: None,
        internal_api_secret: None,
        local_connector_internal_api_secret: None,
        callback_timeout: Duration::from_millis(1000),
        admin_username: "admin".to_string(),
        admin_password: "admin".to_string(),
        admin_display_name: "Admin".to_string(),
        user_service_base_url: "http://127.0.0.1:39190".to_string(),
        user_service_request_timeout: Duration::from_millis(5000),
        project_service_base_url: None,
        project_service_sync_secret: None,
        project_service_request_timeout: Duration::from_millis(5000),
    }
}

fn test_create_task_request(title: &str) -> CreateTaskRequest {
    CreateTaskRequest {
        title: title.to_string(),
        description: None,
        objective: format!("do {title}"),
        input_payload: None,
        status: None,
        priority: None,
        tags: None,
        default_model_config_id: None,
        project_id: None,
        task_profile: None,
        tenant_id: None,
        subject_id: None,
        schedule: None,
        mcp_config: None,
        prerequisite_task_ids: None,
    }
}

fn structured_task_ids(value: &serde_json::Value) -> Vec<String> {
    value
        .get("_structured_result")
        .and_then(|value| value.as_array())
        .expect("structured task array")
        .iter()
        .map(|task| {
            task.get("id")
                .and_then(|value| value.as_str())
                .expect("task id")
                .to_string()
        })
        .collect()
}

fn admin_user(owner_user_id: &str) -> CurrentUser {
    CurrentUser {
        id: owner_user_id.to_string(),
        username: format!("{owner_user_id}-name"),
        display_name: format!("{owner_user_id} name"),
        role: UserRole::Admin,
        owner_user_id: Some(owner_user_id.to_string()),
        owner_username: Some(format!("{owner_user_id}-name")),
        owner_display_name: Some(format!("{owner_user_id} name")),
    }
}

fn agent_user(owner_user_id: &str) -> CurrentUser {
    CurrentUser {
        id: format!("agent-{owner_user_id}"),
        username: format!("agent-{owner_user_id}"),
        display_name: format!("Agent {owner_user_id}"),
        role: UserRole::Agent,
        owner_user_id: Some(owner_user_id.to_string()),
        owner_username: Some(format!("{owner_user_id}-name")),
        owner_display_name: Some(format!("{owner_user_id} name")),
    }
}

fn model_config(id: &str, owner_user_id: &str, enabled: bool) -> ModelConfigRecord {
    ModelConfigRecord {
        id: id.to_string(),
        owner_user_id: Some(owner_user_id.to_string()),
        owner_username: Some(format!("{owner_user_id}-name")),
        owner_display_name: Some(format!("{owner_user_id} name")),
        name: id.to_string(),
        provider: "openai".to_string(),
        base_url: "https://api.example.test/v1".to_string(),
        api_key: format!("{id}-key"),
        model: format!("{id}-model"),
        usage_scenario: Some(format!("{id} usage")),
        temperature: None,
        max_output_tokens: None,
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
