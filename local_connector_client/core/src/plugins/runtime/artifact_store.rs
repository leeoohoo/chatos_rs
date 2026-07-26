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
    PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_CREATE, PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_DOWNLOAD,
    PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_LIST, PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_READ,
    PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_UPDATE, PLUGIN_UI_BRIDGE_PROTOCOL_VERSION_V1,
    PLUGIN_UI_HOST_CSP_V1, PLUGIN_UI_IFRAME_SANDBOX_V1, PLUGIN_UI_MAX_ARTIFACT_MIME_TYPES,
    PLUGIN_UI_MAX_ASSETS, PLUGIN_UI_MAX_BRIDGE_CAPABILITIES, PLUGIN_UI_TOTAL_ASSET_MAX_BYTES,
};
use chrono::Utc;
use ring::hmac;
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;
use uuid::Uuid;

use crate::relay::RelayRequest;
use crate::secure_storage::SecureStorage;
use crate::skills::native::safe_workspace_path;
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

#[derive(Debug, Clone)]
pub(super) struct PluginArtifactProducer<'a> {
    pub owner_user_id: &'a str,
    pub device_id: &'a str,
    pub workspace_id: &'a str,
    pub run_id: &'a str,
    pub plugin_id: &'a str,
    pub release_id: &'a str,
    pub artifact_sha256: &'a str,
    pub component_key: &'a str,
    pub adapter_session_id: &'a str,
    pub skill_id: &'a str,
    pub tool_name: &'a str,
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

    pub fn register_native_outputs(
        &self,
        state_snapshot: &LocalState,
        request: &RelayRequest,
        producer: PluginArtifactProducer<'_>,
        arguments: &Value,
        result: &Value,
    ) -> Result<Vec<PluginArtifactDescriptor>> {
        if !is_artifact_skill(producer.skill_id) {
            return Ok(Vec::new());
        }
        let candidates = output_candidates(arguments);
        if candidates.is_empty() {
            return Ok(Vec::new());
        }

        let mut store = self.lock()?;
        let previous = store.clone();
        prune_expired(&mut store);
        let allowed_media_types = store
            .ui_grants
            .values()
            .filter(|grant| grant.matches_producer(&producer))
            .filter(|grant| grant.allows_any_artifact_read())
            .flat_map(|grant| grant.ui.artifact_mime_types.iter().cloned())
            .collect::<BTreeSet<_>>();
        if allowed_media_types.is_empty() {
            return Ok(Vec::new());
        }

        let mut registered = Vec::new();
        let mut seen_paths = BTreeSet::new();
        for requested_path in candidates {
            let (absolute_path, relative_path) =
                safe_workspace_path(state_snapshot, request, requested_path.as_str())?;
            if !seen_paths.insert(relative_path.clone())
                || !result_contains_string(result, relative_path.as_str())
            {
                continue;
            }
            validate_regular_workspace_file(
                state_snapshot,
                request,
                absolute_path.as_path(),
                relative_path.as_str(),
            )?;
            let media_type = artifact_media_type(absolute_path.as_path())
                .ok_or_else(|| anyhow!("Plugin Artifact output type is unsupported"))?;
            if !allowed_media_types.contains(media_type) {
                continue;
            }
            let metadata = fs::metadata(absolute_path.as_path())
                .with_context(|| format!("read Plugin Artifact metadata: {relative_path}"))?;
            if metadata.len() > PLUGIN_ARTIFACT_MAX_BYTES {
                return Err(anyhow!("Plugin Artifact exceeds the local size limit"));
            }
            let bytes = fs::read(absolute_path.as_path())
                .with_context(|| format!("read Plugin Artifact: {relative_path}"))?;
            if u64::try_from(bytes.len()).ok() != Some(metadata.len()) {
                return Err(anyhow!(
                    "Plugin Artifact changed while it was being registered"
                ));
            }
            let created_at = Utc::now();
            let created_at_epoch_seconds = created_at.timestamp();
            let descriptor = PluginArtifactDescriptor {
                artifact_id: format!("pa_{}", Uuid::new_v4().simple()),
                owner: PluginArtifactOwner {
                    owner_user_id: producer.owner_user_id.to_string(),
                    run_id: producer.run_id.to_string(),
                    device_id: producer.device_id.to_string(),
                    workspace_id: producer.workspace_id.to_string(),
                    plugin_id: producer.plugin_id.to_string(),
                    release_id: producer.release_id.to_string(),
                    artifact_sha256: producer.artifact_sha256.to_string(),
                    component_key: producer.component_key.to_string(),
                    adapter_session_id: producer.adapter_session_id.to_string(),
                },
                workspace_relative_path: relative_path.clone(),
                display_name: Path::new(relative_path.as_str())
                    .file_name()
                    .and_then(|value| value.to_str())
                    .ok_or_else(|| anyhow!("Plugin Artifact filename is not valid UTF-8"))?
                    .to_string(),
                media_type: media_type.to_string(),
                size_bytes: metadata.len(),
                sha256: hex::encode(Sha256::digest(bytes.as_slice())),
                created_at: created_at.to_rfc3339(),
                producer_tool_name: producer.tool_name.to_string(),
                downloadable: true,
                mutable: false,
            };
            store.artifacts.insert(
                descriptor.artifact_id.clone(),
                RegisteredPluginArtifact {
                    descriptor: descriptor.clone(),
                    created_at_epoch_seconds,
                },
            );
            registered.push(descriptor);
        }
        prune_artifact_capacity(&mut store);
        if !registered.is_empty() {
            if let Err(error) = self.persist(&store) {
                *store = previous;
                return Err(error);
            }
        }
        Ok(registered)
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
        validate_write_body(bytes).map_err(bad_request_error)?;
        validate_artifact_display_name(display_name, media_type).map_err(bad_request_error)?;
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
        let relative_path =
            plugin_artifact_workspace_relative_path(grant, artifact_id.as_str(), display_name);
        let absolute_path =
            prepare_plugin_artifact_create_path(state_snapshot, request, relative_path.as_str())
                .map_err(conflict_error)?;
        atomic_write_new(absolute_path.as_path(), bytes).map_err(conflict_error)?;
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
        validate_write_body(bytes).map_err(bad_request_error)?;
        if !is_plugin_artifact_id(artifact_id) || !is_lower_sha256(expected_sha256) {
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
        atomic_replace(absolute_path.as_path(), bytes).map_err(conflict_error)?;
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
            let restore_error = atomic_replace(absolute_path.as_path(), previous_bytes.as_slice())
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

impl PluginArtifactPersistence {
    fn open(state_path: &Path, storage: &SecureStorage) -> Result<Self> {
        let app_data = state_path.parent().unwrap_or_else(|| Path::new("."));
        let registry_directory = app_data.join("plugins");
        let registry_path = registry_directory.join(PLUGIN_ARTIFACT_REGISTRY_FILE_NAME);
        let integrity_key_path = registry_directory.join(PLUGIN_ARTIFACT_REGISTRY_KEY_FILE_NAME);
        let account = plugin_artifact_registry_key_account(state_path);
        let registry_exists = match fs::symlink_metadata(registry_path.as_path()) {
            Ok(_) => true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "inspect Plugin Artifact registry {}",
                        registry_path.display()
                    )
                });
            }
        };
        let integrity_key = match storage
            .load(account.as_str(), integrity_key_path.as_path())
            .context("load Plugin Artifact registry integrity key")?
        {
            Some(key) => key,
            None if registry_exists => {
                bail!("Plugin Artifact registry integrity key is unavailable")
            }
            None => {
                let mut key = vec![0_u8; PLUGIN_ARTIFACT_REGISTRY_KEY_BYTES];
                SystemRandom::new()
                    .fill(key.as_mut_slice())
                    .map_err(|_| anyhow!("generate Plugin Artifact registry integrity key"))?;
                storage
                    .save(
                        account.as_str(),
                        integrity_key_path.as_path(),
                        key.as_slice(),
                    )
                    .context("save Plugin Artifact registry integrity key")?;
                key
            }
        };
        if integrity_key.len() != PLUGIN_ARTIFACT_REGISTRY_KEY_BYTES {
            bail!("Plugin Artifact registry integrity key has an invalid length");
        }
        Ok(Self {
            registry_path,
            integrity_key: Arc::from(integrity_key),
        })
    }

    fn load(&self) -> Result<PluginArtifactStoreState> {
        let metadata = match fs::symlink_metadata(self.registry_path.as_path()) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(PluginArtifactStoreState::default());
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "inspect Plugin Artifact registry {}",
                        self.registry_path.display()
                    )
                });
            }
        };
        if metadata.file_type().is_symlink()
            || !metadata.is_file()
            || metadata.len() > PLUGIN_ARTIFACT_REGISTRY_MAX_BYTES
        {
            bail!("Plugin Artifact registry is not a safe bounded regular file");
        }
        let bytes = fs::read(self.registry_path.as_path()).with_context(|| {
            format!(
                "read Plugin Artifact registry {}",
                self.registry_path.display()
            )
        })?;
        if u64::try_from(bytes.len()).ok() != Some(metadata.len()) {
            bail!("Plugin Artifact registry changed while it was being read");
        }
        let persisted = serde_json::from_slice::<PersistedPluginArtifactRegistry>(bytes.as_slice())
            .context("parse Plugin Artifact registry")?;
        if persisted.schema_version != PLUGIN_ARTIFACT_REGISTRY_SCHEMA_VERSION
            || persisted.integrity_algorithm != PLUGIN_ARTIFACT_REGISTRY_INTEGRITY_ALGORITHM
            || !is_lower_sha256(persisted.integrity_tag.as_str())
        {
            bail!("Plugin Artifact registry envelope is invalid");
        }
        let mac_input = plugin_artifact_registry_mac_input(&persisted.state)?;
        hmac::verify(
            &hmac::Key::new(hmac::HMAC_SHA256, self.integrity_key.as_ref()),
            mac_input.as_slice(),
            hex::decode(persisted.integrity_tag.as_str())
                .context("decode Plugin Artifact registry integrity tag")?
                .as_slice(),
        )
        .map_err(|_| anyhow!("Plugin Artifact registry integrity verification failed"))?;
        let mut state = persisted.state;
        validate_persisted_state(&mut state)?;
        Ok(state)
    }

    fn save(&self, state: &PluginArtifactStoreState) -> Result<()> {
        let mut state = state.clone();
        validate_persisted_state(&mut state)?;
        let mac_input = plugin_artifact_registry_mac_input(&state)?;
        let integrity_tag = hex::encode(
            hmac::sign(
                &hmac::Key::new(hmac::HMAC_SHA256, self.integrity_key.as_ref()),
                mac_input.as_slice(),
            )
            .as_ref(),
        );
        let persisted = PersistedPluginArtifactRegistry {
            schema_version: PLUGIN_ARTIFACT_REGISTRY_SCHEMA_VERSION,
            integrity_algorithm: PLUGIN_ARTIFACT_REGISTRY_INTEGRITY_ALGORITHM.to_string(),
            integrity_tag,
            state,
        };
        let payload =
            serde_json::to_vec_pretty(&persisted).context("serialize Plugin Artifact registry")?;
        if payload.len() as u64 > PLUGIN_ARTIFACT_REGISTRY_MAX_BYTES {
            bail!("Plugin Artifact registry exceeds the local size limit");
        }
        let parent = self
            .registry_path
            .parent()
            .context("Plugin Artifact registry has no parent directory")?;
        ensure_safe_registry_directory(parent)?;
        if let Ok(metadata) = fs::symlink_metadata(self.registry_path.as_path()) {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                bail!("Plugin Artifact registry target is not a safe regular file");
            }
        }
        let mut temporary =
            NamedTempFile::new_in(parent).context("create temporary Plugin Artifact registry")?;
        temporary
            .write_all(payload.as_slice())
            .context("write temporary Plugin Artifact registry")?;
        temporary
            .as_file()
            .sync_all()
            .context("sync temporary Plugin Artifact registry")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            temporary
                .as_file()
                .set_permissions(fs::Permissions::from_mode(0o600))
                .context("restrict temporary Plugin Artifact registry")?;
        }
        temporary
            .persist(self.registry_path.as_path())
            .map_err(|error| error.error)
            .context("atomically replace Plugin Artifact registry")?;
        sync_registry_directory(parent)?;
        Ok(())
    }
}

