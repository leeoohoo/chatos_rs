// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_management_sdk::ProjectContextAuthorization;

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
    pub(crate) fn server(owner_user_id: &str) -> Self {
        Self {
            owner_user_id: owner_user_id.trim().to_string(),
            project_id: None,
            workspace_id: None,
            device_id: None,
            runtime_provider: "server".into(),
            project_context_revision: None,
        }
    }
}

/// Consume only the dedicated service-authorized snapshot, never query a Project provider.
/// Current grant/fingerprint revalidation belongs to the execution boundary.
pub(crate) fn resolve_task_plugin_runtime_context(
    owner: &str,
    project_id: Option<&str>,
    authorization: Option<&ProjectContextAuthorization>,
) -> Result<TaskPluginRuntimeContext, String> {
    if owner.is_empty() || owner.trim() != owner {
        return Err("authenticated owner is required for task runtime context".into());
    }
    match (project_id, authorization) {
        (None, None) => Ok(TaskPluginRuntimeContext::server(owner)),
        (Some(project_id), Some(authorization)) => {
            authorization.validate_expected(owner, &authorization.snapshot)?;
            authorization.snapshot.validate_project_id(project_id)?;
            let execution = authorization.execution_context()?;
            Ok(TaskPluginRuntimeContext {
                owner_user_id: owner.into(),
                project_id: Some(project_id.into()),
                workspace_id: Some(authorization.snapshot.execution_target.workspace_id.clone()),
                device_id: Some(authorization.snapshot.execution_target.device_id.clone()),
                runtime_provider: "local_connector".into(),
                project_context_revision: Some(execution.revision),
            })
        }
        _ => Err(
            "project task requires a matching frozen project_context; server fallback is forbidden"
                .into(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::task_service::project_context::tests::snapshot;

    fn authorization() -> ProjectContextAuthorization {
        ProjectContextAuthorization {
            snapshot: snapshot("project-1"),
            owner_user_id: "owner-1".into(),
            workspace_fingerprint: "binding-1".into(),
        }
    }

    #[test]
    fn resolves_only_frozen_local_connector_context() {
        let context = authorization();
        let resolved =
            resolve_task_plugin_runtime_context("owner-1", Some("project-1"), Some(&context))
                .unwrap();
        assert_eq!(resolved.runtime_provider, "local_connector");
        assert_eq!(resolved.device_id.as_deref(), Some("device-1"));
        assert_eq!(
            resolved.project_context_revision.unwrap(),
            context.execution_context().unwrap().revision
        );
    }

    #[test]
    fn project_id_alone_never_falls_back_to_server() {
        assert!(resolve_task_plugin_runtime_context("owner-1", Some("project-1"), None).is_err());
        assert!(
            resolve_task_plugin_runtime_context("owner-1", None, Some(&authorization())).is_err()
        );
        assert!(resolve_task_plugin_runtime_context(
            "owner-2",
            Some("project-1"),
            Some(&authorization())
        )
        .is_err());
        assert!(resolve_task_plugin_runtime_context(
            "owner-1",
            Some("project-2"),
            Some(&authorization())
        )
        .is_err());
        assert_eq!(
            resolve_task_plugin_runtime_context("owner-1", None, None)
                .unwrap()
                .runtime_provider,
            "server"
        );
    }
}
