// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_management_sdk::{ProjectExecutionContext, WorkspaceProviderKind};

use crate::config::AppConfig;
use crate::models::normalize_project_id;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TaskPluginRuntimeContext {
    pub(crate) owner_user_id: String,
    pub(crate) project_id: Option<String>,
    pub(crate) workspace_id: Option<String>,
    pub(crate) device_id: Option<String>,
    pub(crate) runtime_provider: String,
    pub(crate) project_context_revision: Option<String>,
}

impl TaskPluginRuntimeContext {
    pub(crate) fn server(owner_user_id: &str, project_id: Option<&str>) -> Self {
        Self {
            owner_user_id: owner_user_id.trim().to_string(),
            project_id: project_id
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned),
            workspace_id: None,
            device_id: None,
            runtime_provider: "server".to_string(),
            project_context_revision: None,
        }
    }
}

pub(crate) async fn resolve_task_plugin_runtime_context(
    config: &AppConfig,
    owner_user_id: &str,
    project_id: Option<&str>,
) -> Result<TaskPluginRuntimeContext, String> {
    let owner_user_id = required_text(owner_user_id, "owner_user_id")?;
    let project_id = normalize_project_id(project_id.map(ToOwned::to_owned));
    let Some(project_id) = project_id else {
        return Ok(TaskPluginRuntimeContext::server(owner_user_id, None));
    };
    let project_service_configured = config
        .project_service_internal_base_url
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| !value.is_empty());
    if !project_service_configured {
        return Ok(TaskPluginRuntimeContext::server(
            owner_user_id,
            Some(&project_id),
        ));
    }
    let context = super::project_management_api_client::resolve_project_execution_context(
        config,
        project_id.as_str(),
        owner_user_id,
    )
    .await?;
    task_plugin_runtime_context_from_project(context, owner_user_id, project_id.as_str())
}

fn task_plugin_runtime_context_from_project(
    context: ProjectExecutionContext,
    expected_owner_user_id: &str,
    expected_project_id: &str,
) -> Result<TaskPluginRuntimeContext, String> {
    if context.project_id.as_deref() != Some(expected_project_id.trim()) {
        return Err("project execution context project identity does not match".to_string());
    }
    if context.owner_user_id.trim() != expected_owner_user_id.trim() {
        return Err("project execution context owner identity does not match".to_string());
    }
    let project_context_revision =
        required_text(context.revision.as_str(), "revision")?.to_string();
    match context.workspace_provider {
        WorkspaceProviderKind::None => Ok(TaskPluginRuntimeContext {
            owner_user_id: expected_owner_user_id.trim().to_string(),
            project_id: Some(expected_project_id.trim().to_string()),
            workspace_id: None,
            device_id: None,
            runtime_provider: "server".to_string(),
            project_context_revision: Some(project_context_revision),
        }),
        WorkspaceProviderKind::LocalConnector => {
            let workspace = context.workspace.ok_or_else(|| {
                "Local Connector project execution context is missing workspace".to_string()
            })?;
            let device_id = required_text(
                workspace.device_id.as_deref().unwrap_or_default(),
                "device_id",
            )?;
            let workspace_id = required_text(workspace.workspace_id.as_str(), "workspace_id")?;
            Ok(TaskPluginRuntimeContext {
                owner_user_id: expected_owner_user_id.trim().to_string(),
                project_id: Some(expected_project_id.trim().to_string()),
                workspace_id: Some(workspace_id.to_string()),
                device_id: Some(device_id.to_string()),
                runtime_provider: WorkspaceProviderKind::LocalConnector.as_str().to_string(),
                project_context_revision: Some(project_context_revision),
            })
        }
    }
}

fn required_text<'a>(value: &'a str, field: &str) -> Result<&'a str, String> {
    let value = value.trim();
    if value.is_empty() {
        Err(format!("project execution context {field} is required"))
    } else {
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use chatos_mcp_management_sdk::WorkspaceExecutionTarget;

    use super::*;

    #[test]
    fn resolves_authoritative_local_connector_device() {
        let resolved = task_plugin_runtime_context_from_project(
            ProjectExecutionContext {
                project_id: Some("project-1".to_string()),
                owner_user_id: "owner-1".to_string(),
                workspace_provider: WorkspaceProviderKind::LocalConnector,
                workspace: Some(WorkspaceExecutionTarget {
                    device_id: Some("device-1".to_string()),
                    workspace_id: "workspace-1".to_string(),
                    relative_root: Some("apps/backend".to_string()),
                }),
                revision: "revision-1".to_string(),
            },
            "owner-1",
            "project-1",
        )
        .expect("local connector runtime context");

        assert_eq!(resolved.runtime_provider, "local_connector");
        assert_eq!(resolved.device_id.as_deref(), Some("device-1"));
        assert_eq!(resolved.workspace_id.as_deref(), Some("workspace-1"));
        assert_eq!(
            resolved.project_context_revision.as_deref(),
            Some("revision-1")
        );
    }

    #[test]
    fn rejects_local_connector_context_without_device() {
        let error = task_plugin_runtime_context_from_project(
            ProjectExecutionContext {
                project_id: Some("project-1".to_string()),
                owner_user_id: "owner-1".to_string(),
                workspace_provider: WorkspaceProviderKind::LocalConnector,
                workspace: Some(WorkspaceExecutionTarget {
                    device_id: None,
                    workspace_id: "workspace-1".to_string(),
                    relative_root: None,
                }),
                revision: "revision-1".to_string(),
            },
            "owner-1",
            "project-1",
        )
        .expect_err("missing device must fail closed");

        assert!(error.contains("device_id"));
    }

    #[test]
    fn rejects_context_identity_mismatch() {
        let error = task_plugin_runtime_context_from_project(
            ProjectExecutionContext {
                project_id: Some("another-project".to_string()),
                owner_user_id: "owner-1".to_string(),
                workspace_provider: WorkspaceProviderKind::None,
                workspace: None,
                revision: "revision-1".to_string(),
            },
            "owner-1",
            "project-1",
        )
        .expect_err("project identity mismatch");

        assert!(error.contains("project identity"));
    }
}
