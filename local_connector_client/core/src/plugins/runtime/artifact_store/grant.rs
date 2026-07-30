// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_plugin_management_sdk::{
    PluginArtifactDescriptor, PluginArtifactUiAccess,
    PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_DOWNLOAD, PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_LIST,
    PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_READ,
};
use chrono::Utc;

use super::{PluginArtifactProducer, PluginArtifactStoreState, PluginUiArtifactGrant};
use crate::relay::RelayRequest;

impl PluginUiArtifactGrant {
    pub(super) fn validate_request(
        &self,
        request: &RelayRequest,
        access: &PluginArtifactUiAccess,
        capability: &str,
    ) -> Result<(), (u16, String)> {
        let owner_user_id = request
            .owner_user_id
            .as_deref()
            .map(str::trim)
            .unwrap_or_default();
        let device_id = request
            .device_id
            .as_deref()
            .map(str::trim)
            .unwrap_or_default();
        if owner_user_id != self.owner_user_id
            || device_id != self.device_id
            || request.workspace_id.trim() != self.workspace_id
            || access.run_id != self.run_id
            || access.plugin_id != self.plugin_id
            || access.release_id != self.release_id
            || access.artifact_sha256 != self.artifact_sha256
            || access.component_key != self.component_key
            || access.adapter_session_id != self.adapter_session_id
            || access.ui_snapshot_sha256 != self.ui.snapshot_sha256
        {
            return Err((404, "Plugin UI Artifact session is unavailable".to_string()));
        }
        if !capability.is_empty()
            && !self
                .ui
                .bridge_capabilities
                .iter()
                .any(|value| value == capability)
        {
            return Err((
                403,
                "Plugin UI Artifact capability was not granted".to_string(),
            ));
        }
        Ok(())
    }

    pub(super) fn matches_producer(&self, producer: &PluginArtifactProducer<'_>) -> bool {
        self.owner_user_id == producer.owner_user_id
            && self.device_id == producer.device_id
            && self.workspace_id == producer.workspace_id
            && self.run_id == producer.run_id
            && self.plugin_id == producer.plugin_id
            && self.release_id == producer.release_id
            && self.artifact_sha256 == producer.artifact_sha256
            && self.expires_at > Utc::now().timestamp()
    }

    pub(super) fn allows_any_artifact_read(&self) -> bool {
        self.ui.bridge_capabilities.iter().any(|capability| {
            matches!(
                capability.as_str(),
                PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_LIST
                    | PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_READ
                    | PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_DOWNLOAD
            )
        })
    }

    pub(super) fn can_access(&self, artifact: &PluginArtifactDescriptor) -> bool {
        artifact.owner.owner_user_id == self.owner_user_id
            && artifact.owner.device_id == self.device_id
            && artifact.owner.workspace_id == self.workspace_id
            && artifact.owner.run_id == self.run_id
            && artifact.owner.plugin_id == self.plugin_id
            && artifact.owner.release_id == self.release_id
            && artifact.owner.artifact_sha256 == self.artifact_sha256
            && self
                .ui
                .artifact_mime_types
                .iter()
                .any(|media_type| media_type == &artifact.media_type)
    }
}

pub(super) fn ensure_active_grant(
    state: &PluginArtifactStoreState,
    grant: &PluginUiArtifactGrant,
) -> Result<(), (u16, String)> {
    if state.ui_grants.get(grant.adapter_session_id.as_str()) != Some(grant) {
        return Err((
            409,
            "Plugin UI Artifact grant changed before the write".to_string(),
        ));
    }
    Ok(())
}

pub(super) fn grant_can_retain_artifact(
    grant: &PluginUiArtifactGrant,
    artifact: &PluginArtifactDescriptor,
) -> bool {
    grant.can_access(artifact)
        && (!artifact.mutable
            || (artifact.owner.component_key == grant.component_key
                && artifact.owner.adapter_session_id == grant.adapter_session_id
                && grant
                    .ui
                    .bridge_capabilities
                    .iter()
                    .any(|capability| capability == &artifact.producer_tool_name)))
}
