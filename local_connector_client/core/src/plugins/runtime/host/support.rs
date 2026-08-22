// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::PathBuf;
use std::time::Instant;

use anyhow::Result;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use chatos_plugin_management_sdk::{PluginArtifactUiAccess, PLUGIN_ARTIFACT_WRITE_MAX_BYTES};
use chatos_sandbox_contract::{
    AdditionalFileSystemPermissions, FileSystemAccessMode, FileSystemPath, FileSystemSandboxEntry,
    RequestPermissionProfile,
};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::{
    internal_error, required_body_text, required_envelope_text, required_sha256,
    PluginRuntimeTelemetryIdentity, PreparedPluginSession,
};
use crate::relay::RelayRequest;

pub(super) fn decode_artifact_write_body(body_base64: &str) -> Result<Vec<u8>, (u16, String)> {
    let encoded_limit = PLUGIN_ARTIFACT_WRITE_MAX_BYTES
        .div_ceil(3)
        .saturating_mul(4) as usize;
    if body_base64.len() > encoded_limit {
        return Err((
            413,
            "Plugin Artifact write body exceeds the encoded size limit".to_string(),
        ));
    }
    let bytes = BASE64_STANDARD.decode(body_base64).map_err(|_| {
        (
            400,
            "Plugin Artifact write body is not valid canonical Base64".to_string(),
        )
    })?;
    if bytes.len() as u64 > PLUGIN_ARTIFACT_WRITE_MAX_BYTES
        || BASE64_STANDARD.encode(bytes.as_slice()) != body_base64
    {
        return Err((
            400,
            "Plugin Artifact write body is not canonical or exceeds the size limit".to_string(),
        ));
    }
    Ok(bytes)
}

