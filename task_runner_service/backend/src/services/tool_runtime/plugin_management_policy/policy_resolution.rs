// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::services::TaskProjectService;

impl TaskService {
    pub(crate) async fn resolve_task_runner_policy_for_agent_on_device(
        &self,
        current_user: Option<&CurrentUser>,
        owner_user_id: Option<&str>,
        agent_key: SystemAgentKey,
        device_id: Option<String>,
    ) -> Result<Option<TaskRunnerCapabilityPolicy>, String> {
        let runtime_provider = device_id.as_ref().map(|_| "local_connector".to_string());
        self.resolve_task_runner_policy_for_agent_runtime(
            current_user,
            owner_user_id,
            agent_key,
            device_id,
            runtime_provider,
        )
        .await
    }

    pub(crate) async fn resolve_task_runner_policy_for_agent_runtime(
        &self,
        current_user: Option<&CurrentUser>,
        owner_user_id: Option<&str>,
        agent_key: SystemAgentKey,
        device_id: Option<String>,
        runtime_provider: Option<String>,
    ) -> Result<Option<TaskRunnerCapabilityPolicy>, String> {
        let Some(client) = self.plugin_management_client.as_ref() else {
            // Task definition CRUD does not execute an Agent or grant tools. The run path below
            // remains fail-closed and must resolve Plugin Management before model execution.
            return Ok(None);
        };
        let owner_user_id = resolved_owner_user_id(current_user, owner_user_id)?;
        resolve_policy(
            client,
            owner_user_id,
            get_current_access_token().as_deref(),
            agent_key,
            Some(TaskRunnerPolicyRuntimeContext {
                device_id,
                runtime_provider,
                ..TaskRunnerPolicyRuntimeContext::default()
            }),
        )
        .await
    }
}

impl RunService {
    pub(crate) async fn resolve_task_runner_policy_for_task(
        &self,
        task: &TaskRecord,
    ) -> Result<Option<TaskRunnerCapabilityPolicy>, String> {
        let Some(client) = self.plugin_management_client.as_ref() else {
            return Ok(None);
        };
        let owner_user_id = task_owner_user_id(task)
            .ok_or_else(|| "task owner user id is required for plugin policy".to_string())?;
        let project_source_type = self.task_project_source_type(task).await?;
        let runtime_provider = if project_source_type
            .as_deref()
            .is_some_and(|value| value.eq_ignore_ascii_case("local"))
        {
            "local_connector"
        } else {
            "cloud"
        };
        resolve_policy(
            client,
            owner_user_id,
            None,
            crate::models::task_runner_agent_key_for(
                task.task_profile.as_str(),
                task.mcp_config.requires_execution,
            ),
            Some(TaskRunnerPolicyRuntimeContext {
                task_profile: Some(task.task_profile.clone()),
                project_source_type,
                runtime_provider: Some(runtime_provider.to_string()),
                schedule_mode: Some(task.schedule.mode.mode_key().to_string()),
                device_id: normalized_text(task.plugin_config.device_id.clone()),
            }),
        )
        .await
    }

    async fn task_project_source_type(&self, task: &TaskRecord) -> Result<Option<String>, String> {
        if task.project_id == crate::models::PUBLIC_PROJECT_ID {
            return Ok(Some("public".to_string()));
        }
        let project_service =
            TaskProjectService::new_with_config(self.store.clone(), self.config.clone());
        Ok(project_service
            .get_project(task.project_id.as_str())
            .await?
            .and_then(|project| normalized_text(project.source_type)))
    }
}

#[derive(Debug, Clone, Default)]
struct TaskRunnerPolicyRuntimeContext {
    task_profile: Option<String>,
    project_source_type: Option<String>,
    runtime_provider: Option<String>,
    schedule_mode: Option<String>,
    device_id: Option<String>,
}

async fn resolve_policy(
    client: &PluginManagementClient,
    owner_user_id: &str,
    access_token: Option<&str>,
    agent_key: SystemAgentKey,
    runtime_context: Option<TaskRunnerPolicyRuntimeContext>,
) -> Result<Option<TaskRunnerCapabilityPolicy>, String> {
    let runtime_context = runtime_context.unwrap_or_default();
    let portable_uses_local =
        runtime_context.runtime_provider.as_deref() == Some("local_connector");
    let request = ResolveAgentCapabilitiesRequest::new(agent_key, owner_user_id)
        .with_runtime_context(
            runtime_context.task_profile,
            runtime_context.project_source_type,
            runtime_context.runtime_provider,
            runtime_context.schedule_mode,
        )
        .with_device_id(runtime_context.device_id);
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
    TaskRunnerCapabilityPolicy::new_for_runtime(capabilities, portable_uses_local).map(Some)
}

fn normalized_text(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
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
