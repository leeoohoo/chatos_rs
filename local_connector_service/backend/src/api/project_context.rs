// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::{extract::State, Extension, Json};
use chatos_mcp_management_sdk::{ClientProjectContextSnapshot, ProjectContextAuthorization};

use crate::models::{
    CurrentUser, LocalConnectorDevice, LocalConnectorWorkspace, DEVICE_STATUS_OFFLINE,
    DEVICE_STATUS_ONLINE, DEVICE_STATUS_REGISTERED, WORKSPACE_STATUS_ACTIVE,
};
use crate::state::AppState;

use super::{load_owned_device, load_owned_workspace, ApiError};

/// Stateless: only reads existing Connector grants. Never creates a server project, binding,
/// filesystem entry or import receipt. The client remains the sole project registry.
pub(super) async fn authorize_project_context(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(snapshot): Json<ClientProjectContextSnapshot>,
) -> Result<Json<ProjectContextAuthorization>, ApiError> {
    snapshot.validate().map_err(ApiError::bad_request)?;
    let target = &snapshot.execution_target;
    let device = load_owned_device(&state, &user, &target.device_id, true).await?;
    let workspace = load_owned_workspace(&state, &user, &target.workspace_id).await?;
    authorize_snapshot(
        user.effective_owner_user_id(),
        snapshot,
        &device,
        &workspace,
    )
    .map(Json)
}

fn authorize_snapshot(
    authenticated_owner: &str,
    snapshot: ClientProjectContextSnapshot,
    device: &LocalConnectorDevice,
    workspace: &LocalConnectorWorkspace,
) -> Result<ProjectContextAuthorization, ApiError> {
    snapshot.validate().map_err(ApiError::bad_request)?;
    let target = &snapshot.execution_target;
    if device.owner_user_id != authenticated_owner || workspace.owner_user_id != authenticated_owner
    {
        return Err(ApiError::forbidden(
            "project execution target does not belong to the authenticated owner",
        ));
    }
    if device.id != target.device_id
        || workspace.id != target.workspace_id
        || workspace.device_id != device.id
    {
        return Err(ApiError::forbidden(
            "project execution target device/workspace binding does not match",
        ));
    }
    if device.revoked_at.is_some()
        || !matches!(
            device.status.as_str(),
            DEVICE_STATUS_REGISTERED | DEVICE_STATUS_ONLINE | DEVICE_STATUS_OFFLINE
        )
    {
        return Err(ApiError::forbidden(
            "project execution device is revoked or unavailable",
        ));
    }
    if workspace.status != WORKSPACE_STATUS_ACTIVE {
        return Err(ApiError::forbidden(
            "project execution workspace is disabled",
        ));
    }
    // Offline devices may be queued, but no directory authorization is inferred here. The native
    // Relay must compare this fingerprint and resolve the relative directory at execution time.
    let authorization = ProjectContextAuthorization {
        snapshot,
        owner_user_id: authenticated_owner.to_string(),
        workspace_fingerprint: workspace.local_path_fingerprint.clone(),
    };
    authorization.validate().map_err(ApiError::bad_request)?;
    Ok(authorization)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatos_mcp_management_sdk::ClientProjectExecutionTarget;

    fn fixture() -> (
        ClientProjectContextSnapshot,
        LocalConnectorDevice,
        LocalConnectorWorkspace,
    ) {
        let mut device =
            LocalConnectorDevice::new("alice".into(), "PC".into(), "key".into(), None, None);
        device.id = "device".into();
        let mut workspace = LocalConnectorWorkspace::new(
            "alice".into(),
            device.id.clone(),
            "Work".into(),
            "repo".into(),
            "binding-v1".into(),
            vec![],
        );
        workspace.id = "workspace".into();
        let snapshot = ClientProjectContextSnapshot {
            schema_version: 1,
            project_id: "client-only-project".into(),
            project_name: "Local project".into(),
            project_revision: 1,
            execution_target: ClientProjectExecutionTarget {
                device_id: device.id.clone(),
                workspace_id: workspace.id.clone(),
                relative_root: "app".into(),
            },
        };
        (snapshot, device, workspace)
    }

    #[test]
    fn accepts_client_only_identity_and_offline_device_without_project_lookup() {
        let (snapshot, mut device, workspace) = fixture();
        device.status = DEVICE_STATUS_OFFLINE.into();
        let result = authorize_snapshot("alice", snapshot.clone(), &device, &workspace).unwrap();
        assert_eq!(result.snapshot, snapshot);
        assert_eq!(result.owner_user_id, "alice");
        assert_eq!(result.workspace_fingerprint, "binding-v1");
        assert!(result.execution_context().is_ok());
    }

    #[test]
    fn rejects_owner_spoofing_even_for_super_admin_shaped_requests() {
        let (snapshot, device, workspace) = fixture();
        assert!(authorize_snapshot("bob", snapshot.clone(), &device, &workspace).is_err());
        let mut stolen = workspace.clone();
        stolen.owner_user_id = "bob".into();
        assert!(authorize_snapshot("alice", snapshot, &device, &stolen).is_err());
    }

    #[test]
    fn rejects_workspace_on_another_device_and_identity_substitution() {
        let (snapshot, device, mut workspace) = fixture();
        workspace.device_id = "other".into();
        assert!(authorize_snapshot("alice", snapshot.clone(), &device, &workspace).is_err());
        workspace.device_id = device.id.clone();
        workspace.id = "other-workspace".into();
        assert!(authorize_snapshot("alice", snapshot, &device, &workspace).is_err());
    }

    #[test]
    fn rejects_revoked_device_disabled_workspace_and_missing_fingerprint() {
        let (snapshot, mut device, mut workspace) = fixture();
        device.revoked_at = Some("revoked".into());
        assert!(authorize_snapshot("alice", snapshot.clone(), &device, &workspace).is_err());
        device.revoked_at = None;
        device.status = "revoked".into();
        assert!(authorize_snapshot("alice", snapshot.clone(), &device, &workspace).is_err());
        device.status = DEVICE_STATUS_ONLINE.into();
        workspace.status = "disabled".into();
        assert!(authorize_snapshot("alice", snapshot.clone(), &device, &workspace).is_err());
        workspace.status = WORKSPACE_STATUS_ACTIVE.into();
        workspace.local_path_fingerprint.clear();
        assert!(authorize_snapshot("alice", snapshot, &device, &workspace).is_err());
    }

    #[test]
    fn binding_change_changes_context_revision_without_rewriting_frozen_snapshot() {
        let (snapshot, device, mut workspace) = fixture();
        let original = authorize_snapshot("alice", snapshot.clone(), &device, &workspace).unwrap();
        workspace.local_path_fingerprint = "binding-v2".into();
        let moved = authorize_snapshot("alice", snapshot, &device, &workspace).unwrap();
        assert_ne!(
            original.execution_context().unwrap().revision,
            moved.execution_context().unwrap().revision
        );
        assert_eq!(original.workspace_fingerprint, "binding-v1");
    }
}
