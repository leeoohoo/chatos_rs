// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use axum::extract::FromRef;

use super::ApiError;
use crate::models::{
    CurrentUser, LocalConnectorDevice, LocalConnectorWorkspace, DEVICE_STATUS_REVOKED,
    WORKSPACE_STATUS_DISABLED,
};
use crate::relay::ConnectorRelay;
use crate::state::AppState;
use crate::store::ConnectorStore;

#[derive(Clone)]
pub(super) struct PluginArtifactRelayState {
    pub(super) relay: ConnectorRelay,
    pub(super) read_timeout: Duration,
    pub(super) write_timeout: Duration,
    authorizer: PluginArtifactRelayAuthorizer,
}

#[derive(Clone)]
enum PluginArtifactRelayAuthorizer {
    Store(ConnectorStore),
    #[cfg(feature = "test-support")]
    Fixed(Box<PluginArtifactRelayTestScope>),
}

impl FromRef<AppState> for PluginArtifactRelayState {
    fn from_ref(state: &AppState) -> Self {
        Self {
            relay: state.relay.clone(),
            read_timeout: state.config.relay_request_timeout,
            write_timeout: state.config.plugin_hook_relay_request_timeout,
            authorizer: PluginArtifactRelayAuthorizer::Store(state.store.clone()),
        }
    }
}

impl PluginArtifactRelayState {
    pub(super) async fn authorize(
        &self,
        user: &CurrentUser,
        device_id: &str,
        workspace_id: &str,
    ) -> Result<(), ApiError> {
        let owner_user_id = user.effective_owner_user_id();
        match &self.authorizer {
            PluginArtifactRelayAuthorizer::Store(store) => {
                let device = store
                    .get_device(device_id)
                    .await
                    .map_err(ApiError::internal)?
                    .ok_or_else(|| ApiError::not_found("Local Connector device not found"))?;
                validate_device(owner_user_id, &device)?;
                let active_lease = store
                    .session_holds_active_lease(owner_user_id, device_id)
                    .await
                    .map_err(ApiError::internal)?;
                validate_active_lease(active_lease)?;
                let workspace = store
                    .get_workspace(workspace_id)
                    .await
                    .map_err(ApiError::internal)?
                    .ok_or_else(|| ApiError::not_found("Local Connector workspace not found"))?;
                validate_workspace(owner_user_id, device_id, workspace_id, &workspace)
            }
            #[cfg(feature = "test-support")]
            PluginArtifactRelayAuthorizer::Fixed(scope) => {
                let device = (scope.device.id == device_id)
                    .then_some(&scope.device)
                    .ok_or_else(|| ApiError::not_found("Local Connector device not found"))?;
                validate_device(owner_user_id, device)?;
                validate_active_lease(scope.active_lease)?;
                let workspace = (scope.workspace.id == workspace_id)
                    .then_some(&scope.workspace)
                    .ok_or_else(|| ApiError::not_found("Local Connector workspace not found"))?;
                validate_workspace(owner_user_id, device_id, workspace_id, workspace)
            }
        }
    }

    #[cfg(feature = "test-support")]
    pub(super) fn for_test(
        relay: ConnectorRelay,
        read_timeout: Duration,
        write_timeout: Duration,
        scope: PluginArtifactRelayTestScope,
    ) -> Self {
        Self {
            relay,
            read_timeout,
            write_timeout,
            authorizer: PluginArtifactRelayAuthorizer::Fixed(Box::new(scope)),
        }
    }

    #[cfg(feature = "test-support")]
    pub(super) fn for_store_test(
        relay: ConnectorRelay,
        read_timeout: Duration,
        write_timeout: Duration,
        store: ConnectorStore,
    ) -> Self {
        Self {
            relay,
            read_timeout,
            write_timeout,
            authorizer: PluginArtifactRelayAuthorizer::Store(store),
        }
    }
}

