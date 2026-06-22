use super::*;

impl TaskService {
    pub(super) async fn ensure_model_config_access(
        &self,
        id: &str,
        current_user: Option<&CurrentUser>,
    ) -> Result<(), String> {
        let model = self
            .store
            .get_model_config(id)
            .await?
            .ok_or_else(|| format!("model config not found: {id}"))?;
        if !model.enabled {
            return Err(format!("model config is disabled: {id}"));
        }
        if let Some(current_user) = current_user {
            if !current_user.can_access_owned_resource(model.owner_user_id.as_deref()) {
                return Err(format!("model config not found: {id}"));
            }
        }
        Ok(())
    }

    pub(super) async fn ensure_remote_server_exists(&self, id: &str) -> Result<(), String> {
        match self.store.get_remote_server(id).await? {
            Some(server) if server.enabled => Ok(()),
            Some(_) => Err(format!("remote server is disabled: {id}")),
            None => Err(format!("remote server not found: {id}")),
        }
    }

    pub(super) async fn ensure_external_mcp_config_exists(&self, id: &str) -> Result<(), String> {
        match self.store.get_external_mcp_config(id).await? {
            Some(config) if config.enabled => Ok(()),
            Some(_) => Err(format!("external MCP config is disabled: {id}")),
            None => Err(format!("external MCP config not found: {id}")),
        }
    }

    pub(super) async fn validate_task_mcp_config(
        &self,
        config: &TaskMcpConfig,
    ) -> Result<(), String> {
        if let Some(remote_server_id) = config.default_remote_server_id.as_deref() {
            self.ensure_remote_server_exists(remote_server_id).await?;
        }
        for external_mcp_config_id in &config.external_mcp_config_ids {
            self.ensure_external_mcp_config_exists(external_mcp_config_id)
                .await?;
        }
        if config.workspace_dir.is_some() {
            let _ = ensure_workspace_dir_available(
                self.config.default_workspace_dir.as_str(),
                config.workspace_dir.as_deref(),
            )?;
        }
        Ok(())
    }
}