fn plugin_artifact_registry_key_account(state_path: &Path) -> String {
    let digest = Sha256::digest(state_path.to_string_lossy().as_bytes());
    format!("chatos-plugin-artifact-registry-{}", hex::encode(digest))
}

fn plugin_artifact_registry_mac_input(state: &PluginArtifactStoreState) -> Result<Vec<u8>> {
    let state =
        serde_json::to_vec(state).context("serialize Plugin Artifact registry MAC input")?;
    let mut input = Vec::with_capacity(
        PLUGIN_ARTIFACT_REGISTRY_MAC_PURPOSE
            .len()
            .saturating_add(state.len()),
    );
    input.extend_from_slice(PLUGIN_ARTIFACT_REGISTRY_MAC_PURPOSE);
    input.extend_from_slice(state.as_slice());
    Ok(input)
}

fn ensure_safe_registry_directory(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                bail!("Plugin Artifact registry directory is not a safe directory");
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(path).with_context(|| {
                format!(
                    "create Plugin Artifact registry directory: {}",
                    path.display()
                )
            })?;
            let metadata = fs::symlink_metadata(path).with_context(|| {
                format!(
                    "inspect Plugin Artifact registry directory: {}",
                    path.display()
                )
            })?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                bail!("Plugin Artifact registry directory is not a safe directory");
            }
        }
        Err(error) => {
            return Err(error).with_context(|| {
                format!(
                    "inspect Plugin Artifact registry directory: {}",
                    path.display()
                )
            });
        }
    }
    Ok(())
}

