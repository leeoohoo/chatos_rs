// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl TaskService {
    pub(crate) async fn resolve_task_runner_policy_for_agent_project(
        &self,
        current_user: Option<&CurrentUser>,
        owner_user_id: Option<&str>,
        agent_key: SystemAgentKey,
        project_id: &str,
        task_profile: Option<&str>,
        schedule_mode: Option<&str>,
    ) -> Result<Option<TaskRunnerCapabilityPolicy>, String> {
        let Some(client) = self.plugin_management_client.as_ref() else {
            // Task definition CRUD does not execute an Agent or grant tools. The run path below
            // remains fail-closed and must resolve Plugin Management before model execution.
            return Ok(None);
        };
        let owner_user_id = resolved_owner_user_id(current_user, owner_user_id)?;
        resolve_policy(
            client,
            &self.config,
            owner_user_id,
            get_current_access_token().as_deref(),
            agent_key,
            project_id,
            task_profile,
            schedule_mode,
        )
        .await
    }
}

impl RunService {
    pub(crate) async fn resolve_task_runner_agent_key_for_task(
        &self,
        task: &TaskRecord,
    ) -> Result<SystemAgentKey, String> {
        Ok(crate::models::task_runner_agent_key_for(
            task.task_profile.as_str(),
            task.mcp_config.requires_execution,
        ))
    }

    pub(crate) async fn resolve_task_runner_policy_for_task(
        &self,
        task: &TaskRecord,
    ) -> Result<Option<TaskRunnerCapabilityPolicy>, String> {
        let Some(client) = self.plugin_management_client.as_ref() else {
            return Ok(None);
        };
        let owner_user_id = task_owner_user_id(task)
            .ok_or_else(|| "task owner user id is required for plugin policy".to_string())?;
        let agent_key = crate::models::task_runner_agent_key_for(
            task.task_profile.as_str(),
            task.mcp_config.requires_execution,
        );
        resolve_policy(
            client,
            &self.config,
            owner_user_id,
            None,
            agent_key,
            task.project_id.as_str(),
            Some(task.task_profile.as_str()),
            Some(task.schedule.mode.mode_key()),
        )
        .await
    }
}

async fn resolve_policy(
    client: &PluginManagementClient,
    config: &crate::config::AppConfig,
    owner_user_id: &str,
    access_token: Option<&str>,
    agent_key: SystemAgentKey,
    project_id: &str,
    task_profile: Option<&str>,
    schedule_mode: Option<&str>,
) -> Result<Option<TaskRunnerCapabilityPolicy>, String> {
    let runtime_context =
        crate::services::task_plugin_runtime_context::resolve_task_plugin_runtime_context(
            config,
            owner_user_id,
            project_id,
        )
        .await?;
    tracing::debug!(
        owner_user_id = runtime_context.owner_user_id.as_str(),
        project_id = runtime_context.project_id.as_str(),
        runtime_provider = runtime_context.runtime_provider.as_str(),
        device_id = runtime_context.device_id.as_deref().unwrap_or(""),
        workspace_id = runtime_context.workspace_id.as_deref().unwrap_or(""),
        project_context_revision = runtime_context
            .project_context_revision
            .as_deref()
            .unwrap_or(""),
        agent_key = agent_key.as_str(),
        "resolved Task Runner Plugin runtime context"
    );
    let request = ResolveAgentCapabilitiesRequest::new(agent_key, owner_user_id)
        .with_runtime_context(
            normalized_text(task_profile),
            Some(runtime_context.runtime_provider.clone()),
            normalized_text(schedule_mode),
        )
        .with_device_id(runtime_context.device_id.clone());
    let capabilities = if let Some(access_token) = access_token {
        client
            .resolve_for_user(&request, access_token)
            .await
            .map_err(|err| err.to_string())?
    } else {
        client
            .resolve_for_service(&request)
            .await
            .map_err(|err| err.to_string())?
    };
    TaskRunnerCapabilityPolicy::new(capabilities, runtime_context).map(Some)
}

fn normalized_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn resolved_owner_user_id<'a>(
    current_user: Option<&'a CurrentUser>,
    task_owner_user_id: Option<&'a str>,
) -> Result<&'a str, String> {
    let current_owner = current_user.and_then(CurrentUser::effective_owner_user_id);
    let task_owner = task_owner_user_id
        .map(str::trim)
        .filter(|value| !value.is_empty());
    match (current_owner, task_owner) {
        (Some(current_owner), Some(task_owner)) if current_owner != task_owner => {
            Err("task owner does not match authenticated owner".to_string())
        }
        (Some(owner), _) | (_, Some(owner)) => Ok(owner),
        (None, None) => Err("task owner user id is required for plugin policy".to_string()),
    }
}

fn task_owner_user_id(task: &TaskRecord) -> Option<&str> {
    task.owner_user_id
        .as_deref()
        .or(task.creator_user_id.as_deref())
        .or(Some(task.subject_id.as_str()))
        .map(str::trim)
        .filter(|value| !value.is_empty())
}
