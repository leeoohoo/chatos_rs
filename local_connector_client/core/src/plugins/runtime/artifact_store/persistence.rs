// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl PluginArtifactPersistence {
    pub(super) fn open(state_path: &Path, storage: &SecureStorage) -> Result<Self> {
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

    pub(super) fn load(&self) -> Result<PluginArtifactStoreState> {
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
            || !super::validation::is_lower_sha256(persisted.integrity_tag.as_str())
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
        super::validation::validate_persisted_state(&mut state)?;
        Ok(state)
    }

    pub(super) fn save(&self, state: &PluginArtifactStoreState) -> Result<()> {
        let mut state = state.clone();
        super::validation::validate_persisted_state(&mut state)?;
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
pub(super) fn sync_registry_directory(path: &Path) -> Result<()> {
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
pub(super) fn sync_registry_directory(_path: &Path) -> Result<()> {
    Ok(())
}