#[cfg(unix)]
fn sync_registry_directory(path: &Path) -> Result<()> {
    File::open(path)
        .with_context(|| {
            format!(
                "open Plugin Artifact registry directory: {}",
                path.display()
            )
        })?
        .sync_all()
        .with_context(|| {
            format!(
                "sync Plugin Artifact registry directory: {}",
                path.display()
            )
        })
}

#[cfg(not(unix))]
fn sync_registry_directory(_path: &Path) -> Result<()> {
    Ok(())
}

impl PluginUiArtifactGrant {
    fn validate_request(
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

    fn matches_producer(&self, producer: &PluginArtifactProducer<'_>) -> bool {
        self.owner_user_id == producer.owner_user_id
            && self.device_id == producer.device_id
            && self.workspace_id == producer.workspace_id
            && self.run_id == producer.run_id
            && self.plugin_id == producer.plugin_id
            && self.release_id == producer.release_id
            && self.artifact_sha256 == producer.artifact_sha256
            && self.expires_at > Utc::now().timestamp()
    }

    fn allows_any_artifact_read(&self) -> bool {
        self.ui.bridge_capabilities.iter().any(|capability| {
            matches!(
                capability.as_str(),
                PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_LIST
                    | PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_READ
                    | PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_DOWNLOAD
            )
        })
    }

    fn can_access(&self, artifact: &PluginArtifactDescriptor) -> bool {
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

fn ensure_active_grant(
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

fn grant_can_retain_artifact(
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

fn validate_write_body(bytes: &[u8]) -> Result<()> {
    if bytes.len() as u64 > PLUGIN_ARTIFACT_WRITE_MAX_BYTES {
        bail!("Plugin Artifact write body exceeds the local size limit");
    }
    Ok(())
}

fn validate_artifact_display_name(display_name: &str, media_type: &str) -> Result<()> {
    let path = Path::new(display_name);
    if display_name.trim() != display_name
        || display_name.is_empty()
        || display_name.len() > 512
        || display_name.contains('/')
        || display_name.contains('\\')
        || display_name.chars().any(char::is_control)
        || path.components().count() != 1
        || !matches!(path.components().next(), Some(Component::Normal(_)))
        || artifact_media_type(path) != Some(media_type)
    {
        bail!("Plugin Artifact display name or MIME type is invalid");
    }
    Ok(())
}

fn plugin_artifact_workspace_relative_path(
    grant: &PluginUiArtifactGrant,
    artifact_id: &str,
    display_name: &str,
) -> String {
    let mut identity = Sha256::new();
    identity.update(b"chatos.plugin.artifact.workspace.v1\0");
    for value in [
        grant.owner_user_id.as_str(),
        grant.device_id.as_str(),
        grant.workspace_id.as_str(),
        grant.run_id.as_str(),
        grant.plugin_id.as_str(),
        grant.release_id.as_str(),
        grant.component_key.as_str(),
        grant.adapter_session_id.as_str(),
    ] {
        identity.update((value.len() as u64).to_be_bytes());
        identity.update(value.as_bytes());
    }
    let identity = hex::encode(identity.finalize());
    format!(
        "{PLUGIN_ARTIFACT_WORKSPACE_DIRECTORY}/{}/{artifact_id}/{display_name}",
        &identity[..32]
    )
}

fn prepare_plugin_artifact_create_path(
    state: &LocalState,
    request: &RelayRequest,
    relative_path: &str,
) -> Result<PathBuf> {
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
        bail!("Plugin Artifact create path is not workspace-relative");
    }
    let parent = relative
        .parent()
        .context("Plugin Artifact create path has no parent")?;
    let mut cursor = root.clone();
    for component in parent.components() {
        let Component::Normal(component) = component else {
            bail!("Plugin Artifact create directory is invalid");
        };
        cursor.push(component);
        match fs::symlink_metadata(cursor.as_path()) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    bail!("Plugin Artifact create path contains an unsafe directory");
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(cursor.as_path()).with_context(|| {
                    format!("create Plugin Artifact directory: {}", cursor.display())
                })?;
                let metadata = fs::symlink_metadata(cursor.as_path()).with_context(|| {
                    format!("inspect Plugin Artifact directory: {}", cursor.display())
                })?;
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    bail!("Plugin Artifact create directory is unsafe");
                }
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("inspect Plugin Artifact directory: {}", cursor.display())
                });
            }
        }
    }
    let target = root.join(relative);
    match fs::symlink_metadata(target.as_path()) {
        Ok(_) => bail!("Plugin Artifact create target already exists"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(target),
        Err(error) => Err(error).context("inspect Plugin Artifact create target"),
    }
}

