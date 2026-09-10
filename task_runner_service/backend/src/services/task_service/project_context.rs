// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_management_sdk::{
    ClientProjectContextSnapshot, McpManagementClient, McpManagementClientConfig,
    ProjectContextAuthorization,
};

use super::TaskService;

#[async_trait::async_trait]
pub(crate) trait ProjectContextAuthorizer: Send + Sync {
    async fn authorize(
        &self,
        owner: &str,
        snapshot: &ClientProjectContextSnapshot,
    ) -> Result<ProjectContextAuthorization, String>;
}

pub(crate) struct McpProjectContextAuthorizer;

#[async_trait::async_trait]
impl ProjectContextAuthorizer for McpProjectContextAuthorizer {
    async fn authorize(
        &self,
        owner: &str,
        snapshot: &ClientProjectContextSnapshot,
    ) -> Result<ProjectContextAuthorization, String> {
        let config = McpManagementClientConfig::from_env("task-runner")
            .await
            .map_err(|error| error.to_string())?;
        McpManagementClient::new(config)
            .map_err(|error| error.to_string())?
            .authorize_project_context(owner, snapshot)
            .await
            .map_err(|error| error.to_string())
    }
}

impl TaskService {
    pub(crate) async fn authorize_task_project_context(
        &self,
        project_id: Option<&str>,
        snapshot: Option<&ClientProjectContextSnapshot>,
        owner: Option<&str>,
    ) -> Result<Option<ProjectContextAuthorization>, String> {
        match (project_id, snapshot) {
            (None, None) => Ok(None),
            (Some(project_id), Some(snapshot)) => {
                snapshot.validate_project_id(project_id)?;
                let owner = owner
                    .ok_or_else(|| "project task requires an authenticated owner".to_string())?;
                snapshot.validate_for_owner(owner)?;
                let context = self
                    .project_context_authorizer
                    .authorize(owner, snapshot)
                    .await?;
                context.validate_expected(owner, snapshot)?;
                Ok(Some(context))
            }
            _ => Err("project task requires a matching client project_context".to_string()),
        }
    }
}

pub(crate) async fn revalidate_task_project_context(
    task: &crate::models::TaskRecord,
    authorizer: &dyn ProjectContextAuthorizer,
) -> Result<(), String> {
    task.validate_project_context()?;
    if let Some(frozen) = task.project_context.as_ref() {
        let current = authorizer
            .authorize(&frozen.owner_user_id, &frozen.snapshot)
            .await?;
        current.validate_expected(&frozen.owner_user_id, &frozen.snapshot)?;
        if &current != frozen {
            return Err("project workspace binding changed after task creation; create a new task from the client".into());
        }
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn snapshot(project_id: &str) -> ClientProjectContextSnapshot {
        ClientProjectContextSnapshot {
            schema_version: 1,
            project_id: project_id.into(),
            project_name: "Local project".into(),
            project_revision: 3,
            execution_target: chatos_mcp_management_sdk::ClientProjectExecutionTarget {
                device_id: "device-1".into(),
                workspace_id: "workspace-1".into(),
                relative_root: "apps/repo".into(),
            },
        }
    }

    pub(crate) struct TestAuthorizer;

    #[async_trait::async_trait]
    impl ProjectContextAuthorizer for TestAuthorizer {
        async fn authorize(
            &self,
            owner: &str,
            snapshot: &ClientProjectContextSnapshot,
        ) -> Result<ProjectContextAuthorization, String> {
            Ok(ProjectContextAuthorization {
                snapshot: snapshot.clone(),
                owner_user_id: owner.into(),
                workspace_fingerprint: "test-binding-1".into(),
            })
        }
    }

    impl TaskService {
        pub(crate) fn with_test_project_authorizer(mut self) -> Self {
            self.project_context_authorizer = std::sync::Arc::new(TestAuthorizer);
            self
        }
    }

    impl crate::services::RunService {
        pub(crate) fn with_test_project_authorizer(mut self) -> Self {
            self.project_context_authorizer = std::sync::Arc::new(TestAuthorizer);
            self
        }
    }
}
