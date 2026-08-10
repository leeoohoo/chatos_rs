// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::services::TaskProjectService;
use chatos_project_execution::parse_local_connector_workspace_root;

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
        let mut runtime_context =
            project_runtime_context(&self.store, &self.config, project_id).await?;
        runtime_context.task_profile = task_profile.map(str::to_string);
        runtime_context.schedule_mode = schedule_mode.map(str::to_string);
        self.resolve_task_runner_policy_for_agent_context(
            current_user,
            owner_user_id,
            agent_key,
            runtime_context,
        )
        .await
    }

    async fn resolve_task_runner_policy_for_agent_context(
        &self,
        current_user: Option<&CurrentUser>,
        owner_user_id: Option<&str>,
        agent_key: SystemAgentKey,
        runtime_context: TaskRunnerPolicyRuntimeContext,
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
            Some(runtime_context),
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
        let mut runtime_context =
            project_runtime_context(&self.store, &self.config, task.project_id.as_str()).await?;
        runtime_context.task_profile = Some(task.task_profile.clone());
        runtime_context.schedule_mode = Some(task.schedule.mode.mode_key().to_string());
        resolve_policy(
            client,
            owner_user_id,
            None,
            crate::models::task_runner_agent_key_for(
                task.task_profile.as_str(),
                task.mcp_config.requires_execution,
            ),
            Some(runtime_context),
        )
        .await
    }
}

#[derive(Debug, Clone, Default)]
struct TaskRunnerPolicyRuntimeContext {
    task_profile: Option<String>,
    project_source_type: Option<String>,
    runtime_provider: Option<String>,
    schedule_mode: Option<String>,
    device_id: Option<String>,
    workspace_id: Option<String>,
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
    let runtime_device_id = runtime_context.device_id.clone();
    let request = ResolveAgentCapabilitiesRequest::new(agent_key, owner_user_id)
        .with_runtime_context(
            runtime_context.task_profile,
            runtime_context.project_source_type,
            runtime_context.runtime_provider,
            runtime_context.schedule_mode,
        )
        .with_device_id(runtime_device_id);
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
    TaskRunnerCapabilityPolicy::new_for_runtime(capabilities, portable_uses_local)
        .map(|policy| {
            policy.with_project_runtime_target(
                runtime_context.device_id,
                runtime_context.workspace_id,
            )
        })
        .map(Some)
}

async fn project_runtime_context(
    store: &crate::store::AppStore,
    config: &crate::config::AppConfig,
    project_id: &str,
) -> Result<TaskRunnerPolicyRuntimeContext, String> {
    if project_id == crate::models::PUBLIC_PROJECT_ID {
        return Ok(TaskRunnerPolicyRuntimeContext {
            project_source_type: Some("public".to_string()),
            runtime_provider: Some("cloud".to_string()),
            ..TaskRunnerPolicyRuntimeContext::default()
        });
    }
    let project_service = TaskProjectService::new_with_config(store.clone(), config.clone());
    let project = project_service
        .get_project(project_id)
        .await?
        .ok_or_else(|| format!("task project not found: {project_id}"))?;
    runtime_context_for_project(project.source_type, project.root_path.as_deref())
}

fn runtime_context_for_project(
    source_type: Option<String>,
    root_path: Option<&str>,
) -> Result<TaskRunnerPolicyRuntimeContext, String> {
    let project_source_type = normalized_text(source_type);
    let local = project_source_type.as_deref().is_some_and(|value| {
        value.eq_ignore_ascii_case("local") || value.eq_ignore_ascii_case("local_connector")
    });
    let (device_id, workspace_id) = if local {
        let root_path = root_path
            .and_then(|value| parse_local_connector_workspace_root(value.trim()))
            .ok_or_else(|| {
                "Local Connector project is missing its managed device/workspace reference"
                    .to_string()
            })?;
        (Some(root_path.device_id), Some(root_path.workspace_id))
    } else {
        (None, None)
    };
    Ok(TaskRunnerPolicyRuntimeContext {
        project_source_type,
        runtime_provider: Some(if local { "local_connector" } else { "cloud" }.to_string()),
        device_id,
        workspace_id,
        ..TaskRunnerPolicyRuntimeContext::default()
    })
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

#[cfg(test)]
mod tests {
    use super::runtime_context_for_project;

    #[test]
    fn local_project_runtime_target_comes_from_managed_root() {
        let context = runtime_context_for_project(
            Some("local".to_string()),
            Some("local://connector/device-1/workspace-1/apps/inventory"),
        )
        .expect("local project runtime context");

        assert_eq!(context.runtime_provider.as_deref(), Some("local_connector"));
        assert_eq!(context.device_id.as_deref(), Some("device-1"));
        assert_eq!(context.workspace_id.as_deref(), Some("workspace-1"));
    }

    #[test]
    fn cloud_project_runtime_has_no_connector_target() {
        let context = runtime_context_for_project(None, Some("/workspace/inventory"))
            .expect("cloud project runtime context");

        assert_eq!(context.runtime_provider.as_deref(), Some("cloud"));
        assert!(context.device_id.is_none());
        assert!(context.workspace_id.is_none());
    }

    #[test]
    fn malformed_local_project_runtime_fails_closed() {
        let error = runtime_context_for_project(
            Some("local_connector".to_string()),
            Some("/workspace/not-managed"),
        )
        .expect_err("local project without managed target must fail");

        assert!(error.contains("managed device/workspace reference"));
    }
}