fn atomic_write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .context("Plugin Artifact target has no parent directory")?;
    let mut temporary =
        NamedTempFile::new_in(parent).context("create temporary Plugin Artifact file")?;
    temporary
        .write_all(bytes)
        .context("write temporary Plugin Artifact file")?;
    temporary
        .as_file()
        .sync_all()
        .context("sync temporary Plugin Artifact file")?;
    temporary
        .persist_noclobber(path)
        .map_err(|error| error.error)
        .context("persist new Plugin Artifact file")?;
    sync_registry_directory(parent)?;
    Ok(())
}

fn atomic_replace(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .context("Plugin Artifact target has no parent directory")?;
    let mut temporary =
        NamedTempFile::new_in(parent).context("create replacement Plugin Artifact file")?;
    temporary
        .write_all(bytes)
        .context("write replacement Plugin Artifact file")?;
    temporary
        .as_file()
        .sync_all()
        .context("sync replacement Plugin Artifact file")?;
    temporary
        .persist(path)
        .map_err(|error| error.error)
        .context("atomically replace Plugin Artifact file")?;
    sync_registry_directory(parent)?;
    Ok(())
}

fn validate_persisted_state(state: &mut PluginArtifactStoreState) -> Result<()> {
    if state.ui_grants.len() > MAX_PERSISTED_PLUGIN_UI_GRANTS
        || state.artifacts.len() > MAX_REGISTERED_PLUGIN_ARTIFACTS
    {
        bail!("Plugin Artifact registry exceeds its item limit");
    }
    prune_expired(state);
    for (adapter_session_id, grant) in &state.ui_grants {
        validate_persisted_grant(adapter_session_id.as_str(), grant)?;
    }
    for (artifact_id, artifact) in &state.artifacts {
        validate_persisted_artifact(artifact_id.as_str(), artifact)?;
        let matching_grants = state
            .ui_grants
            .values()
            .filter(|grant| grant_can_retain_artifact(grant, &artifact.descriptor))
            .collect::<Vec<_>>();
        if matching_grants.is_empty() {
            bail!("Plugin Artifact registry contains an Artifact without an active UI grant");
        }
        if artifact.descriptor.mutable
            && !matching_grants.iter().any(|grant| {
                artifact.descriptor.owner.component_key == grant.component_key
                    && artifact.descriptor.owner.adapter_session_id == grant.adapter_session_id
                    && grant.ui.bridge_capabilities.iter().any(|capability| {
                        capability == artifact.descriptor.producer_tool_name.as_str()
                    })
            })
        {
            bail!("Plugin Artifact registry mutable Artifact has no exact UI write grant");
        }
    }
    Ok(())
}

fn validate_persisted_grant(adapter_session_id: &str, grant: &PluginUiArtifactGrant) -> Result<()> {
    validate_bounded_identity("owner user id", grant.owner_user_id.as_str(), 256)?;
    validate_bounded_identity("device id", grant.device_id.as_str(), 256)?;
    validate_bounded_identity("workspace id", grant.workspace_id.as_str(), 256)?;
    validate_bounded_identity("Run id", grant.run_id.as_str(), 256)?;
    validate_bounded_identity("Plugin id", grant.plugin_id.as_str(), 256)?;
    validate_bounded_identity("Plugin Release id", grant.release_id.as_str(), 256)?;
    validate_bounded_identity("Plugin component key", grant.component_key.as_str(), 256)?;
    validate_bounded_identity("adapter session id", grant.adapter_session_id.as_str(), 256)?;
    if adapter_session_id != grant.adapter_session_id {
        bail!("Plugin Artifact registry UI grant key does not match its adapter session");
    }
    if !is_lower_sha256(grant.artifact_sha256.as_str())
        || grant.expires_at <= Utc::now().timestamp()
    {
        bail!("Plugin Artifact registry UI grant identity or expiry is invalid");
    }
    if grant.permission_snapshot.len() > 256 {
        bail!("Plugin Artifact registry UI permission snapshot exceeds its item limit");
    }
    for permission in &grant.permission_snapshot {
        validate_bounded_identity("Plugin permission", permission.as_str(), 256)?;
    }
    validate_persisted_ui_snapshot(grant)
}

