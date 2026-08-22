// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use anyhow::{anyhow, bail, Context, Result};
use chatos_plugin_management_sdk::{
    plugin_ui_snapshot_sha256, PluginArtifactDescriptor, PluginArtifactListResponse,
    PluginArtifactOwner, PluginArtifactReadMode, PluginArtifactReadResponse,
    PluginArtifactUiAccess, PluginArtifactWriteOperation, PluginArtifactWriteResponse,
    PluginUiSnapshot, PLUGIN_ARTIFACT_INLINE_READ_MAX_BYTES, PLUGIN_ARTIFACT_MAX_BYTES,
    PLUGIN_ARTIFACT_WRITE_MAX_BYTES, PLUGIN_UI_ASSET_MAX_BYTES,
    PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_CREATE, PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_UPDATE,
    PLUGIN_UI_BRIDGE_PROTOCOL_VERSION_V1, PLUGIN_UI_HOST_CSP_V1, PLUGIN_UI_IFRAME_SANDBOX_V1,
    PLUGIN_UI_MAX_ARTIFACT_MIME_TYPES, PLUGIN_UI_MAX_ASSETS, PLUGIN_UI_MAX_BRIDGE_CAPABILITIES,
    PLUGIN_UI_TOTAL_ASSET_MAX_BYTES,
};
use chrono::Utc;
use ring::hmac;
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;
use uuid::Uuid;

use crate::relay::RelayRequest;
use crate::secure_storage::SecureStorage;
use crate::workspace::paths::safe_workspace_path;
use crate::LocalState;

const MAX_REGISTERED_PLUGIN_ARTIFACTS: usize = 1_024;
const PLUGIN_ARTIFACT_WORKSPACE_DIRECTORY: &str = "chatos-plugin-artifacts";
const MAX_PERSISTED_PLUGIN_UI_GRANTS: usize = 1_024;
const PLUGIN_ARTIFACT_REGISTRY_SCHEMA_VERSION: u32 = 1;
const PLUGIN_ARTIFACT_REGISTRY_FILE_NAME: &str = "artifact-registry-v1.json";
const PLUGIN_ARTIFACT_REGISTRY_KEY_FILE_NAME: &str = "artifact-registry-v1.key";
const PLUGIN_ARTIFACT_REGISTRY_INTEGRITY_ALGORITHM: &str = "hmac-sha256";
const PLUGIN_ARTIFACT_REGISTRY_KEY_BYTES: usize = 32;
const PLUGIN_ARTIFACT_REGISTRY_MAX_BYTES: u64 = 8 * 1024 * 1024;
#[cfg(not(test))]
const PLUGIN_ARTIFACT_REGISTRY_KEY_SERVICE: &str =
    "Chat OS Local Connector Plugin Artifact Registry";
const PLUGIN_ARTIFACT_REGISTRY_MAC_PURPOSE: &[u8] = b"chatos.plugin.artifact.registry.v1\0";

mod grant;
mod persistence;
mod validation;

use grant::{ensure_active_grant, grant_can_retain_artifact};
use persistence::sync_registry_directory;

#[derive(Debug, Clone)]
pub(super) struct PluginArtifactStore {
    inner: Arc<Mutex<PluginArtifactStoreState>>,
    persistence: Option<PluginArtifactPersistence>,
    initialization_error: Option<Arc<str>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PluginArtifactStoreState {
    #[serde(default)]
    ui_grants: BTreeMap<String, PluginUiArtifactGrant>,
    #[serde(default)]
    artifacts: BTreeMap<String, RegisteredPluginArtifact>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PluginUiArtifactGrant {
    pub owner_user_id: String,
    pub device_id: String,
    pub workspace_id: String,
    pub run_id: String,
    pub plugin_id: String,
    pub release_id: String,
    pub artifact_sha256: String,
    pub component_key: String,
    pub adapter_session_id: String,
    pub ui: PluginUiSnapshot,
    pub permission_snapshot: BTreeSet<String>,
    pub expires_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RegisteredPluginArtifact {
    descriptor: PluginArtifactDescriptor,
    created_at_epoch_seconds: i64,
}

#[derive(Clone)]
struct PluginArtifactPersistence {
    registry_path: PathBuf,
    integrity_key: Arc<[u8]>,
}

impl fmt::Debug for PluginArtifactPersistence {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PluginArtifactPersistence")
            .field("registry_path", &self.registry_path)
            .field("integrity_key", &"[REDACTED]")
            .finish()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PersistedPluginArtifactRegistry {
    schema_version: u32,
    integrity_algorithm: String,
    integrity_tag: String,
    state: PluginArtifactStoreState,
}

impl Default for PluginArtifactStore {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(PluginArtifactStoreState::default())),
            persistence: None,
            initialization_error: None,
        }
    }
}

impl PluginArtifactStore {
    #[cfg(not(test))]
    pub(super) fn for_state_path(state_path: &Path) -> Self {
        Self::for_state_path_with_storage(
            state_path,
            SecureStorage::platform(PLUGIN_ARTIFACT_REGISTRY_KEY_SERVICE),
        )
    }

