// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

//! Cross-platform storage contracts for ChatOS clients.
//!
//! This crate is the only entry point through which client business modules
//! select a structured-data backend. Bootstrap profiles contain references to
//! secrets, never PostgreSQL passwords or connection strings.

mod bootstrap;
mod connection;
mod error;

pub use bootstrap::{
    BootstrapStorageProfile, PostgresBootstrapProfile, SecretReference, SqliteBootstrapProfile,
    StorageBackend,
};
pub use connection::{
    PostgresConnectionSettings, PostgresCredentials, PostgresEndpoint, PostgresTlsMode,
};
pub use error::{StorageConfigurationError, StorageResult};
