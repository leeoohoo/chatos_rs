// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use chatos_client_storage::{
    BootstrapStorageProfile, ClientStorage, ClientStorageFactory, PostgresBootstrapProfile,
    PostgresConnectionSettings, PostgresCredentials, PostgresEndpoint, PostgresTlsMode,
    SecretReference, SqliteBootstrapProfile, StorageBackend, StorageBackendConnector,
    StorageEncryptionKey, StorageError, StorageResult, StorageSecretResolver, StorageTransaction,
};

#[derive(Default)]
struct Resolver {
    sqlite_key_calls: AtomicUsize,
    postgres_calls: AtomicUsize,
}

#[async_trait]
impl StorageSecretResolver for Resolver {
    async fn resolve_sqlite_encryption_key(
        &self,
        _reference: &SecretReference,
    ) -> StorageResult<StorageEncryptionKey> {
        self.sqlite_key_calls.fetch_add(1, Ordering::SeqCst);
        Ok(StorageEncryptionKey::new([42; 32]))
    }

    async fn resolve_postgres(
        &self,
        _reference: &SecretReference,
    ) -> StorageResult<PostgresConnectionSettings> {
        self.postgres_calls.fetch_add(1, Ordering::SeqCst);
        Ok(PostgresConnectionSettings {
            endpoint: PostgresEndpoint {
                host: "localhost".to_string(),
                port: 5432,
                database: "chatos".to_string(),
                tls_mode: PostgresTlsMode::Disabled,
            },
            credentials: PostgresCredentials::new("chatos", "secret").unwrap(),
        })
    }
}

#[derive(Default)]
struct Connector {
    sqlite_calls: AtomicUsize,
    postgres_calls: AtomicUsize,
    fail_postgres: bool,
}

#[async_trait]
impl StorageBackendConnector for Connector {
    async fn open_sqlite(
        &self,
        _profile: &SqliteBootstrapProfile,
        _encryption_key: &StorageEncryptionKey,
    ) -> StorageResult<Box<dyn ClientStorage>> {
        self.sqlite_calls.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(SelectedStorage(StorageBackend::Sqlite)))
    }

    async fn open_postgres(
        &self,
        _settings: &PostgresConnectionSettings,
    ) -> StorageResult<Box<dyn ClientStorage>> {
        self.postgres_calls.fetch_add(1, Ordering::SeqCst);
        if self.fail_postgres {
            return Err(StorageError::Unavailable {
                reason: "PostgreSQL unavailable".to_string(),
            });
        }
        Ok(Box::new(SelectedStorage(StorageBackend::Postgres)))
    }
}

struct SelectedStorage(StorageBackend);

#[async_trait]
impl ClientStorage for SelectedStorage {
    fn backend(&self) -> StorageBackend {
        self.0
    }

    async fn transaction(&self, _operation: &mut dyn StorageTransaction) -> StorageResult<()> {
        unreachable!("provider-selection tests do not execute transactions")
    }
}

#[tokio::test]
async fn sqlite_selection_never_reads_postgres_secrets() {
    let resolver = Resolver::default();
    let connector = Connector::default();
    let profile = BootstrapStorageProfile::Sqlite(SqliteBootstrapProfile {
        database_path: PathBuf::from("/tmp/chatos.sqlite3"),
        encryption_secret: SecretReference::new("keychain:client-storage/sqlite-key").unwrap(),
    });

    let storage = ClientStorageFactory::open_with(&profile, &resolver, &connector)
        .await
        .unwrap();

    assert_eq!(storage.backend(), StorageBackend::Sqlite);
    assert_eq!(resolver.sqlite_key_calls.load(Ordering::SeqCst), 1);
    assert_eq!(resolver.postgres_calls.load(Ordering::SeqCst), 0);
    assert_eq!(connector.sqlite_calls.load(Ordering::SeqCst), 1);
    assert_eq!(connector.postgres_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn postgres_failure_is_returned_without_sqlite_fallback() {
    let resolver = Resolver::default();
    let connector = Connector {
        fail_postgres: true,
        ..Connector::default()
    };
    let profile = BootstrapStorageProfile::Postgres(PostgresBootstrapProfile {
        connection_secret: SecretReference::new("keychain:client-storage/main").unwrap(),
    });

    let result = ClientStorageFactory::open_with(&profile, &resolver, &connector).await;

    assert!(matches!(result, Err(StorageError::Unavailable { .. })));
    assert_eq!(resolver.sqlite_key_calls.load(Ordering::SeqCst), 0);
    assert_eq!(resolver.postgres_calls.load(Ordering::SeqCst), 1);
    assert_eq!(connector.postgres_calls.load(Ordering::SeqCst), 1);
    assert_eq!(connector.sqlite_calls.load(Ordering::SeqCst), 0);
}
