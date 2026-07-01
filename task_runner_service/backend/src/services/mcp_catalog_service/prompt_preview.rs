// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl McpCatalogService {
    pub async fn preview_task_prompt(
        &self,
        task_id: &str,
    ) -> Result<Option<McpPromptPreviewResponse>, String> {
        let Some(task) = self.task_service.get_task(task_id).await? else {
            return Ok(None);
        };

        let enabled_builtin_kinds = runtime_selected_builtin_kinds(&task)
            .into_iter()
            .map(|kind| kind.kind_name().to_string())
            .collect();

        self.preview_prompt(McpPromptPreviewRequest {
            enabled: Some(task.mcp_config.enabled),
            init_mode: Some(task.mcp_config.init_mode),
            builtin_prompt_mode: Some(task.mcp_config.builtin_prompt_mode),
            builtin_prompt_locale: Some(task.mcp_config.builtin_prompt_locale),
            enabled_builtin_kinds: Some(enabled_builtin_kinds),
            workspace_dir: task.mcp_config.workspace_dir,
            default_remote_server_id: task.mcp_config.default_remote_server_id,
        })
        .map(Some)
    }

    pub fn preview_prompt(
        &self,
        request: McpPromptPreviewRequest,
    ) -> Result<McpPromptPreviewResponse, String> {
        let enabled = request.enabled.unwrap_or(true);
        let init_mode = if enabled {
            chatos_ai_runtime::TaskMcpInitMode::Full
        } else {
            chatos_ai_runtime::TaskMcpInitMode::Disabled
        };
        let builtin_prompt_mode = request
            .builtin_prompt_mode
            .unwrap_or(TaskBuiltinMcpPromptMode::Effective);
        let builtin_prompt_locale = request
            .builtin_prompt_locale
            .clone()
            .unwrap_or_else(|| BuiltinMcpPromptLocale::DEFAULT_KEY.to_string());
        let selected_kind_names = request.enabled_builtin_kinds.unwrap_or_else(|| {
            default_runtime_builtin_kinds()
                .into_iter()
                .map(|kind| kind.kind_name().to_string())
                .collect()
        });

        let mcp_config = TaskMcpConfig {
            enabled,
            init_mode,
            builtin_prompt_mode,
            builtin_prompt_locale: builtin_prompt_locale.clone(),
            enabled_builtin_kinds: selected_kind_names,
            workspace_dir: normalized_optional(request.workspace_dir),
            default_remote_server_id: normalized_optional(request.default_remote_server_id),
            external_mcp_config_ids: Vec::new(),
            skill_ids: Vec::new(),
        };
        let selected_builtin_kinds = if enabled {
            selected_builtin_kinds(&mcp_config)
        } else {
            Vec::new()
        };

        let mut server_options = BuiltinMcpServerOptions::new(resolve_workspace_dir_with_base(
            self.task_service.config.default_workspace_dir.as_str(),
            mcp_config.workspace_dir.as_deref(),
        ))
        .with_auto_create_task(true);
        if let Some(remote_server_id) = mcp_config.default_remote_server_id.clone() {
            server_options = server_options.with_remote_connection_id(remote_server_id);
        }
        let builtin_servers =
            builtin_servers_from_kinds(selected_builtin_kinds.clone(), &server_options);
        let (builtin_registry, _) = build_builtin_registry(
            &builtin_servers,
            self.task_service.clone(),
            self.ask_user_prompt_service.clone(),
        );
        let executor = McpExecutorBuilder::new()
            .with_builtin_servers(builtin_servers)
            .with_builtin_registry(builtin_registry)
            .build_builtin_only()?;
        let locale = BuiltinMcpPromptLocale::from_key(Some(&builtin_prompt_locale));
        let build = match builtin_prompt_mode {
            TaskBuiltinMcpPromptMode::Configured => {
                executor.inspect_builtin_mcp_system_prompt(locale)
            }
            TaskBuiltinMcpPromptMode::Effective => {
                executor.inspect_effective_builtin_mcp_system_prompt(locale)
            }
        };

        Ok(McpPromptPreviewResponse {
            enabled,
            init_mode,
            builtin_prompt_mode,
            builtin_prompt_locale,
            selected_builtin_kinds: selected_builtin_kinds
                .into_iter()
                .map(|kind| kind.kind_name().to_string())
                .collect(),
            build,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ask_user_prompt_service::AskUserPromptService;
    use crate::config::{AppConfig, StoreMode};
    use crate::store::AppStore;
    use std::net::{IpAddr, Ipv4Addr};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(name: &str) -> String {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|value| value.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir()
            .join(format!(
                "task-runner-mcp-prompt-preview-{name}-{}-{unique}",
                std::process::id()
            ))
            .to_string_lossy()
            .to_string()
    }

    fn test_config(default_workspace_dir: String) -> AppConfig {
        AppConfig {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 0,
            store_mode: StoreMode::Memory,
            database_url: "memory://task_runner_mcp_prompt_preview_test".to_string(),
            memory_engine_base_url: None,
            memory_engine_source_id: "task".to_string(),
            memory_engine_operator_token: None,
            default_tenant_id: "tenant".to_string(),
            default_subject_id: "subject".to_string(),
            default_workspace_dir,
            memory_timeout: Duration::from_millis(1_000),
            execution_timeout: Duration::from_millis(1_000),
            scheduler_poll_interval: Duration::from_millis(1_000),
            auto_memory_summary: false,
            default_task_execution_max_iterations: 1,
            default_tool_result_model_max_chars: 1_000,
            default_tool_results_model_total_max_chars: 2_000,
            default_execution_environment_mode: "local".to_string(),
            default_sandbox_manager_base_url: "http://127.0.0.1:8095".to_string(),
            default_sandbox_lease_ttl_seconds: 7_200,
            chatos_callback_url: None,
            chatos_callback_secret: None,
            internal_api_secret: None,
            callback_timeout: Duration::from_millis(1_000),
            admin_username: "admin".to_string(),
            admin_password: "admin".to_string(),
            admin_display_name: "Admin".to_string(),
            user_service_base_url: "http://127.0.0.1:39190".to_string(),
            user_service_request_timeout: Duration::from_millis(5_000),
            project_service_base_url: None,
            project_service_sync_secret: None,
            project_service_request_timeout: Duration::from_millis(5000),
        }
    }

    async fn test_service() -> McpCatalogService {
        let workspace_dir = unique_temp_dir("workspace");
        std::fs::create_dir_all(&workspace_dir).expect("create workspace");
        let config = test_config(workspace_dir);
        let store = AppStore::new(&config).await.expect("store");
        McpCatalogService::new(
            TaskService::new(config, store.clone()),
            AskUserPromptService::new(store),
        )
    }

    #[tokio::test]
    async fn preview_prompt_uses_runtime_defaults_when_kinds_are_omitted() {
        let service = test_service().await;

        let preview = service
            .preview_prompt(McpPromptPreviewRequest {
                enabled: Some(true),
                init_mode: Some(chatos_ai_runtime::TaskMcpInitMode::Full),
                builtin_prompt_mode: Some(TaskBuiltinMcpPromptMode::Effective),
                builtin_prompt_locale: Some(BuiltinMcpPromptLocale::DEFAULT_KEY.to_string()),
                enabled_builtin_kinds: None,
                workspace_dir: None,
                default_remote_server_id: None,
            })
            .expect("preview prompt");

        assert!(preview
            .selected_builtin_kinds
            .contains(&"TaskManager".to_string()));
        assert!(preview
            .selected_builtin_kinds
            .contains(&"AskUser".to_string()));
        let prompt = preview.build.prompt.expect("builtin prompt");
        assert!(prompt.contains("内置 MCP 优先"));
        assert!(prompt.contains("`task_manager_add_task`"));
        assert!(prompt.contains("`ask_user_prompt_choices`"));
    }
}
