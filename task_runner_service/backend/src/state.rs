use crate::auth::AuthService;
use crate::config::AppConfig;
use crate::mcp_server::TaskRunnerMcpService;
use crate::services::{
    McpCatalogService, ModelConfigService, RemoteServerService, RunService, TaskService,
    ToolingStateService,
};
use crate::store::AppStore;
use crate::ui_prompt_service::UiPromptService;
use tracing::{info, warn};

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub task_service: TaskService,
    pub model_config_service: ModelConfigService,
    pub remote_server_service: RemoteServerService,
    pub run_service: RunService,
    pub ui_prompt_service: UiPromptService,
    pub mcp_catalog_service: McpCatalogService,
    pub tooling_state_service: ToolingStateService,
    pub task_runner_mcp_service: TaskRunnerMcpService,
    pub auth_service: AuthService,
}

impl AppState {
    pub async fn new(config: AppConfig) -> Result<Self, String> {
        let store = AppStore::new(&config).await?;
        let auth_service = AuthService::new(store.clone());
        auth_service.ensure_default_admin(&config).await?;
        let task_service = TaskService::new(config.clone(), store.clone());
        let model_config_service = ModelConfigService::new(store.clone());
        let remote_server_service = RemoteServerService::new(store.clone());
        let ui_prompt_service = UiPromptService::new(store.clone());
        let run_service = RunService::new(config.clone(), store.clone(), ui_prompt_service.clone());
        match run_service.recover_incomplete_runs().await {
            Ok(count) if count > 0 => {
                info!("recovered {} incomplete task runs during startup", count);
            }
            Ok(_) => {}
            Err(err) => {
                warn!(
                    "failed to recover incomplete task runs during startup: {}",
                    err
                );
            }
        }
        let mcp_catalog_service =
            McpCatalogService::new(task_service.clone(), ui_prompt_service.clone());
        let tooling_state_service = ToolingStateService::new(config.clone());
        let task_runner_mcp_service = TaskRunnerMcpService::new(
            task_service.clone(),
            model_config_service.clone(),
            run_service.clone(),
            ui_prompt_service.clone(),
            mcp_catalog_service.clone(),
        );
        Ok(Self {
            config,
            task_service,
            model_config_service,
            remote_server_service,
            run_service,
            ui_prompt_service,
            mcp_catalog_service,
            tooling_state_service,
            task_runner_mcp_service,
            auth_service,
        })
    }
}