    pub(super) fn for_state_path_with_storage(state_path: &Path, storage: SecureStorage) -> Self {
        match PluginArtifactPersistence::open(state_path, &storage)
            .and_then(|persistence| persistence.load().map(|state| (persistence, state)))
        {
            Ok((persistence, state)) => Self {
                inner: Arc::new(Mutex::new(state)),
                persistence: Some(persistence),
                initialization_error: None,
            },
            Err(error) => Self {
                inner: Arc::new(Mutex::new(PluginArtifactStoreState::default())),
                persistence: None,
                initialization_error: Some(Arc::from(format!(
                    "Plugin Artifact registry initialization failed: {error:#}"
                ))),
            },
        }
    }

    pub fn register_ui_grant(&self, grant: PluginUiArtifactGrant) -> Result<()> {
        let mut state = self.lock()?;
        let previous = state.clone();
        prune_expired(&mut state);
        state
            .ui_grants
            .insert(grant.adapter_session_id.clone(), grant);
        prune_ui_grant_capacity(&mut state);
        if let Err(error) = self.persist(&state) {
            *state = previous;
            return Err(error);
        }
        Ok(())
    }

    pub fn ui_grant(
        &self,
        request: &RelayRequest,
        access: &PluginArtifactUiAccess,
        capability: &str,
    ) -> Result<PluginUiArtifactGrant, (u16, String)> {
        let mut state = self.lock().map_err(internal_error)?;
        prune_expired(&mut state);
        let grant = state
            .ui_grants
            .get(access.adapter_session_id.as_str())
            .cloned()
            .ok_or_else(|| (404, "Plugin UI Artifact session is unavailable".to_string()))?;
        grant.validate_request(request, access, capability)?;
        Ok(grant)
    }

    pub fn list(
        &self,
        grant: &PluginUiArtifactGrant,
        access: PluginArtifactUiAccess,
    ) -> Result<PluginArtifactListResponse, (u16, String)> {
        let mut state = self.lock().map_err(internal_error)?;
        prune_expired(&mut state);
        let mut artifacts = state
            .artifacts
            .values()
            .filter(|artifact| grant.can_access(&artifact.descriptor))
            .map(|artifact| artifact.descriptor.clone())
            .collect::<Vec<_>>();
        artifacts.sort_by(|left, right| {
            right
                .created_at
                .cmp(&left.created_at)
                .then_with(|| left.artifact_id.cmp(&right.artifact_id))
        });
        Ok(PluginArtifactListResponse { access, artifacts })
    }

