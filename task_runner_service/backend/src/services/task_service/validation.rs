// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

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
        task_owner_user_id: Option<&str>,
    ) -> Result<(), String> {
        match self.store.get_external_mcp_config(id).await? {
            Some(config) if config.enabled => {
                let config_owner_user_id = resource_owner_or_creator(
                    config.owner_user_id.as_deref(),
                    config.creator_user_id.as_deref(),
                );
                ensure_owned_service_resource_access(config_owner_user_id, current_user)?;
                ensure_external_mcp_owner_matches_task(id, config_owner_user_id, task_owner_user_id)
            }
            Some(_) => Err(format!("external MCP config is disabled: {id}")),
            None => Err(format!("external MCP config not found: {id}")),
        }
    }

    pub(super) async fn validate_task_external_mcp_configs(
        &self,
        config: &TaskMcpConfig,
        current_user: Option<&CurrentUser>,
        task_owner_user_id: Option<&str>,
    ) -> Result<(), String> {
        for external_mcp_config_id in &config.external_mcp_config_ids {
            self.ensure_external_mcp_config_exists(
                external_mcp_config_id,
                current_user,
                task_owner_user_id,
            )
            .await?;
        }
        Ok(())
    }

    pub(super) fn validate_task_ephemeral_http_servers(
        &self,
        config: &TaskMcpConfig,
    ) -> Result<(), String> {
        for server in &config.ephemeral_http_servers {
            let name = server.name.trim();
            if name.is_empty() {
                return Err("ephemeral HTTP MCP server name is required".to_string());
            }
            let url = server.url.trim();
            if url.is_empty() {
                return Err(format!("ephemeral HTTP MCP server url is required: {name}"));
            }
            let parsed = reqwest::Url::parse(url).map_err(|err| {
                format!("ephemeral HTTP MCP server url is invalid: {name}: {err}")
            })?;
            if !matches!(parsed.scheme(), "http" | "https") {
                return Err(format!(
                    "ephemeral HTTP MCP server url must use http or https: {name}"
                ));
            }
            if let Some(auth_mode) = server.auth_mode.as_deref() {
                if !matches!(
                    auth_mode,
                    crate::models::TASK_MCP_HTTP_AUTH_LOCAL_CONNECTOR_INTERNAL
                        | crate::models::TASK_MCP_HTTP_AUTH_PROJECT_SERVICE_SYNC
                ) {
                    return Err(format!(
                        "unsupported ephemeral HTTP MCP auth_mode for {name}: {auth_mode}"
                    ));
                }
            }
        }
        Ok(())
    }

    pub(super) async fn validate_task_mcp_config(
        &self,
        config: &TaskMcpConfig,
        current_user: Option<&CurrentUser>,
        task_owner_user_id: Option<&str>,
    ) -> Result<(), String> {
        if let Some(remote_server_id) = config.default_remote_server_id.as_deref() {
            self.ensure_remote_server_exists(remote_server_id, current_user)
                .await?;
        }
        self.validate_task_external_mcp_configs(config, current_user, task_owner_user_id)
            .await?;
        self.validate_task_ephemeral_http_servers(config)?;
        if config.workspace_dir.is_some() {
            let _ = ensure_workspace_dir_available(
                self.config.default_workspace_dir.as_str(),
                config.workspace_dir.as_deref(),
            )?;
        }
        Ok(())
    }
}

fn ensure_external_mcp_owner_matches_task(
    external_mcp_config_id: &str,
    config_owner_user_id: Option<&str>,
    task_owner_user_id: Option<&str>,
) -> Result<(), String> {
    let config_owner_user_id = config_owner_user_id
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let task_owner_user_id = task_owner_user_id
        .map(str::trim)
        .filter(|value| !value.is_empty());

    if let (Some(config_owner_user_id), Some(task_owner_user_id)) =
        (config_owner_user_id, task_owner_user_id)
    {
        if config_owner_user_id == task_owner_user_id {
            Ok(())
        } else {
            Err(format!(
                "external MCP config owner does not match task owner: {external_mcp_config_id}"
            ))
        }
    } else if config_owner_user_id.is_none() && task_owner_user_id.is_some() {
        Err(format!(
            "external MCP config is missing owner information: {external_mcp_config_id}"
        ))
    } else {
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

#[cfg(test)]
mod tests {
    use super::ensure_external_mcp_owner_matches_task;

    #[test]
    fn external_mcp_owner_must_match_task_owner() {
        assert!(
            ensure_external_mcp_owner_matches_task("mcp-1", Some("user-1"), Some("user-1")).is_ok()
        );

        assert!(
            ensure_external_mcp_owner_matches_task("mcp-1", Some("user-2"), Some("user-1"))
                .is_err()
        );

        assert!(ensure_external_mcp_owner_matches_task("mcp-1", None, Some("user-1")).is_err());
    }
}
