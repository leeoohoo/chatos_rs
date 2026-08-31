// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::Json;
use chatos_mcp_management_sdk::{
    ProjectExecutionContext, WorkspaceExecutionTarget, WorkspaceProviderKind,
};
use chatos_project_execution::parse_local_connector_workspace_root;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::internal_auth::{
    require_project_internal_request, MCP_MANAGEMENT_CALLER, PROJECT_EXECUTION_CONTEXT_SCOPE,
    TASK_RUNNER_CALLER,
};
use super::ApiError;
use crate::models::ProjectRecord;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub(in crate::api) struct ExecutionContextQuery {
    owner_user_id: String,
}

pub(in crate::api) async fn resolve_project_execution_context(
    Path(project_id): Path<String>,
    Query(query): Query<ExecutionContextQuery>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ProjectExecutionContext>, ApiError> {
    require_project_internal_request(
        &state.config,
        &headers,
        &[MCP_MANAGEMENT_CALLER, TASK_RUNNER_CALLER],
        PROJECT_EXECUTION_CONTEXT_SCOPE,
    )?;
    let owner_user_id = required_text(query.owner_user_id.as_str(), "owner_user_id")?;
    let project = state
        .store
        .get_project(project_id.as_str())
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("项目不存在: {project_id}")))?;
    let project_owner = project
        .owner_user_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::conflict("project owner is not initialized"))?;
    if project_owner != owner_user_id {
        return Err(ApiError::forbidden(
            "project owner does not match the requested runtime owner",
        ));
    }
    Ok(Json(build_execution_context(&project, owner_user_id)))
}

fn build_execution_context(
    project: &ProjectRecord,
    owner_user_id: &str,
) -> ProjectExecutionContext {
    let local_workspace = project
        .root_path
        .as_deref()
        .and_then(parse_local_connector_workspace);
    let workspace_provider = resolve_workspace_provider(&local_workspace);
    let workspace = (workspace_provider == WorkspaceProviderKind::LocalConnector)
        .then_some(local_workspace)
        .flatten();
    let revision = execution_context_revision(project, workspace_provider, workspace.as_ref());
    ProjectExecutionContext {
        project_id: project.id.clone(),
        owner_user_id: owner_user_id.to_string(),
        workspace_provider,
        workspace,
        revision,
    }
}

fn resolve_workspace_provider(
    local_workspace: &Option<WorkspaceExecutionTarget>,
) -> WorkspaceProviderKind {
    if local_workspace.is_some() {
        WorkspaceProviderKind::LocalConnector
    } else {
        WorkspaceProviderKind::None
    }
}

fn parse_local_connector_workspace(root_path: &str) -> Option<WorkspaceExecutionTarget> {
    // Normalize the persisted Local Connector root before exposing it to MCP Management.
    let workspace = parse_local_connector_workspace_root(root_path)?;
    Some(WorkspaceExecutionTarget {
        device_id: Some(workspace.device_id),
        workspace_id: workspace.workspace_id,
        relative_root: workspace.relative_path,
    })
}

fn execution_context_revision(
    project: &ProjectRecord,
    workspace_provider: WorkspaceProviderKind,
    workspace: Option<&WorkspaceExecutionTarget>,
) -> String {
    let input = serde_json::json!({
        "purpose": "mcp-project-execution-context-local-connector-v1",
        "project_id": project.id,
        "project_updated_at": project.updated_at,
        "workspace_provider": workspace_provider,
        "workspace": workspace,
    });
    let bytes = serde_json::to_vec(&input).unwrap_or_default();
    hex::encode(Sha256::digest(bytes))
}

fn required_text<'a>(value: &'a str, field: &str) -> Result<&'a str, ApiError> {
    let value = value.trim();
    if value.is_empty() {
        Err(ApiError::bad_request(format!("{field} is required")))
    } else {
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> ProjectRecord {
        ProjectRecord {
            id: "project-1".to_string(),
            creator_user_id: None,
            creator_username: None,
            creator_display_name: None,
            owner_user_id: Some("user-1".to_string()),
            owner_username: None,
            owner_display_name: None,
            name: "Project".to_string(),
            root_path: Some("local://connector/device-1/workspace-1".to_string()),
            git_url: None,
            cloud_import_source: crate::models::CloudImportSource::Empty,
            import_status: crate::models::ProjectImportStatus::Ready,
            source_git_url: None,
            harness_space_identifier: None,
            harness_repo_identifier: None,
            harness_repo_path: None,
            harness_git_url: None,
            harness_git_ssh_url: None,
            harness_default_branch: None,
            harness_provision_status: None,
            harness_provision_error: None,
            harness_provisioned_at: None,
            import_error: None,
            import_started_at: None,
            import_finished_at: None,
            description: None,
            status: crate::models::ProjectStatus::Active,
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
            archived_at: None,
        }
    }

    #[test]
    fn local_connector_root_becomes_normalized_workspace_context() {
        let workspace =
            parse_local_connector_workspace("local://connector/device-1/workspace-1/apps/backend")
                .unwrap();
        assert_eq!(workspace.device_id.as_deref(), Some("device-1"));
        assert_eq!(workspace.workspace_id, "workspace-1");
        assert_eq!(workspace.relative_root.as_deref(), Some("apps/backend"));
    }

    #[test]
    fn unsafe_relative_root_is_not_forwarded() {
        assert!(parse_local_connector_workspace(
            "local://connector/device-1/workspace-1/apps/%2E%2E/secrets",
        )
        .is_none());
        assert!(parse_local_connector_workspace(
            "local://connector/device-1/workspace-1/apps%2F..%2Fsecrets",
        )
        .is_none());
    }

    #[test]
    fn local_connector_project_builds_authoritative_workspace_context() {
        let project = project();
        let context = build_execution_context(&project, "user-1");

        assert_eq!(
            context.workspace_provider,
            WorkspaceProviderKind::LocalConnector
        );
        assert_eq!(
            context
                .workspace
                .as_ref()
                .map(|workspace| workspace.workspace_id.as_str()),
            Some("workspace-1")
        );
    }

    #[test]
    fn project_without_local_connector_root_has_no_mcp_workspace() {
        let mut project = project();
        project.root_path = None;
        let context = build_execution_context(&project, "user-1");

        assert_eq!(context.workspace_provider, WorkspaceProviderKind::None);
        assert!(context.workspace.is_none());
    }
}