    pub fn read(
        &self,
        state_snapshot: &LocalState,
        request: &RelayRequest,
        grant: &PluginUiArtifactGrant,
        access: PluginArtifactUiAccess,
        artifact_id: &str,
        mode: PluginArtifactReadMode,
    ) -> Result<PluginArtifactReadResponse, (u16, String)> {
        let artifact = {
            let mut state = self.lock().map_err(internal_error)?;
            prune_expired(&mut state);
            state
                .artifacts
                .get(artifact_id)
                .cloned()
                .ok_or_else(|| (404, "Plugin Artifact does not exist".to_string()))?
        };
        if !grant.can_access(&artifact.descriptor) {
            return Err((404, "Plugin Artifact does not exist".to_string()));
        }
        if mode == PluginArtifactReadMode::Inline
            && artifact.descriptor.size_bytes > PLUGIN_ARTIFACT_INLINE_READ_MAX_BYTES
        {
            return Err((
                413,
                "Plugin Artifact is too large for inline reading".to_string(),
            ));
        }
        let (absolute_path, relative_path) = safe_workspace_path(
            state_snapshot,
            request,
            artifact.descriptor.workspace_relative_path.as_str(),
        )
        .map_err(conflict_error)?;
        if relative_path != artifact.descriptor.workspace_relative_path {
            return Err((
                409,
                "Plugin Artifact path no longer matches its registration".to_string(),
            ));
        }
        validate_regular_workspace_file(
            state_snapshot,
            request,
            absolute_path.as_path(),
            relative_path.as_str(),
        )
        .map_err(conflict_error)?;
        let bytes = fs::read(absolute_path.as_path()).map_err(|error| {
            (
                409,
                format!("read registered Plugin Artifact failed: {error}"),
            )
        })?;
        if u64::try_from(bytes.len()).ok() != Some(artifact.descriptor.size_bytes)
            || hex::encode(Sha256::digest(bytes.as_slice())) != artifact.descriptor.sha256
            || artifact_media_type(absolute_path.as_path())
                != Some(artifact.descriptor.media_type.as_str())
        {
            return Err((
                409,
                "Plugin Artifact changed after registration".to_string(),
            ));
        }
        Ok(PluginArtifactReadResponse {
            access,
            artifact: artifact.descriptor,
            body_base64: base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes),
        })
    }

    pub fn create(
        &self,
        state_snapshot: &LocalState,
        request: &RelayRequest,
        grant: &PluginUiArtifactGrant,
        access: PluginArtifactUiAccess,
        display_name: &str,
        media_type: &str,
        bytes: &[u8],
    ) -> Result<PluginArtifactWriteResponse, (u16, String)> {
        validation::validate_write_body(bytes).map_err(bad_request_error)?;
        validation::validate_artifact_display_name(display_name, media_type)
            .map_err(bad_request_error)?;
        if !grant
            .ui
            .artifact_mime_types
            .iter()
            .any(|allowed| allowed == media_type)
        {
            return Err((
                403,
                "Plugin UI is not allowed to create this Artifact MIME type".to_string(),
            ));
        }
        let mut state = self.lock().map_err(internal_error)?;
        let previous = state.clone();
        prune_expired(&mut state);
        ensure_active_grant(&state, grant)?;
        let artifact_id = format!("pa_{}", Uuid::new_v4().simple());
        let relative_path = validation::plugin_artifact_workspace_relative_path(
            grant,
            artifact_id.as_str(),
            display_name,
        );
        let absolute_path = validation::prepare_plugin_artifact_create_path(
            state_snapshot,
            request,
            relative_path.as_str(),
        )
        .map_err(conflict_error)?;
        validation::atomic_write_new(absolute_path.as_path(), bytes).map_err(conflict_error)?;
        let created_at = Utc::now();
        let created_at_epoch_seconds = created_at.timestamp();
        let descriptor = PluginArtifactDescriptor {
            artifact_id: artifact_id.clone(),
            owner: PluginArtifactOwner {
                owner_user_id: grant.owner_user_id.clone(),
                run_id: grant.run_id.clone(),
                device_id: grant.device_id.clone(),
                workspace_id: grant.workspace_id.clone(),
                plugin_id: grant.plugin_id.clone(),
                release_id: grant.release_id.clone(),
                artifact_sha256: grant.artifact_sha256.clone(),
                component_key: grant.component_key.clone(),
                adapter_session_id: grant.adapter_session_id.clone(),
            },
            workspace_relative_path: relative_path,
            display_name: display_name.to_string(),
            media_type: media_type.to_string(),
            size_bytes: bytes.len() as u64,
            sha256: hex::encode(Sha256::digest(bytes)),
            created_at: created_at.to_rfc3339(),
            producer_tool_name: PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_CREATE.to_string(),
            downloadable: true,
            mutable: true,
        };
        state.artifacts.insert(
            artifact_id,
            RegisteredPluginArtifact {
                descriptor: descriptor.clone(),
                created_at_epoch_seconds,
            },
        );
        prune_artifact_capacity(&mut state);
        if let Err(error) = self.persist(&state) {
            *state = previous;
            let _ = fs::remove_file(absolute_path.as_path());
            return Err(internal_error(error));
        }
        Ok(PluginArtifactWriteResponse {
            access,
            operation: PluginArtifactWriteOperation::Create,
            artifact: descriptor,
        })
    }

