// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;

use crate::{
    BootstrapStorageProfile, ClientStorage, PostgresBootstrapProfile, PostgresClientStorage,
    PostgresConnectionSettings, SecretReference, SqliteBootstrapProfile, SqliteClientStorage,
    StorageResult,
};

/// Resolves an opaque bootstrap reference through the platform secure store.
/// Implementations must use Keychain on macOS and Credential Manager/DPAPI on
/// Windows; the resolved value must never be persisted by this crate.
#[async_trait]
pub trait StorageSecretResolver: Send + Sync {
    async fn resolve_postgres(
        &self,
        reference: &SecretReference,
    ) -> StorageResult<PostgresConnectionSettings>;
}

/// Opens a concrete backend. This seam keeps selection policy testable while
/// the native implementation remains the only production connector.
#[async_trait]
pub trait StorageBackendConnector: Send + Sync {
    async fn open_sqlite(
        &self,
        profile: &SqliteBootstrapProfile,
    ) -> StorageResult<Box<dyn ClientStorage>>;

    async fn open_postgres(
        &self,
        settings: &PostgresConnectionSettings,
    ) -> StorageResult<Box<dyn ClientStorage>>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct NativeStorageBackendConnector;

#[async_trait]
impl StorageBackendConnector for NativeStorageBackendConnector {
    async fn open_sqlite(
        &self,
        profile: &SqliteBootstrapProfile,
    ) -> StorageResult<Box<dyn ClientStorage>> {
        Ok(Box::new(SqliteClientStorage::open(profile).await?))
    }

    async fn open_postgres(
        &self,
        settings: &PostgresConnectionSettings,
    ) -> StorageResult<Box<dyn ClientStorage>> {
        Ok(Box::new(PostgresClientStorage::open(settings).await?))
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ClientStorageFactory;

impl ClientStorageFactory {
    pub async fn open(
        profile: &BootstrapStorageProfile,
        secrets: &dyn StorageSecretResolver,
    ) -> StorageResult<Box<dyn ClientStorage>> {
        Self::open_with(profile, secrets, &NativeStorageBackendConnector).await
    }

    pub async fn open_with(
        profile: &BootstrapStorageProfile,
        secrets: &dyn StorageSecretResolver,
        connector: &dyn StorageBackendConnector,
    ) -> StorageResult<Box<dyn ClientStorage>> {
        match profile {
            BootstrapStorageProfile::Sqlite(profile) => connector.open_sqlite(profile).await,
            BootstrapStorageProfile::Postgres(PostgresBootstrapProfile { connection_secret }) => {
                let settings = secrets.resolve_postgres(connection_secret).await?;
                connector.open_postgres(&settings).await
            }
        }
    }
}
