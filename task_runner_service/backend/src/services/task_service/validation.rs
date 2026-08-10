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

    pub(super) async fn validate_task_mcp_config_for_agent(
        &self,
        config: &TaskMcpConfig,
        plugin_config: &chatos_plugin_management_sdk::TaskPluginConfig,
        project_id: &str,
        current_user: Option<&CurrentUser>,
        task_owner_user_id: Option<&str>,
        agent_key: chatos_plugin_management_sdk::SystemAgentKey,
        task_profile: &str,
        schedule_mode: &str,
    ) -> Result<bool, String> {
        let policy_resolved = self
            .validate_task_capability_selection_for_agent(
                config,
                plugin_config,
                project_id,
                current_user,
                task_owner_user_id,
                agent_key,
                task_profile,
                schedule_mode,
            )
            .await?;
        if let Some(remote_server_id) = config.default_remote_server_id.as_deref() {
            self.ensure_remote_server_exists(remote_server_id, current_user)
                .await?;
        }
        self.validate_task_ephemeral_http_servers(config)?;
        if config.workspace_dir.is_some() {
            let _ = ensure_workspace_dir_available(
                self.config.default_workspace_dir.as_str(),
                config.workspace_dir.as_deref(),
            )?;
        }
        Ok(policy_resolved)
    }

    pub(super) async fn validate_task_capability_selection_for_agent(
        &self,
        config: &TaskMcpConfig,
        plugin_config: &chatos_plugin_management_sdk::TaskPluginConfig,
        project_id: &str,
        current_user: Option<&CurrentUser>,
        task_owner_user_id: Option<&str>,
        agent_key: chatos_plugin_management_sdk::SystemAgentKey,
        task_profile: &str,
        schedule_mode: &str,
    ) -> Result<bool, String> {
        let Some(policy) = self
            .resolve_task_runner_policy_for_agent_project(
                current_user,
                task_owner_user_id,
                agent_key,
                project_id,
                Some(task_profile),
                Some(schedule_mode),
            )
            .await?
        else {
            return Ok(false);
        };
        policy.validate_optional_config(config)?;
        policy.validate_plugin_config(plugin_config)?;
        Ok(true)
    }
}

pub(super) fn reject_task_level_plugin_runtime_target(
    config: &chatos_plugin_management_sdk::TaskPluginConfig,
) -> Result<(), String> {
    if config
        .device_id
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
        || config
            .workspace_id
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
    {
        return Err(
            "Plugin execution device and workspace are project properties and cannot be set on a task"
                .to_string(),
        );
    }
    Ok(())
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
    use super::reject_task_level_plugin_runtime_target;
    use chatos_plugin_management_sdk::TaskPluginConfig;

    #[test]
    fn task_plugin_runtime_target_must_not_be_overridden() {
        for config in [
            TaskPluginConfig {
                device_id: Some("device-1".to_string()),
                ..TaskPluginConfig::default()
            },
            TaskPluginConfig {
                workspace_id: Some("workspace-1".to_string()),
                ..TaskPluginConfig::default()
            },
        ] {
            let error = reject_task_level_plugin_runtime_target(&config)
                .expect_err("task-level runtime target must be rejected");
            assert!(error.contains("project properties"));
        }
    }

    #[test]
    fn empty_task_plugin_runtime_target_is_accepted_for_legacy_payloads() {
        let config = TaskPluginConfig {
            device_id: Some("  ".to_string()),
            workspace_id: Some(String::new()),
            ..TaskPluginConfig::default()
        };
        reject_task_level_plugin_runtime_target(&config)
            .expect("empty legacy target values do not override the project");
    }
}