    pub fn update(
        &self,
        state_snapshot: &LocalState,
        request: &RelayRequest,
        grant: &PluginUiArtifactGrant,
        access: PluginArtifactUiAccess,
        artifact_id: &str,
        expected_sha256: &str,
        bytes: &[u8],
    ) -> Result<PluginArtifactWriteResponse, (u16, String)> {
        validation::validate_write_body(bytes).map_err(bad_request_error)?;
        if !validation::is_plugin_artifact_id(artifact_id)
            || !validation::is_lower_sha256(expected_sha256)
        {
            return Err((400, "artifact_id or expected_sha256 is invalid".to_string()));
        }
        let mut state = self.lock().map_err(internal_error)?;
        let previous = state.clone();
        prune_expired(&mut state);
        ensure_active_grant(&state, grant)?;
        let registered = state
            .artifacts
            .get(artifact_id)
            .cloned()
            .ok_or_else(|| (404, "Plugin Artifact does not exist".to_string()))?;
        let descriptor = &registered.descriptor;
        if !grant.can_access(descriptor)
            || !descriptor.mutable
            || descriptor.owner.component_key != grant.component_key
            || descriptor.owner.adapter_session_id != grant.adapter_session_id
            || descriptor.sha256 != expected_sha256
        {
            return Err((
                409,
                "Plugin Artifact is not an exact mutable Artifact for this UI session".to_string(),
            ));
        }
        if !grant
            .ui
            .artifact_mime_types
            .iter()
            .any(|allowed| allowed == &descriptor.media_type)
        {
            return Err((
                403,
                "Plugin UI is not allowed to update this Artifact MIME type".to_string(),
            ));
        }
        let (absolute_path, relative_path) = safe_workspace_path(
            state_snapshot,
            request,
            descriptor.workspace_relative_path.as_str(),
        )
        .map_err(conflict_error)?;
        if relative_path != descriptor.workspace_relative_path {
            return Err((
                409,
                "Plugin Artifact path no longer matches its registration".to_string(),
            ));
        }
        validate_regular_workspace_file(
            state_snapshot,
            request,
            absolute_path.as_path(),
            relative_path.as_str(),
        )
        .map_err(conflict_error)?;
        let previous_bytes = fs::read(absolute_path.as_path())
            .map_err(|error| (409, format!("read mutable Plugin Artifact failed: {error}")))?;
        if u64::try_from(previous_bytes.len()).ok() != Some(descriptor.size_bytes)
            || hex::encode(Sha256::digest(previous_bytes.as_slice())) != descriptor.sha256
            || artifact_media_type(absolute_path.as_path()) != Some(descriptor.media_type.as_str())
        {
            return Err((
                409,
                "Plugin Artifact changed before the requested update".to_string(),
            ));
        }
        validation::atomic_replace(absolute_path.as_path(), bytes).map_err(conflict_error)?;
        let mut updated = registered;
        updated.descriptor.size_bytes = bytes.len() as u64;
        updated.descriptor.sha256 = hex::encode(Sha256::digest(bytes));
        updated.descriptor.producer_tool_name =
            PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_UPDATE.to_string();
        state
            .artifacts
            .insert(artifact_id.to_string(), updated.clone());
        if let Err(error) = self.persist(&state) {
            *state = previous;
            let restore_error =
                validation::atomic_replace(absolute_path.as_path(), previous_bytes.as_slice())
                    .err()
                    .map(|restore| format!("; restoring the previous file failed: {restore:#}"))
                    .unwrap_or_default();
            return Err((
                500,
                format!("persist Plugin Artifact update failed: {error:#}{restore_error}"),
            ));
        }
        Ok(PluginArtifactWriteResponse {
            access,
            operation: PluginArtifactWriteOperation::Update,
            artifact: updated.descriptor,
        })
    }

    fn lock(&self) -> Result<MutexGuard<'_, PluginArtifactStoreState>> {
        if let Some(error) = &self.initialization_error {
            bail!(error.to_string());
        }
        self.inner
            .lock()
            .map_err(|_| anyhow!("Plugin Artifact store lock is poisoned"))
    }

    fn persist(&self, state: &PluginArtifactStoreState) -> Result<()> {
        if let Some(persistence) = &self.persistence {
            persistence.save(state)?;
        }
        Ok(())
    }
}

