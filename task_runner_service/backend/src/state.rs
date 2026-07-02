// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::ask_user_prompt_service::AskUserPromptService;
use crate::auth::AuthService;
use crate::config::AppConfig;
use crate::mcp_server::TaskRunnerMcpService;
use crate::services::{
    ExternalMcpConfigService, McpCatalogService, ModelConfigService, RemoteServerService,
    RunService, SkillService, TaskProjectService, TaskService, ToolingStateService,
};
use crate::store::AppStore;
use memory_engine_sdk::UpsertSourceRequest;
use serde_json::json;
use tracing::{info, warn};

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub task_service: TaskService,
    pub model_config_service: ModelConfigService,
    pub remote_server_service: RemoteServerService,
    pub external_mcp_config_service: ExternalMcpConfigService,
    pub skill_service: SkillService,
    pub task_project_service: TaskProjectService,
    pub run_service: RunService,
    pub ask_user_prompt_service: AskUserPromptService,
    pub mcp_catalog_service: McpCatalogService,
    pub tooling_state_service: ToolingStateService,
    pub task_runner_mcp_service: TaskRunnerMcpService,
    pub auth_service: AuthService,
}

impl AppState {
    pub async fn new(config: AppConfig) -> Result<Self, String> {
        ensure_task_runner_memory_engine_source(&config).await?;
        let store = AppStore::new(&config).await?;
        let auth_service = AuthService::new(config.clone(), store.clone());
        auth_service.ensure_default_admin(&config).await?;
        let task_service = TaskService::new(config.clone(), store.clone());
        let model_config_service = ModelConfigService::new(store.clone());
        let task_project_service =
            TaskProjectService::new_with_config(store.clone(), config.clone());
        task_project_service.ensure_public_project().await?;
        let remote_server_service = RemoteServerService::new(store.clone());
        let external_mcp_config_service = ExternalMcpConfigService::new(store.clone());
        let skill_service = SkillService::new(&config, store.clone());
        match skill_service.sync_bundled_skills().await {
            Ok(count) if count > 0 => {
                info!("synced {} bundled skills during startup", count);
            }
            Ok(_) => {}
            Err(err) => {
                warn!("failed to sync bundled skills during startup: {}", err);
            }
        }
        let ask_user_prompt_service =
            AskUserPromptService::new_with_config(store.clone(), config.clone());
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
            external_mcp_config_service.clone(),
            skill_service.clone(),
            run_service.clone(),
            ask_user_prompt_service.clone(),
            mcp_catalog_service.clone(),
        );
        Ok(Self {
            config,
            task_service,
            model_config_service,
            remote_server_service,
            external_mcp_config_service,
            skill_service,
            task_project_service,
            run_service,
            ask_user_prompt_service,
            mcp_catalog_service,
            tooling_state_service,
            task_runner_mcp_service,
            auth_service,
        })
    }
}

async fn ensure_task_runner_memory_engine_source(config: &AppConfig) -> Result<(), String> {
    let Some(client) = config.memory_client()? else {
        return Ok(());
    };
    let source_id = config.memory_engine_source_id.trim();
    if source_id.is_empty() {
        return Ok(());
    }
    client
        .upsert_source(
            source_id,
            &UpsertSourceRequest {
                tenant_id: None,
                source_type: "task_runner".to_string(),
                name: "Task Runner".to_string(),
                description: Some(
                    "Task Runner managed source for task threads, run records, summaries, and subject memories."
                        .to_string(),
                ),
                config: Some(json!({
                    "platform_managed": true,
                    "owner_service": "task_runner_service_backend",
                    "mapping_version": "task_runner.v1",
                    "capabilities": [
                        "threads",
                        "records",
                        "summaries",
                        "subject_memories"
                    ],
                })),
                sdk_enabled: Some(true),
                status: Some("active".to_string()),
            },
        )
        .await?;
    Ok(())
}