fn validate_device(owner_user_id: &str, device: &LocalConnectorDevice) -> Result<(), ApiError> {
    if device.owner_user_id != owner_user_id {
        return Err(ApiError::forbidden(
            "Local Connector device does not belong to current user",
        ));
    }
    if device.status == DEVICE_STATUS_REVOKED {
        return Err(ApiError::bad_request(
            "Local Connector device has been revoked",
        ));
    }
    Ok(())
}

fn validate_active_lease(active_lease: bool) -> Result<(), ApiError> {
    if !active_lease {
        return Err(ApiError::service_unavailable(
            "Local Connector device does not hold the active session lease",
        ));
    }
    Ok(())
}

fn validate_workspace(
    owner_user_id: &str,
    device_id: &str,
    workspace_id: &str,
    workspace: &LocalConnectorWorkspace,
) -> Result<(), ApiError> {
    if workspace.owner_user_id != owner_user_id {
        return Err(ApiError::forbidden(
            "Local Connector workspace does not belong to current user",
        ));
    }
    if workspace.device_id != device_id {
        return Err(ApiError::bad_request(
            "Local Connector workspace is not attached to the selected device",
        ));
    }
    if workspace.id != workspace_id {
        return Err(ApiError::not_found("Local Connector workspace not found"));
    }
    if workspace.status == WORKSPACE_STATUS_DISABLED {
        return Err(ApiError::bad_request(
            "Local Connector workspace is disabled",
        ));
    }
    Ok(())
}

#[cfg(any(test, feature = "test-support"))]
#[derive(Debug, Clone)]
pub struct PluginArtifactRelayTestScope {
    pub device: LocalConnectorDevice,
    pub workspace: LocalConnectorWorkspace,
    pub active_lease: bool,
}

#[cfg(any(test, feature = "test-support"))]
impl PluginArtifactRelayTestScope {
    pub fn new(owner_user_id: &str, device_id: &str, workspace_id: &str) -> Self {
        let mut device = LocalConnectorDevice::new(
            owner_user_id.to_string(),
            "Packaged Connector fixture".to_string(),
            "fixture-public-key".to_string(),
            Some("test".to_string()),
            Some("test".to_string()),
        );
        device.id = device_id.to_string();
        let mut workspace = LocalConnectorWorkspace::new(
            owner_user_id.to_string(),
            device_id.to_string(),
            "Packaged workspace fixture".to_string(),
            "fixture-workspace".to_string(),
            "fixture-workspace-fingerprint".to_string(),
            Vec::new(),
        );
        workspace.id = workspace_id.to_string();
        Self {
            device,
            workspace,
            active_lease: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{DEVICE_STATUS_REVOKED, WORKSPACE_STATUS_DISABLED};

    #[test]
    fn artifact_relay_scope_fails_closed_on_identity_lease_and_status_drift() {
        let scope = PluginArtifactRelayTestScope::new("owner-a", "device-a", "workspace-a");
        validate_device("owner-a", &scope.device).expect("owned device");
        validate_active_lease(scope.active_lease).expect("active lease");
        validate_workspace("owner-a", "device-a", "workspace-a", &scope.workspace)
            .expect("owned attached workspace");

        assert!(validate_device("owner-other", &scope.device)
            .expect_err("cross-owner device must fail")
            .message()
            .contains("does not belong"));
        let mut revoked = scope.device.clone();
        revoked.status = DEVICE_STATUS_REVOKED.to_string();
        assert!(validate_device("owner-a", &revoked)
            .expect_err("revoked device must fail")
            .message()
            .contains("revoked"));
        assert!(validate_active_lease(false)
            .expect_err("inactive lease must fail")
            .message()
            .contains("active session lease"));

        let mut detached = scope.workspace.clone();
        detached.device_id = "device-other".to_string();
        assert!(
            validate_workspace("owner-a", "device-a", "workspace-a", &detached)
                .expect_err("detached workspace must fail")
                .message()
                .contains("not attached")
        );
        let mut disabled = scope.workspace;
        disabled.status = WORKSPACE_STATUS_DISABLED.to_string();
        assert!(
            validate_workspace("owner-a", "device-a", "workspace-a", &disabled)
                .expect_err("disabled workspace must fail")
                .message()
                .contains("disabled")
        );
    }
}
