use super::*;

impl TaskService {
    pub(super) async fn ensure_model_config_exists(&self, id: &str) -> Result<(), String> {
        match self.store.get_model_config(id).await? {
            Some(model) if model.enabled => Ok(()),
            Some(_) => Err(format!("模型配置未启用: {id}")),
            None => Err(format!("模型配置不存在: {id}")),
        }
    }

    pub(super) async fn ensure_remote_server_exists(&self, id: &str) -> Result<(), String> {
        match self.store.get_remote_server(id).await? {
            Some(server) if server.enabled => Ok(()),
            Some(_) => Err(format!("远程服务器未启用: {id}")),
            None => Err(format!("远程服务器不存在: {id}")),
        }
    }

    pub(super) async fn validate_task_mcp_config(
        &self,
        config: &TaskMcpConfig,
    ) -> Result<(), String> {
        if let Some(remote_server_id) = config.default_remote_server_id.as_deref() {
            self.ensure_remote_server_exists(remote_server_id).await?;
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