fn validate_persisted_ui_snapshot(grant: &PluginUiArtifactGrant) -> Result<()> {
    let ui = &grant.ui;
    if ui.plugin_id != grant.plugin_id
        || ui.release_id != grant.release_id
        || ui.artifact_sha256 != grant.artifact_sha256
        || ui.component_key != grant.component_key
        || ui.bridge_protocol_version != PLUGIN_UI_BRIDGE_PROTOCOL_VERSION_V1
        || ui.content_security_policy != PLUGIN_UI_HOST_CSP_V1
        || ui.iframe_sandbox != PLUGIN_UI_IFRAME_SANDBOX_V1
        || !is_lower_sha256(ui.content_sha256.as_str())
        || !is_lower_sha256(ui.snapshot_sha256.as_str())
    {
        bail!("Plugin Artifact registry UI snapshot identity is invalid");
    }
    for (label, value, limit) in [
        ("Plugin UI version", ui.version.as_str(), 128_usize),
        ("Plugin UI title", ui.title.as_str(), 512_usize),
        ("Plugin UI surface", ui.surface.as_str(), 64_usize),
        (
            "Plugin UI relative source path",
            ui.relative_source_path.as_str(),
            4_096_usize,
        ),
    ] {
        validate_bounded_identity(label, value, limit)?;
    }
    if ui.assets.len() > PLUGIN_UI_MAX_ASSETS
        || ui.bridge_capabilities.len() > PLUGIN_UI_MAX_BRIDGE_CAPABILITIES
        || ui.artifact_mime_types.len() > PLUGIN_UI_MAX_ARTIFACT_MIME_TYPES
    {
        bail!("Plugin Artifact registry UI snapshot exceeds its item limit");
    }
    let mut total_asset_bytes = 0_u64;
    let mut asset_paths = BTreeSet::new();
    for asset in &ui.assets {
        validate_bounded_identity(
            "Plugin UI asset relative path",
            asset.relative_path.as_str(),
            4_096,
        )?;
        validate_bounded_identity("Plugin UI asset media type", asset.media_type.as_str(), 256)?;
        if !asset_paths.insert(asset.relative_path.as_str())
            || asset.size_bytes > PLUGIN_UI_ASSET_MAX_BYTES
            || !is_lower_sha256(asset.sha256.as_str())
        {
            bail!("Plugin Artifact registry UI asset snapshot is invalid");
        }
        total_asset_bytes = total_asset_bytes
            .checked_add(asset.size_bytes)
            .context("Plugin UI asset size total overflow")?;
    }
    if total_asset_bytes > PLUGIN_UI_TOTAL_ASSET_MAX_BYTES {
        bail!("Plugin Artifact registry UI assets exceed the total size limit");
    }
    validate_unique_bounded_values(
        "Plugin UI bridge capability",
        ui.bridge_capabilities.as_slice(),
        256,
    )?;
    validate_unique_bounded_values(
        "Plugin UI Artifact MIME type",
        ui.artifact_mime_types.as_slice(),
        256,
    )?;
    let expected_snapshot_sha256 = plugin_ui_snapshot_sha256(
        ui.plugin_id.as_str(),
        ui.release_id.as_str(),
        ui.component_key.as_str(),
        ui.title.as_str(),
        ui.surface.as_str(),
        ui.relative_source_path.as_str(),
        ui.content_sha256.as_str(),
        ui.assets.as_slice(),
        ui.bridge_protocol_version,
        ui.bridge_capabilities.as_slice(),
        ui.artifact_mime_types.as_slice(),
        ui.content_security_policy.as_str(),
        ui.iframe_sandbox.as_str(),
    )
    .context("hash persisted Plugin UI snapshot")?;
    if expected_snapshot_sha256 != ui.snapshot_sha256 {
        bail!("Plugin Artifact registry UI snapshot hash does not match");
    }
    Ok(())
}

fn validate_persisted_artifact(
    artifact_id: &str,
    artifact: &RegisteredPluginArtifact,
) -> Result<()> {
    let descriptor = &artifact.descriptor;
    if artifact_id != descriptor.artifact_id
        || !is_plugin_artifact_id(artifact_id)
        || !descriptor.downloadable
        || descriptor.size_bytes > PLUGIN_ARTIFACT_MAX_BYTES
        || !is_lower_sha256(descriptor.sha256.as_str())
    {
        bail!("Plugin Artifact registry descriptor flags or identity are invalid");
    }
    if descriptor.mutable
        && (descriptor.size_bytes > PLUGIN_ARTIFACT_WRITE_MAX_BYTES
            || !matches!(
                descriptor.producer_tool_name.as_str(),
                PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_CREATE
                    | PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_UPDATE
            ))
    {
        bail!("Plugin Artifact registry mutable descriptor is invalid");
    }
    for (label, value, limit) in [
        (
            "Artifact owner user id",
            descriptor.owner.owner_user_id.as_str(),
            256_usize,
        ),
        (
            "Artifact Run id",
            descriptor.owner.run_id.as_str(),
            256_usize,
        ),
        (
            "Artifact device id",
            descriptor.owner.device_id.as_str(),
            256_usize,
        ),
        (
            "Artifact workspace id",
            descriptor.owner.workspace_id.as_str(),
            256_usize,
        ),
        (
            "Artifact Plugin id",
            descriptor.owner.plugin_id.as_str(),
            256_usize,
        ),
        (
            "Artifact Release id",
            descriptor.owner.release_id.as_str(),
            256_usize,
        ),
        (
            "Artifact component key",
            descriptor.owner.component_key.as_str(),
            256_usize,
        ),
        (
            "Artifact adapter session id",
            descriptor.owner.adapter_session_id.as_str(),
            256_usize,
        ),
        (
            "Artifact workspace-relative path",
            descriptor.workspace_relative_path.as_str(),
            4_096_usize,
        ),
        (
            "Artifact display name",
            descriptor.display_name.as_str(),
            512_usize,
        ),
        (
            "Artifact media type",
            descriptor.media_type.as_str(),
            256_usize,
        ),
        (
            "Artifact producer tool name",
            descriptor.producer_tool_name.as_str(),
            256_usize,
        ),
    ] {
        validate_bounded_identity(label, value, limit)?;
    }
    if !is_lower_sha256(descriptor.owner.artifact_sha256.as_str()) {
        bail!("Plugin Artifact registry owner package hash is invalid");
    }
    let relative = Path::new(descriptor.workspace_relative_path.as_str());
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        || relative.file_name().and_then(|value| value.to_str())
            != Some(descriptor.display_name.as_str())
        || artifact_media_type(relative) != Some(descriptor.media_type.as_str())
    {
        bail!("Plugin Artifact registry workspace path or media type is invalid");
    }
    let created_at = chrono::DateTime::parse_from_rfc3339(descriptor.created_at.as_str())
        .context("parse persisted Plugin Artifact creation time")?;
    if created_at.timestamp() != artifact.created_at_epoch_seconds {
        bail!("Plugin Artifact registry creation time does not match");
    }
    Ok(())
}

fn validate_unique_bounded_values(label: &str, values: &[String], limit: usize) -> Result<()> {
    let mut unique = BTreeSet::new();
    for value in values {
        validate_bounded_identity(label, value.as_str(), limit)?;
        if !unique.insert(value.as_str()) {
            bail!("Plugin Artifact registry {label} values must be unique");
        }
    }
    Ok(())
}

fn validate_bounded_identity(label: &str, value: &str, max_bytes: usize) -> Result<()> {
    if value.trim() != value
        || value.is_empty()
        || value.len() > max_bytes
        || value.chars().any(char::is_control)
    {
        bail!("Plugin Artifact registry {label} is invalid");
    }
    Ok(())
}

