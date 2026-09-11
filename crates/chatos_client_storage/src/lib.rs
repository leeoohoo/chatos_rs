// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

//! Cross-platform storage contracts for ChatOS clients.
//!
//! This crate is the only entry point through which client business modules
//! select a structured-data backend. Bootstrap profiles contain references to
//! secrets, never PostgreSQL passwords or connection strings.

mod archive;
mod bootstrap;
mod connection;
mod contracts;
mod error;
mod factory;
mod postgres;
mod record_store;
mod repositories;
mod sqlite;
mod sqlite_cipher;
mod transaction;

pub use archive::{
    decode_storage_archive, encode_storage_archive, export_storage_archive, import_storage_archive,
    ClientStorageArchive, StorageArchiveRecords,
};
pub use bootstrap::{
    BootstrapStorageProfile, PostgresBootstrapProfile, SecretReference, SqliteBootstrapProfile,
    StorageBackend,
};
pub use connection::{
    PostgresConnectionSettings, PostgresCredentials, PostgresEndpoint, PostgresTlsMode,
    StorageEncryptionKey,
};
pub use contracts::*;
pub use error::{ConfigurationResult, StorageConfigurationError, StorageError, StorageResult};
pub use factory::{
    ClientStorageFactory, NativeStorageBackendConnector, StorageBackendConnector,
    StorageSecretResolver,
};
pub use postgres::PostgresClientStorage;
pub use repositories::*;
pub use sqlite::SqliteClientStorage;
pub use transaction::{ClientStorage, StorageTransaction, TransactionRepositories};
