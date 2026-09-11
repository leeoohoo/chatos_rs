// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::PathBuf;

use thiserror::Error;

pub type ConfigurationResult<T> = Result<T, StorageConfigurationError>;
pub type StorageResult<T> = Result<T, StorageError>;

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

#[derive(Debug, Error, PartialEq, Eq)]
pub enum StorageError {
    #[error("client storage is unavailable: {reason}")]
    Unavailable { reason: String },
    #[error("client storage migration {version} failed: {reason}")]
    Migration { version: u32, reason: String },
    #[error("client storage record conflicts with revision {actual_revision}")]
    Conflict { actual_revision: u64 },
    #[error("client storage record was not found")]
    NotFound,
    #[error("client storage rejected invalid data: {reason}")]
    InvalidData { reason: String },
    #[error("client storage transaction failed: {reason}")]
    Transaction { reason: String },
}