fn is_plugin_artifact_id(value: &str) -> bool {
    value.len() == 35
        && value.starts_with("pa_")
        && value.as_bytes().iter().skip(3).all(u8::is_ascii_hexdigit)
        && !value.as_bytes().iter().skip(3).any(u8::is_ascii_uppercase)
}

fn is_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value.as_bytes().iter().all(u8::is_ascii_hexdigit)
        && !value.as_bytes().iter().any(u8::is_ascii_uppercase)
}

fn output_candidates(arguments: &Value) -> Vec<String> {
    let Some(arguments) = arguments.as_object() else {
        return Vec::new();
    };
    ["target_path", "pdf_target_path"]
        .into_iter()
        .filter_map(|field| arguments.get(field).and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

fn is_artifact_skill(skill_id: &str) -> bool {
    matches!(
        skill_id,
        "internal_skill_documents"
            | "internal_skill_pdf"
            | "internal_skill_spreadsheets"
            | "internal_skill_presentations"
            | "internal_skill_template_creator"
    )
}

fn result_contains_string(value: &Value, expected: &str) -> bool {
    match value {
        Value::String(value) => value == expected,
        Value::Array(values) => values
            .iter()
            .any(|value| result_contains_string(value, expected)),
        Value::Object(values) => values
            .values()
            .any(|value| result_contains_string(value, expected)),
        _ => false,
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
mod tests {
    use super::*;
    use crate::WorkspaceState;
    use chatos_plugin_management_sdk::{
        PluginArtifactListRequest, PluginArtifactReadRequest,
        PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_CREATE, PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_DOWNLOAD,
        PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_LIST, PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_READ,
        PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_UPDATE, PLUGIN_UI_BRIDGE_PROTOCOL_VERSION_V1,
        PLUGIN_UI_HOST_CSP_V1, PLUGIN_UI_IFRAME_SANDBOX_V1,
    };
    use serde_json::json;
    use tempfile::TempDir;

    fn test_grant(access: &mut PluginArtifactUiAccess) -> PluginUiArtifactGrant {
        let mut ui = PluginUiSnapshot {
            plugin_id: access.plugin_id.clone(),
            release_id: access.release_id.clone(),
            version: "1.0.0".to_string(),
            artifact_sha256: access.artifact_sha256.clone(),
            component_key: access.component_key.clone(),
            title: "Workbench".to_string(),
            surface: "workbench".to_string(),
            relative_source_path: "./ui/index.html".to_string(),
            content_sha256: "c".repeat(64),
            assets: Vec::new(),
            bridge_protocol_version: PLUGIN_UI_BRIDGE_PROTOCOL_VERSION_V1,
            bridge_capabilities: vec![
                PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_LIST.to_string(),
                PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_READ.to_string(),
                PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_DOWNLOAD.to_string(),
                PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_CREATE.to_string(),
                PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_UPDATE.to_string(),
            ],
            artifact_mime_types: vec![
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
                    .to_string(),
                "application/json".to_string(),
            ],
            content_security_policy: PLUGIN_UI_HOST_CSP_V1.to_string(),
            iframe_sandbox: PLUGIN_UI_IFRAME_SANDBOX_V1.to_string(),
            snapshot_sha256: String::new(),
        };
        ui.snapshot_sha256 = plugin_ui_snapshot_sha256(
            ui.plugin_id.as_str(),
            ui.release_id.as_str(),
            ui.component_key.as_str(),
            ui.title.as_str(),
            ui.surface.as_str(),
            ui.relative_source_path.as_str(),
            ui.content_sha256.as_str(),
            ui.assets.as_slice(),
            ui.bridge_protocol_version,
            ui.bridge_capabilities.as_slice(),
            ui.artifact_mime_types.as_slice(),
            ui.content_security_policy.as_str(),
            ui.iframe_sandbox.as_str(),
        )
        .expect("hash UI snapshot");
        access.ui_snapshot_sha256 = ui.snapshot_sha256.clone();
        PluginUiArtifactGrant {
            owner_user_id: "owner-a".to_string(),
            device_id: "device-a".to_string(),
            workspace_id: "workspace-a".to_string(),
            run_id: access.run_id.clone(),
            plugin_id: access.plugin_id.clone(),
            release_id: access.release_id.clone(),
            artifact_sha256: access.artifact_sha256.clone(),
            component_key: access.component_key.clone(),
            adapter_session_id: access.adapter_session_id.clone(),
            ui,
            permission_snapshot: BTreeSet::new(),
            expires_at: Utc::now().timestamp() + 3_600,
        }
    }

    fn fixture() -> (
        TempDir,
        LocalState,
        RelayRequest,
        PluginArtifactStore,
        PluginArtifactUiAccess,
    ) {
        let temp = TempDir::new().expect("temp directory");
        let workspace = temp.path().join("workspace");
        fs::create_dir_all(workspace.join("artifacts")).expect("workspace");
        let state = LocalState {
            workspaces: vec![WorkspaceState {
                id: "workspace-a".to_string(),
                absolute_root: workspace,
                alias: "workspace-a".to_string(),
                fingerprint: "workspace-fingerprint".to_string(),
                project_config_trust: None,
            }],
            ..LocalState::default()
        };
        let request = RelayRequest {
            _message_type: "plugin_execute_request".to_string(),
            request_id: "request-a".to_string(),
            owner_user_id: Some("owner-a".to_string()),
            device_id: Some("device-a".to_string()),
            workspace_id: "workspace-a".to_string(),
            method: Some("POST".to_string()),
            path: Some("/plugins/execute".to_string()),
            headers: BTreeMap::new(),
            body: json!({}),
        };
        let mut access = PluginArtifactUiAccess {
            run_id: "run-a".to_string(),
            plugin_id: "plugin-a".to_string(),
            release_id: "release-a".to_string(),
            artifact_sha256: "a".repeat(64),
            component_key: "workbench".to_string(),
            adapter_session_id: "ui-session-a".to_string(),
            ui_snapshot_sha256: String::new(),
        };
        let store = PluginArtifactStore::default();
        store
            .register_ui_grant(test_grant(&mut access))
            .expect("register UI grant");
        (temp, state, request, store, access)
    }

    #[test]
    fn registers_lists_and_revalidates_exact_plugin_artifact_files() {
        let (_temp, state, request, store, access) = fixture();
        let workspace = &state.workspaces[0].absolute_root;
        fs::write(workspace.join("artifacts/report.docx"), b"docx fixture")
            .expect("write artifact");
        let descriptors = store
            .register_native_outputs(
                &state,
                &request,
                PluginArtifactProducer {
                    owner_user_id: "owner-a",
                    device_id: "device-a",
                    workspace_id: "workspace-a",
                    run_id: "run-a",
                    plugin_id: "plugin-a",
                    release_id: "release-a",
                    artifact_sha256: &"a".repeat(64),
                    component_key: "documents",
                    adapter_session_id: "native-session-a",
                    skill_id: "internal_skill_documents",
                    tool_name: "create_docx",
                },
                &json!({"target_path": "artifacts/report.docx"}),
                &json!({"created": true, "path": "artifacts/report.docx"}),
            )
            .expect("register Artifact");
        assert_eq!(descriptors.len(), 1);
        let grant = store
            .ui_grant(
                &request,
                &PluginArtifactListRequest {
                    access: access.clone(),
                }
                .access,
                PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_LIST,
            )
            .expect("UI grant");
        let listed = store.list(&grant, access.clone()).expect("list Artifacts");
        assert_eq!(listed.artifacts, descriptors);

        let read_request = PluginArtifactReadRequest {
            access: access.clone(),
            artifact_id: descriptors[0].artifact_id.clone(),
            mode: PluginArtifactReadMode::Download,
        };
        let read = store
            .read(
                &state,
                &request,
                &grant,
                read_request.access,
                read_request.artifact_id.as_str(),
                read_request.mode,
            )
            .expect("read Artifact");
        assert_eq!(
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, read.body_base64,)
                .expect("decode Artifact"),
            b"docx fixture"
        );

        fs::write(workspace.join("artifacts/report.docx"), b"tampered").expect("tamper Artifact");
        assert!(store
            .read(
                &state,
                &request,
                &grant,
                access,
                descriptors[0].artifact_id.as_str(),
                PluginArtifactReadMode::Download,
            )
            .is_err());
    }

    #[test]
    fn creates_and_optimistically_updates_ui_owned_mutable_artifacts() {
        let (temp, state, request, _ephemeral_store, access) = fixture();
        let state_path = temp.path().join("state.json");
        let storage = SecureStorage::in_memory("Plugin Artifact write test");
        let store =
            PluginArtifactStore::for_state_path_with_storage(state_path.as_path(), storage.clone());
        let mut write_access = access.clone();
        store
            .register_ui_grant(test_grant(&mut write_access))
            .expect("persist write grant");
        let grant = store
            .ui_grant(
                &request,
                &write_access,
                PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_CREATE,
            )
            .expect("create grant");
        fs::write(
            state.workspaces[0]
                .absolute_root
                .join("artifacts/native.docx"),
            b"immutable native artifact",
        )
        .expect("write immutable native Artifact");
        let immutable = store
            .register_native_outputs(
                &state,
                &request,
                PluginArtifactProducer {
                    owner_user_id: "owner-a",
                    device_id: "device-a",
                    workspace_id: "workspace-a",
                    run_id: "run-a",
                    plugin_id: "plugin-a",
                    release_id: "release-a",
                    artifact_sha256: &"a".repeat(64),
                    component_key: "documents",
                    adapter_session_id: "native-session-a",
                    skill_id: "internal_skill_documents",
                    tool_name: "create_docx",
                },
                &json!({"target_path": "artifacts/native.docx"}),
                &json!({"created": true, "path": "artifacts/native.docx"}),
            )
            .expect("register immutable native Artifact");
        let immutable_update = store
            .update(
                &state,
                &request,
                &grant,
                write_access.clone(),
                immutable[0].artifact_id.as_str(),
                immutable[0].sha256.as_str(),
                b"must not overwrite native Artifact",
            )
            .expect_err("immutable native Artifact update must fail");
        assert_eq!(immutable_update.0, 409);
        let created = store
            .create(
                &state,
                &request,
                &grant,
                write_access.clone(),
                "report.json",
                "application/json",
                br#"{"version":1}"#,
            )
            .expect("create mutable Artifact");
        assert_eq!(created.operation, PluginArtifactWriteOperation::Create);
        assert!(created.artifact.mutable);
        assert_eq!(
            created.artifact.owner.adapter_session_id,
            write_access.adapter_session_id
        );
        assert!(created
            .artifact
            .workspace_relative_path
            .starts_with("chatos-plugin-artifacts/"));
        assert!(!created.artifact.workspace_relative_path.contains("owner-a"));

        let stale = store
            .update(
                &state,
                &request,
                &grant,
                write_access.clone(),
                created.artifact.artifact_id.as_str(),
                &"0".repeat(64),
                br#"{"version":2}"#,
            )
            .expect_err("stale update must fail");
        assert_eq!(stale.0, 409);

        let updated = store
            .update(
                &state,
                &request,
                &grant,
                write_access.clone(),
                created.artifact.artifact_id.as_str(),
                created.artifact.sha256.as_str(),
                br#"{"version":2}"#,
            )
            .expect("update mutable Artifact");
        assert_eq!(updated.operation, PluginArtifactWriteOperation::Update);
        assert_ne!(updated.artifact.sha256, created.artifact.sha256);
        assert_eq!(
            updated.artifact.producer_tool_name,
            PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_UPDATE
        );
        drop(store);

        let restored =
            PluginArtifactStore::for_state_path_with_storage(state_path.as_path(), storage);
        let restored_grant = restored
            .ui_grant(
                &request,
                &write_access,
                PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_READ,
            )
            .expect("restore write grant");
        let read = restored
            .read(
                &state,
                &request,
                &restored_grant,
                write_access,
                updated.artifact.artifact_id.as_str(),
                PluginArtifactReadMode::Inline,
            )
            .expect("read restored mutable Artifact");
        assert_eq!(read.artifact, updated.artifact);
        assert_eq!(
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, read.body_base64,)
                .expect("decode updated Artifact"),
            br#"{"version":2}"#
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symbolic_links_during_registration() {
        use std::os::unix::fs::symlink;

        let (_temp, state, request, store, _access) = fixture();
        let workspace = &state.workspaces[0].absolute_root;
        fs::write(workspace.join("outside.docx"), b"outside").expect("write source");
        symlink(
            workspace.join("outside.docx"),
            workspace.join("artifacts/link.docx"),
        )
        .expect("create symlink");
        let error = store
            .register_native_outputs(
                &state,
                &request,
                PluginArtifactProducer {
                    owner_user_id: "owner-a",
                    device_id: "device-a",
                    workspace_id: "workspace-a",
                    run_id: "run-a",
                    plugin_id: "plugin-a",
                    release_id: "release-a",
                    artifact_sha256: &"a".repeat(64),
                    component_key: "documents",
                    adapter_session_id: "native-session-a",
                    skill_id: "internal_skill_documents",
                    tool_name: "create_docx",
                },
                &json!({"target_path": "artifacts/link.docx"}),
                &json!({"created": true, "path": "artifacts/link.docx"}),
            )
            .expect_err("symlink must fail");
        assert!(error.to_string().contains("symbolic link"));
    }

    #[test]
    fn persists_and_restores_exact_artifact_registry_state() {
        let (temp, state, request, _ephemeral_store, access) = fixture();
        let state_path = temp.path().join("state.json");
        let storage = SecureStorage::in_memory("Plugin Artifact registry test");
        let store =
            PluginArtifactStore::for_state_path_with_storage(state_path.as_path(), storage.clone());
        let mut persisted_access = access.clone();
        store
            .register_ui_grant(test_grant(&mut persisted_access))
            .expect("persist UI grant");
        let workspace = &state.workspaces[0].absolute_root;
        fs::write(
            workspace.join("artifacts/restored.docx"),
            b"restart fixture",
        )
        .expect("write persisted Artifact");
        let descriptors = store
            .register_native_outputs(
                &state,
                &request,
                PluginArtifactProducer {
                    owner_user_id: "owner-a",
                    device_id: "device-a",
                    workspace_id: "workspace-a",
                    run_id: "run-a",
                    plugin_id: "plugin-a",
                    release_id: "release-a",
                    artifact_sha256: &"a".repeat(64),
                    component_key: "documents",
                    adapter_session_id: "native-session-a",
                    skill_id: "internal_skill_documents",
                    tool_name: "create_docx",
                },
                &json!({"target_path": "artifacts/restored.docx"}),
                &json!({"created": true, "path": "artifacts/restored.docx"}),
            )
            .expect("persist Artifact");
        assert_eq!(descriptors.len(), 1);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let registry_path = temp
                .path()
                .join("plugins")
                .join(PLUGIN_ARTIFACT_REGISTRY_FILE_NAME);
            let mode = fs::metadata(registry_path)
                .expect("registry metadata")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }
        drop(store);

        let restored =
            PluginArtifactStore::for_state_path_with_storage(state_path.as_path(), storage.clone());
        let grant = restored
            .ui_grant(
                &request,
                &persisted_access,
                PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_LIST,
            )
            .expect("restore UI grant");
        assert_eq!(
            restored
                .list(&grant, persisted_access.clone())
                .expect("list restored Artifacts")
                .artifacts,
            descriptors
        );
        let read = restored
            .read(
                &state,
                &request,
                &grant,
                persisted_access,
                descriptors[0].artifact_id.as_str(),
                PluginArtifactReadMode::Download,
            )
            .expect("read restored Artifact");
        assert_eq!(
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, read.body_base64,)
                .expect("decode restored Artifact"),
            b"restart fixture"
        );
    }

    #[test]
    fn rejects_tampered_persisted_artifact_registry() {
        let (temp, _state, request, _ephemeral_store, access) = fixture();
        let state_path = temp.path().join("state.json");
        let storage = SecureStorage::in_memory("Plugin Artifact registry tamper test");
        let store =
            PluginArtifactStore::for_state_path_with_storage(state_path.as_path(), storage.clone());
        let mut persisted_access = access.clone();
        store
            .register_ui_grant(test_grant(&mut persisted_access))
            .expect("persist UI grant");
        drop(store);

        let registry_path = temp
            .path()
            .join("plugins")
            .join(PLUGIN_ARTIFACT_REGISTRY_FILE_NAME);
        let original = fs::read_to_string(registry_path.as_path()).expect("read registry");
        let tampered = original.replacen("owner-a", "owner-z", 1);
        assert_ne!(tampered, original);
        fs::write(registry_path.as_path(), tampered).expect("tamper registry");

        let restored =
            PluginArtifactStore::for_state_path_with_storage(state_path.as_path(), storage);
        let error = restored
            .ui_grant(
                &request,
                &persisted_access,
                PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_LIST,
            )
            .expect_err("tampered registry must fail closed");
        assert_eq!(error.0, 500);
        assert!(error.1.contains("integrity verification failed"));
    }

    #[test]
    fn expired_ui_grants_are_not_restored() {
        let (temp, _state, request, _ephemeral_store, access) = fixture();
        let state_path = temp.path().join("state.json");
        let storage = SecureStorage::in_memory("Plugin Artifact registry expiry test");
        let persistence =
            PluginArtifactPersistence::open(state_path.as_path(), &storage).expect("persistence");
        let mut expired_access = access.clone();
        let mut expired_grant = test_grant(&mut expired_access);
        expired_grant.expires_at = Utc::now().timestamp() - 1;
        let mut state = PluginArtifactStoreState::default();
        state
            .ui_grants
            .insert(expired_grant.adapter_session_id.clone(), expired_grant);
        persistence.save(&state).expect("persist pruned registry");

        let restored =
            PluginArtifactStore::for_state_path_with_storage(state_path.as_path(), storage);
        let error = restored
            .ui_grant(
                &request,
                &expired_access,
                PLUGIN_UI_BRIDGE_CAPABILITY_ARTIFACT_LIST,
            )
            .expect_err("expired grant must not restore");
        assert_eq!(error.0, 404);
    }
}
