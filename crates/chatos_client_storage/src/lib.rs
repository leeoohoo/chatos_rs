// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

//! Cross-platform storage contracts for ChatOS clients.
//!
//! This crate is the only entry point through which client business modules
//! select a structured-data backend. Bootstrap profiles contain references to
//! secrets, never PostgreSQL passwords or connection strings.

mod bootstrap;
mod connection;
mod contracts;
mod error;
mod repositories;
mod transaction;

pub use bootstrap::{
    BootstrapStorageProfile, PostgresBootstrapProfile, SecretReference, SqliteBootstrapProfile,
    StorageBackend,
};
pub use connection::{
    PostgresConnectionSettings, PostgresCredentials, PostgresEndpoint, PostgresTlsMode,
};
pub use contracts::*;
pub use error::{ConfigurationResult, StorageConfigurationError, StorageError, StorageResult};
pub use repositories::*;
pub use transaction::{ClientStorage, StorageTransaction, TransactionRepositories};
