use super::*;

impl McpCatalogService {
    pub async fn preview_task_prompt(
        &self,
        task_id: &str,
    ) -> Result<Option<McpPromptPreviewResponse>, String> {
        let Some(task) = self.task_service.get_task(task_id).await? else {
            return Ok(None);
        };

        self.preview_prompt(McpPromptPreviewRequest {
            enabled: Some(task.mcp_config.enabled),
            init_mode: Some(task.mcp_config.init_mode),
            builtin_prompt_mode: Some(task.mcp_config.builtin_prompt_mode),
            builtin_prompt_locale: Some(task.mcp_config.builtin_prompt_locale),
            enabled_builtin_kinds: Some(task.mcp_config.enabled_builtin_kinds),
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
        let init_mode = request
            .init_mode
            .unwrap_or(chatos_ai_runtime::TaskMcpInitMode::BuiltinOnly);
        let builtin_prompt_mode = request
            .builtin_prompt_mode
            .unwrap_or(TaskBuiltinMcpPromptMode::Effective);
        let builtin_prompt_locale = request
            .builtin_prompt_locale
            .clone()
            .unwrap_or_else(|| BuiltinMcpPromptLocale::DEFAULT_KEY.to_string());
        let selected_kind_names = request.enabled_builtin_kinds.unwrap_or_default();

        let mcp_config = TaskMcpConfig {
            enabled,
            init_mode,
            builtin_prompt_mode,
            builtin_prompt_locale: builtin_prompt_locale.clone(),
            enabled_builtin_kinds: selected_kind_names,
            workspace_dir: normalized_optional(request.workspace_dir),
            default_remote_server_id: normalized_optional(request.default_remote_server_id),
        };
        let selected_builtin_kinds =
            if enabled && !matches!(init_mode, chatos_ai_runtime::TaskMcpInitMode::Disabled) {
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
            self.ui_prompt_service.clone(),
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
