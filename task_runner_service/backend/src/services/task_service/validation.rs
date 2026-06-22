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

    pub(super) async fn ensure_remote_server_exists(
        &self,
        id: &str,
        current_user: Option<&CurrentUser>,
    ) -> Result<(), String> {
        match self.store.get_remote_server(id).await? {
            Some(server) if server.enabled => ensure_owned_service_resource_access(
                resource_owner_or_creator(
                    server.owner_user_id.as_deref(),
                    server.creator_user_id.as_deref(),
                ),
                current_user,
            ),
            Some(_) => Err(format!("remote server is disabled: {id}")),
            None => Err(format!("remote server not found: {id}")),
        }
    }

    pub(super) async fn ensure_external_mcp_config_exists(
        &self,
        id: &str,
        current_user: Option<&CurrentUser>,
    ) -> Result<(), String> {
        match self.store.get_external_mcp_config(id).await? {
            Some(config) if config.enabled => ensure_owned_service_resource_access(
                resource_owner_or_creator(
                    config.owner_user_id.as_deref(),
                    config.creator_user_id.as_deref(),
                ),
                current_user,
            ),
            Some(_) => Err(format!("external MCP config is disabled: {id}")),
            None => Err(format!("external MCP config not found: {id}")),
        }
    }

    pub(super) async fn validate_task_mcp_config(
        &self,
        config: &TaskMcpConfig,
        current_user: Option<&CurrentUser>,
    ) -> Result<(), String> {
        if let Some(remote_server_id) = config.default_remote_server_id.as_deref() {
            self.ensure_remote_server_exists(remote_server_id, current_user)
                .await?;
        }
        for external_mcp_config_id in &config.external_mcp_config_ids {
            self.ensure_external_mcp_config_exists(external_mcp_config_id, current_user)
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

fn ensure_owned_service_resource_access(
    owner_user_id: Option<&str>,
    current_user: Option<&CurrentUser>,
) -> Result<(), String> {
    let Some(current_user) = current_user else {
        return Ok(());
    };
    if current_user.is_admin() {
        return Ok(());
    }
    let Some(expected_owner_user_id) = current_user.effective_owner_user_id() else {
        return Err("当前登录态缺少用户归属信息".to_string());
    };
    if owner_user_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        == Some(expected_owner_user_id)
    {
        Ok(())
    } else {
        Err("无权引用该资源".to_string())
    }
}

fn resource_owner_or_creator<'a>(
    owner_user_id: Option<&'a str>,
    creator_user_id: Option<&'a str>,
) -> Option<&'a str> {
    owner_user_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            creator_user_id
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })
}