pub(super) fn approved_workspace_root(path: &std::path::Path) -> Result<PathBuf, (u16, String)> {
    let metadata = std::fs::symlink_metadata(path).map_err(|error| {
        (
            409,
            format!("read Plugin workspace metadata failed: {error}"),
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err((
            409,
            "Plugin workspace must be a non-symlink directory".to_string(),
        ));
    }
    crate::workspace::paths::canonicalize_existing_dir(path).map_err(internal_error)
}

pub(super) fn workspace_write_permission_request(
    workspace_root: &std::path::Path,
) -> RequestPermissionProfile {
    let workspace_root = workspace_root.to_string_lossy().into_owned();
    RequestPermissionProfile {
        file_system: Some(AdditionalFileSystemPermissions {
            entries: Some(vec![
                FileSystemSandboxEntry {
                    access: FileSystemAccessMode::Write,
                    path: FileSystemPath::Path {
                        path: workspace_root.clone(),
                    },
                },
                FileSystemSandboxEntry {
                    access: FileSystemAccessMode::Deny,
                    path: FileSystemPath::Path {
                        path: std::path::Path::new(workspace_root.as_str())
                            .join(".git")
                            .to_string_lossy()
                            .into_owned(),
                    },
                },
            ]),
            ..AdditionalFileSystemPermissions::default()
        }),
        network: None,
    }
}

pub(super) struct ExactSessionIdentity {
    run_id: String,
    owner_user_id: String,
    device_id: String,
    workspace_id: String,
    plugin_id: String,
    release_id: String,
    artifact_sha256: String,
    component_key: String,
}

impl ExactSessionIdentity {
    pub(super) fn from_request(request: &RelayRequest) -> Result<Self, (u16, String)> {
        Ok(Self {
            run_id: required_body_text(&request.body, "run_id")?,
            owner_user_id: required_envelope_text(
                request.owner_user_id.as_deref(),
                "owner_user_id",
            )?,
            device_id: required_envelope_text(request.device_id.as_deref(), "device_id")?,
            workspace_id: request.workspace_id.trim().to_string(),
            plugin_id: required_body_text(&request.body, "plugin_id")?,
            release_id: required_body_text(&request.body, "release_id")?,
            artifact_sha256: required_sha256(&request.body, "artifact_sha256")?,
            component_key: required_body_text(&request.body, "component_key")?,
        })
    }

    pub(super) fn validate(&self, session: &PreparedPluginSession) -> Result<(), (u16, String)> {
        if self.run_id != session.run_id
            || self.owner_user_id != session.owner_user_id
            || self.device_id != session.device_id
            || self.workspace_id != session.workspace_id
            || self.plugin_id != session.plugin_id
            || self.release_id != session.release_id
            || self.artifact_sha256 != session.artifact_sha256
            || self.component_key != session.component_key
        {
            return Err((
                409,
                "Plugin request snapshot does not match the prepared session".to_string(),
            ));
        }
        Ok(())
    }
}

impl PreparedPluginSession {
    pub(super) fn telemetry_identity(&self) -> PluginRuntimeTelemetryIdentity {
        PluginRuntimeTelemetryIdentity {
            run_id: self.run_id.clone(),
            plugin_id: self.plugin_id.clone(),
            release_id: self.release_id.clone(),
            component_key: self.component_key.clone(),
        }
    }
}

pub(super) fn telemetry_identity_from_request(
    request: &RelayRequest,
) -> Result<PluginRuntimeTelemetryIdentity, (u16, String)> {
    Ok(PluginRuntimeTelemetryIdentity {
        run_id: required_body_text(&request.body, "run_id")?,
        plugin_id: required_body_text(&request.body, "plugin_id")?,
        release_id: required_body_text(&request.body, "release_id")?,
        component_key: required_body_text(&request.body, "component_key")?,
    })
}

pub(super) fn elapsed_millis(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

pub(super) fn artifact_ui_access_from_body(
    body: &Value,
) -> Result<PluginArtifactUiAccess, (u16, String)> {
    let access = PluginArtifactUiAccess {
        run_id: required_body_text(body, "run_id")?,
        plugin_id: required_body_text(body, "plugin_id")?,
        release_id: required_body_text(body, "release_id")?,
        artifact_sha256: required_sha256(body, "artifact_sha256")?,
        component_key: required_body_text(body, "component_key")?,
        adapter_session_id: required_body_text(body, "adapter_session_id")?,
        ui_snapshot_sha256: required_sha256(body, "ui_snapshot_sha256")?,
    };
    Ok(access)
}

pub(super) fn plugin_disabled_summary_sha256(
    installation: &crate::plugins::ActivePluginInstallation,
) -> String {
    hex::encode(Sha256::digest(
        format!(
            "chatos.plugin.disabled.v1\n{}\n{}\n{}",
            installation.plugin_id,
            installation.version.release_id,
            installation.version.artifact_sha256,
        )
        .as_bytes(),
    ))
}

pub(super) fn session_audit_hash(session: &PreparedPluginSession) -> String {
    let mut payload = format!(
        "chatos.plugin.session.v4\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
        session.run_id,
        session.owner_user_id,
        session.device_id,
        session.workspace_id,
        session.plugin_id,
        session.release_id,
        session.artifact_sha256,
        session.component_key,
    );
    for skill in session.skills.values() {
        payload.push('\n');
        payload.push_str(skill.snapshot_sha256.as_str());
    }
    for agent in session.agents.values() {
        payload.push('\n');
        payload.push_str(agent.snapshot_sha256.as_str());
    }
    for command in session.commands.values() {
        payload.push('\n');
        payload.push_str(command.snapshot_sha256.as_str());
    }
    for hook in session.hooks.values() {
        payload.push('\n');
        payload.push_str(hook.snapshot_sha256.as_str());
    }
    if let Some(ui) = &session.ui {
        payload.push('\n');
        payload.push_str(ui.snapshot_sha256.as_str());
    }
    if let Some(mcp) = &session.mcp {
        payload.push('\n');
        payload.push_str(mcp.snapshot().snapshot_sha256.as_str());
    }
    for permission in &session.permission_snapshot {
        payload.push('\n');
        payload.push_str(permission.as_str());
    }
    hex::encode(Sha256::digest(payload.as_bytes()))
}
