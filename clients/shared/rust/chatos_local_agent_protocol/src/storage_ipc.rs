// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};

use crate::{require_digest, require_identifier, ProtocolError};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClientStorageBackendKind {
    Sqlite,
    Postgres,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClientStorageHealth {
    Active,
    RestartRequired,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ClientStorageProfileDescriptor {
    pub backend: ClientStorageBackendKind,
    pub health: ClientStorageHealth,
    pub sqlite_database_reference: Option<String>,
    pub postgres_connection_secret_reference: Option<String>,
    pub schema_version: u32,
    pub last_error_code: Option<String>,
}

impl ClientStorageProfileDescriptor {
    pub(crate) fn validate(&self) -> Result<(), ProtocolError> {
        if self.schema_version == 0 {
            return Err(ProtocolError::InvalidState {
                reason: "storage schema version must be positive",
            });
        }
        match self.backend {
            ClientStorageBackendKind::Sqlite => {
                require_identifier(
                    "sqlite_database_reference",
                    self.sqlite_database_reference
                        .as_deref()
                        .unwrap_or_default(),
                )?;
                if self.postgres_connection_secret_reference.is_some() {
                    return Err(ProtocolError::InvalidState {
                        reason: "SQLite profile cannot include a PostgreSQL secret reference",
                    });
                }
            }
            ClientStorageBackendKind::Postgres => {
                require_identifier(
                    "postgres_connection_secret_reference",
                    self.postgres_connection_secret_reference
                        .as_deref()
                        .unwrap_or_default(),
                )?;
                if self.sqlite_database_reference.is_some() {
                    return Err(ProtocolError::InvalidState {
                        reason: "PostgreSQL profile cannot include a SQLite database reference",
                    });
                }
            }
        }
        if let Some(code) = &self.last_error_code {
            require_identifier("storage_error_code", code)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "backend", rename_all = "snake_case")]
pub enum ClientStorageProfileSelection {
    Sqlite {
        database_reference: String,
        encryption_secret_reference: String,
    },
    Postgres {
        connection_secret_reference: String,
    },
}

impl ClientStorageProfileSelection {
    pub(crate) fn validate(&self) -> Result<(), ProtocolError> {
        match self {
            Self::Sqlite {
                database_reference,
                encryption_secret_reference,
            } => {
                require_identifier("database_reference", database_reference)?;
                require_identifier("encryption_secret_reference", encryption_secret_reference)
            }
            Self::Postgres {
                connection_secret_reference,
            } => require_identifier("connection_secret_reference", connection_secret_reference),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PostgresConnectionTestCommand {
    pub connection_secret_reference: String,
}

impl PostgresConnectionTestCommand {
    pub(crate) fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier(
            "connection_secret_reference",
            &self.connection_secret_reference,
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PostgresConnectionTestResult {
    pub server_version: String,
    pub tls_active: bool,
    pub authentication_ok: bool,
    pub transaction_ok: bool,
    pub migration_permission_ok: bool,
}

impl PostgresConnectionTestResult {
    pub(crate) fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("postgres_server_version", &self.server_version)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ApplyStorageProfileCommand {
    pub profile: ClientStorageProfileSelection,
    pub confirm_no_active_runs: bool,
}

impl ApplyStorageProfileCommand {
    pub(crate) fn validate(&self) -> Result<(), ProtocolError> {
        if !self.confirm_no_active_runs {
            return Err(ProtocolError::InvalidState {
                reason: "applying a storage profile requires explicit active Run confirmation",
            });
        }
        self.profile.validate()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ExportClientDataCommand {
    pub destination_reference: String,
    pub include_large_payload_references: bool,
}

impl ExportClientDataCommand {
    pub(crate) fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("export_destination_reference", &self.destination_reference)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ImportClientDataCommand {
    pub source_reference: String,
    pub expected_archive_digest: String,
    pub confirm_no_active_runs: bool,
}

impl ImportClientDataCommand {
    pub(crate) fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("import_source_reference", &self.source_reference)?;
        require_digest("expected_archive_digest", &self.expected_archive_digest)?;
        if !self.confirm_no_active_runs {
            return Err(ProtocolError::InvalidState {
                reason: "import requires explicit active Run confirmation",
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ClientDataTransferResult {
    pub archive_reference: String,
    pub archive_digest: String,
    pub record_count: u64,
}

impl ClientDataTransferResult {
    pub(crate) fn validate(&self) -> Result<(), ProtocolError> {
        require_identifier("archive_reference", &self.archive_reference)?;
        require_digest("archive_digest", &self.archive_digest)
    }
}
