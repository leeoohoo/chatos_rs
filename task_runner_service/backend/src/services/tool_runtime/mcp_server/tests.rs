// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::chatos_async_planner;
use super::support::{
    agent_tool_allowed, create_task_schema, enrich_tool_schemas_with_model_configs,
    filter_model_configs_for_user, model_visible_to_user, update_task_schema,
};
use super::{CreateTaskArgs, McpRequestContext, McpToolProfile, TaskRunnerMcpService};
use crate::ask_user_prompt_service::AskUserPromptService;
use crate::auth::CurrentUser;
use crate::config::{AppConfig, StoreMode};
use crate::models::{
    ChatosSyncedModelConfigRequest, CreateTaskRequest, ModelConfigRecord, TaskMcpConfig,
    TaskMcpRequestConfig, TaskScheduleMode, TaskSourceContext, TaskStatus, UpdateTaskRequest,
    UserRole, TASK_PROFILE_DEFAULT,
};
use crate::services::{ModelConfigService, RunService, TaskService};
use crate::store::AppStore;
use serde_json::json;
use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

#[path = "tests/plan_profile.rs"]
mod plan_profile;
#[path = "tests/plan_profile_async.rs"]
mod plan_profile_async;
#[path = "tests/plan_profile_scope.rs"]
mod plan_profile_scope;
#[path = "tests/schema.rs"]
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
        project_context: None,
        task_profile: None,
        tenant_id: None,
        subject_id: None,
        schedule: None,
        plugin_config: Default::default(),
        mcp_config: Some(TaskMcpRequestConfig {
            requires_execution: Some(false),
            workspace_changes_required: None,
            enabled_builtin_kinds: Vec::new(),
            external_mcp_config_ids: Vec::new(),
        }),
        prerequisite_task_ids: None,
    }
}

#[derive(Debug)]
struct ClientProjectSnapshotInput {
    name: String,
    root_path: Option<String>,
    git_url: Option<String>,
    description: Option<String>,
}

#[derive(Debug)]
struct ClientProjectFixture {
    id: String,
}

#[derive(Debug, Clone, Copy)]
struct ClientProjectRegistryFixture;

impl ClientProjectRegistryFixture {
    async fn register_project(
        &self,
        input: ClientProjectSnapshotInput,
        _current_user: &CurrentUser,
    ) -> Result<ClientProjectFixture, String> {
        if input.name.trim().is_empty() {
            return Err("client project name is required".to_string());
        }
        let _client_owned_metadata = (input.root_path, input.git_url, input.description);
        Ok(ClientProjectFixture {
            id: format!("client-project-{}", uuid::Uuid::new_v4()),
        })
    }
}

async fn test_mcp_service() -> (
    TaskRunnerMcpService,
    TaskService,
    ClientProjectRegistryFixture,
) {
    let config = test_config();
    test_mcp_service_with_config(config).await
}

async fn test_mcp_service_with_config(
    config: AppConfig,
) -> (
    TaskRunnerMcpService,
    TaskService,
    ClientProjectRegistryFixture,
) {
    test_mcp_service_with_config_and_policy_mode(config, false).await
}

async fn test_mcp_service_with_config_and_policy_mode(
    config: AppConfig,
    allow_unresolved_plugin_policy: bool,
) -> (
    TaskRunnerMcpService,
    TaskService,
    ClientProjectRegistryFixture,
) {
    let store = AppStore::new(&config).await.expect("store");
    let task_service =
        TaskService::new(config.clone(), store.clone()).with_test_project_authorizer();
    let task_service = if allow_unresolved_plugin_policy {
        task_service.with_unresolved_plugin_policy_for_test()
    } else {
        task_service
    };
    let model_config_service = ModelConfigService::new(store.clone());
    let ask_user_prompt_service = AskUserPromptService::new(store.clone());
    let run_service = RunService::new(config, store.clone(), ask_user_prompt_service.clone())
        .with_test_project_authorizer();
    (
        TaskRunnerMcpService::new(
            task_service.clone(),
            model_config_service,
            run_service,
            ask_user_prompt_service,
        ),
        task_service,
        ClientProjectRegistryFixture,
    )
}

fn test_config() -> AppConfig {
    AppConfig {
        host: IpAddr::V4(Ipv4Addr::LOCALHOST),
        port: 0,
        otlp_endpoint: "http://127.0.0.1:4317".to_string(),
        otlp_trace_sample_ratio: 0.0,
        otlp_export_timeout: Duration::from_secs(1),
        role: crate::config::TaskRunnerRole::All,
        store_mode: StoreMode::Memory,
        database_url: "memory://mcp-project-scope-test".to_string(),
        memory_engine_base_url: None,
        memory_engine_source_id: "task".to_string(),
        memory_engine_operator_token: None,
        memory_engine_http_client: reqwest::Client::new(),
        default_tenant_id: "tenant".to_string(),
        default_subject_id: "subject".to_string(),
        default_workspace_dir: ".".to_string(),
        memory_timeout: Duration::from_millis(1000),
        execution_timeout: Duration::from_millis(1000),
        scheduler_poll_interval: Duration::from_millis(1000),
        worker_id: "test-worker".to_string(),
        worker_claim_ttl: Duration::from_millis(120_000),
        worker_concurrency: 4,
        auto_memory_summary: false,
        default_task_execution_max_iterations: 1,
        default_tool_result_model_max_chars: 1000,
        default_tool_results_model_total_max_chars: 2000,
        chatos_callback_url: String::new(),
        chatos_callback_http_client: reqwest::Client::new(),
        chatos_internal_api_secret: None,
        mcp_management_internal_api_secret: None,
        user_service_internal_api_secret: None,
        callback_timeout: Duration::from_millis(1000),
        admin_username: "admin".to_string(),
        admin_password: "admin".to_string(),
        admin_display_name: "Admin".to_string(),
        user_service_base_url: "http://127.0.0.1:39190".to_string(),
        user_service_request_timeout: Duration::from_millis(5000),
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
        project_context: None,
        task_profile: None,
        tenant_id: None,
        subject_id: None,
        schedule: None,
        plugin_config: Default::default(),
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
        prompt_vendor: Some("gpt".to_string()),
        base_url: "https://api.example.test/v1".to_string(),
        api_key: format!("{id}-key"),
        model: format!("{id}-model"),
        usage_scenario: Some(format!("{id} usage")),
        temperature: None,
        max_output_tokens: None,
        model_request_max_retries: 5,
        thinking_level: None,
        supports_images: false,
        supports_reasoning: false,
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

#[test]
fn administrator_can_use_cloud_models_owned_by_another_user() {
    let model = model_config("shared-model", "model-owner", true);
    assert!(model_visible_to_user(&model, &admin_user("administrator")));
    assert!(!model_visible_to_user(
        &model,
        &agent_user("different-owner")
    ));
    assert!(model_visible_to_user(&model, &agent_user("model-owner")));
}