fn validate_regular_workspace_file(
    state: &LocalState,
    request: &RelayRequest,
    absolute_path: &Path,
    relative_path: &str,
) -> Result<()> {
    let workspace = state
        .workspace_by_id(request.workspace_id.trim())
        .ok_or_else(|| anyhow!("Plugin Artifact workspace is not registered"))?;
    let root = workspace
        .absolute_root
        .canonicalize()
        .context("resolve Plugin Artifact workspace root")?;
    let relative = Path::new(relative_path);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(anyhow!("Plugin Artifact path is not workspace-relative"));
    }
    let mut cursor = root.clone();
    for component in relative.components() {
        let Component::Normal(component) = component else {
            return Err(anyhow!("Plugin Artifact path is invalid"));
        };
        cursor.push(component);
        let metadata = fs::symlink_metadata(cursor.as_path())
            .with_context(|| format!("inspect Plugin Artifact path: {relative_path}"))?;
        if metadata.file_type().is_symlink() {
            return Err(anyhow!("Plugin Artifact path contains a symbolic link"));
        }
    }
    if cursor != absolute_path || !cursor.is_file() {
        return Err(anyhow!("Plugin Artifact output is not a regular file"));
    }
    let canonical = cursor
        .canonicalize()
        .with_context(|| format!("resolve Plugin Artifact path: {relative_path}"))?;
    if !canonical.starts_with(root.as_path()) {
        return Err(anyhow!("Plugin Artifact path escapes the workspace"));
    }
    Ok(())
}

fn artifact_media_type(path: &Path) -> Option<&'static str> {
    match path
        .extension()
        .and_then(|value| value.to_str())?
        .to_ascii_lowercase()
        .as_str()
    {
        "pdf" => Some("application/pdf"),
        "docx" => Some("application/vnd.openxmlformats-officedocument.wordprocessingml.document"),
        "xlsx" => Some("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
        "pptx" => Some("application/vnd.openxmlformats-officedocument.presentationml.presentation"),
        "csv" => Some("text/csv"),
        "txt" => Some("text/plain"),
        "json" => Some("application/json"),
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        _ => None,
    }
}

fn prune_expired(state: &mut PluginArtifactStoreState) {
    let now = Utc::now().timestamp();
    state.ui_grants.retain(|_, grant| grant.expires_at > now);
    state.artifacts.retain(|_, artifact| {
        state
            .ui_grants
            .values()
            .any(|grant| grant_can_retain_artifact(grant, &artifact.descriptor))
    });
}

fn prune_ui_grant_capacity(state: &mut PluginArtifactStoreState) {
    let remove = state
        .ui_grants
        .len()
        .saturating_sub(MAX_PERSISTED_PLUGIN_UI_GRANTS);
    if remove == 0 {
        return;
    }
    let mut oldest = state
        .ui_grants
        .iter()
        .map(|(adapter_session_id, grant)| (grant.expires_at, adapter_session_id.clone()))
        .collect::<Vec<_>>();
    oldest.sort();
    for (_, adapter_session_id) in oldest.into_iter().take(remove) {
        state.ui_grants.remove(adapter_session_id.as_str());
    }
    state.artifacts.retain(|_, artifact| {
        state
            .ui_grants
            .values()
            .any(|grant| grant_can_retain_artifact(grant, &artifact.descriptor))
    });
}

fn prune_artifact_capacity(state: &mut PluginArtifactStoreState) {
    let remove = state
        .artifacts
        .len()
        .saturating_sub(MAX_REGISTERED_PLUGIN_ARTIFACTS);
    if remove == 0 {
        return;
    }
    let mut oldest = state
        .artifacts
        .iter()
        .map(|(artifact_id, artifact)| (artifact.created_at_epoch_seconds, artifact_id.clone()))
        .collect::<Vec<_>>();
    oldest.sort();
    for (_, artifact_id) in oldest.into_iter().take(remove) {
        state.artifacts.remove(artifact_id.as_str());
    }
}

fn conflict_error(error: anyhow::Error) -> (u16, String) {
    (409, error.to_string())
}

fn bad_request_error(error: anyhow::Error) -> (u16, String) {
    (400, error.to_string())
}

fn internal_error(error: anyhow::Error) -> (u16, String) {
    (500, error.to_string())
}

#[cfg(test)]
mod tests;
