// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{ConfigurationResult, StorageConfigurationError};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StorageBackend {
    Sqlite,
    Postgres,
}

/// The non-secret profile read before the client storage provider is opened.
///
/// Only one variant can be active, which makes dual writes and implicit
/// database fallback impossible at the configuration boundary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "backend", rename_all = "snake_case")]
pub enum BootstrapStorageProfile {
    Sqlite(SqliteBootstrapProfile),
    Postgres(PostgresBootstrapProfile),
}

impl BootstrapStorageProfile {
    pub fn backend(&self) -> StorageBackend {
        match self {
            Self::Sqlite(_) => StorageBackend::Sqlite,
            Self::Postgres(_) => StorageBackend::Postgres,
        }
    }

    pub fn validate(&self) -> ConfigurationResult<()> {
        match self {
            Self::Sqlite(profile) => profile.validate(),
            Self::Postgres(profile) => profile.validate(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SqliteBootstrapProfile {
    pub database_path: PathBuf,
    /// Opaque reference to the 256-bit device key held by the platform secure
    /// store. The key itself never enters this profile.
    pub encryption_secret: SecretReference,
}

impl SqliteBootstrapProfile {
    pub fn validate(&self) -> ConfigurationResult<()> {
        if !self.database_path.is_absolute() {
            return Err(StorageConfigurationError::SqlitePathNotAbsolute(
                self.database_path.clone(),
            ));
        }
        self.encryption_secret.validate()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PostgresBootstrapProfile {
    /// Opaque identifier resolved through Keychain or Windows Credential
    /// Manager. It is not a DSN and does not contain connection material.
    pub connection_secret: SecretReference,
}

impl PostgresBootstrapProfile {
    pub fn validate(&self) -> ConfigurationResult<()> {
        self.connection_secret.validate()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct SecretReference(String);

impl SecretReference {
    pub fn new(value: impl Into<String>) -> ConfigurationResult<Self> {
        let reference = Self(value.into());
        reference.validate()?;
        Ok(reference)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn validate(&self) -> ConfigurationResult<()> {
        if self.0.trim().is_empty() {
            return Err(StorageConfigurationError::EmptyField {
                field: "connection_secret",
            });
        }
        Ok(())
    }
}
