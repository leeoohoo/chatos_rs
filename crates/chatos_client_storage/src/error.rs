// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::PathBuf;

use thiserror::Error;

pub type StorageResult<T> = Result<T, StorageConfigurationError>;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum StorageConfigurationError {
    #[error("SQLite database path must be absolute: {0}")]
    SqlitePathNotAbsolute(PathBuf),
    #[error("{field} must not be empty")]
    EmptyField { field: &'static str },
    #[error("PostgreSQL port must be greater than zero")]
    InvalidPostgresPort,
    #[error("PostgreSQL connections to non-loopback hosts require verified TLS")]
    TlsRequiredForRemotePostgres,
}
