// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use chatos_sandbox_contract::{
    merge_codex_permission_profile_document_layers, parse_codex_permission_profile_toml,
    parse_managed_requirements_toml, CodexPermissionProfileDocument,
};

use super::RuntimePermissionProfileLayers;
use crate::config::{home_dir, optional_env};

const MAX_PERMISSION_CONFIG_BYTES: u64 = 1024 * 1024;
const SYSTEM_CONFIG_ENV: &str = "LOCAL_CONNECTOR_SYSTEM_PERMISSIONS_CONFIG";
const USER_CONFIG_ENV: &str = "LOCAL_CONNECTOR_USER_PERMISSIONS_CONFIG";

#[derive(Debug, Clone)]
pub(super) struct ConfigPath {
    pub(super) path: Option<PathBuf>,
    pub(super) required: bool,
    pub(super) secure_system_file: bool,
}

impl RuntimePermissionProfileLayers {
    pub(crate) fn blocked(message: impl Into<String>) -> Self {
        Self {
            load_error: Some(message.into()),
            ..Default::default()
        }
    }

    pub(crate) fn load_from_environment_with_cloud_managed(
        cloud_managed: Option<CodexPermissionProfileDocument>,
    ) -> Result<Self> {
        let system =
            permission_config_path(SYSTEM_CONFIG_ENV, default_system_permission_config_path());
        let user = permission_config_path(USER_CONFIG_ENV, default_user_permission_config_path());
        let managed = ConfigPath {
            path: default_managed_permission_config_path(),
            required: false,
            secure_system_file: true,
        };
        let mut layers = Self::load_from_paths(system, user, managed)?;
        layers.managed = merge_optional_documents(layers.managed, cloud_managed)?;
        Ok(layers)
    }

    pub(super) fn load_from_paths(
        system: ConfigPath,
        user: ConfigPath,
        managed: ConfigPath,
    ) -> Result<Self> {
        Ok(Self {
            system: load_permission_document("system permission config", &system, false)?,
            user: load_permission_document("user permission config", &user, false)?,
            managed: load_permission_document("managed permission requirements", &managed, true)?,
            load_error: None,
        })
    }
}

fn permission_config_path(environment_key: &str, default: Option<PathBuf>) -> ConfigPath {
    match optional_env(environment_key) {
        Some(path) => ConfigPath {
            path: Some(PathBuf::from(path)),
            required: true,
            secure_system_file: false,
        },
        None => ConfigPath {
            path: default,
            required: false,
            secure_system_file: false,
        },
    }
}

#[cfg(not(windows))]
fn default_system_permission_config_path() -> Option<PathBuf> {
    Some(PathBuf::from("/etc/chatos/config.toml"))
}

#[cfg(windows)]
fn default_system_permission_config_path() -> Option<PathBuf> {
    std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .map(|path| {
            path.join("ChatOS")
                .join("LocalConnector")
                .join("config.toml")
        })
}

fn default_user_permission_config_path() -> Option<PathBuf> {
    home_dir().map(|path| {
        path.join(".chatos")
            .join("local_connector")
            .join("config.toml")
    })
}

#[cfg(not(windows))]
fn default_managed_permission_config_path() -> Option<PathBuf> {
    Some(PathBuf::from("/etc/chatos/requirements.toml"))
}

#[cfg(windows)]
fn default_managed_permission_config_path() -> Option<PathBuf> {
    std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .map(|path| {
            path.join("ChatOS")
                .join("LocalConnector")
                .join("requirements.toml")
        })
}

pub(super) fn load_permission_document(
    label: &str,
    configured_path: &ConfigPath,
    managed_requirements: bool,
) -> Result<Option<CodexPermissionProfileDocument>> {
    let Some(path) = configured_path.path.as_deref() else {
        return Ok(None);
    };
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound && !configured_path.required => {
            return Ok(None)
        }
        Err(err) => {
            return Err(err).with_context(|| format!("read {label} metadata {}", path.display()))
        }
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(anyhow!(
            "{label} {} is not a regular non-symlink file",
            path.display()
        ));
    }
    validate_secure_system_file(label, path, &metadata, configured_path.secure_system_file)?;
    if metadata.len() > MAX_PERMISSION_CONFIG_BYTES {
        return Err(anyhow!(
            "{label} {} exceeds the 1 MiB limit",
            path.display()
        ));
    }
    let source =
        fs::read_to_string(path).with_context(|| format!("read {label} {}", path.display()))?;
    if source.len() as u64 > MAX_PERMISSION_CONFIG_BYTES {
        return Err(anyhow!(
            "{label} {} exceeds the 1 MiB limit",
            path.display()
        ));
    }
    let parsed = if managed_requirements {
        parse_managed_requirements_toml(source.as_str())
    } else {
        parse_codex_permission_profile_toml(source.as_str())
    };
    parsed
        .map(Some)
        .map_err(anyhow::Error::msg)
        .with_context(|| format!("parse {label} {}", path.display()))
}

#[cfg(unix)]
fn validate_secure_system_file(
    label: &str,
    path: &std::path::Path,
    metadata: &fs::Metadata,
    required: bool,
) -> Result<()> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    if !required {
        return Ok(());
    }
    if metadata.uid() != 0 {
        return Err(anyhow!("{label} {} must be owned by root", path.display()));
    }
    if metadata.permissions().mode() & 0o022 != 0 {
        return Err(anyhow!(
            "{label} {} must not be group- or world-writable",
            path.display()
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn validate_secure_system_file(
    label: &str,
    path: &std::path::Path,
    _metadata: &fs::Metadata,
    required: bool,
) -> Result<()> {
    if required {
        crate::sandbox::windows_security::validate_windows_secure_system_path(path, label)?;
    }
    Ok(())
}

#[cfg(all(not(unix), not(windows)))]
fn validate_secure_system_file(
    label: &str,
    path: &std::path::Path,
    _metadata: &fs::Metadata,
    required: bool,
) -> Result<()> {
    if required {
        return Err(anyhow!(
            "{label} {} cannot be trusted until platform ACL validation is available",
            path.display()
        ));
    }
    Ok(())
}

pub(super) fn merge_optional_documents(
    lower: Option<CodexPermissionProfileDocument>,
    higher: Option<CodexPermissionProfileDocument>,
) -> Result<Option<CodexPermissionProfileDocument>> {
    match (lower, higher) {
        (Some(lower), Some(higher)) => Ok(Some(merge_codex_permission_profile_document_layers(
            lower, higher,
        ))),
        (Some(document), None) | (None, Some(document)) => Ok(Some(document)),
        (None, None) => Ok(None),
    }
}
